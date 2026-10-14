"""Converts binary FBX models to .glb, which the game can load.

Usage:
    python tools/fbx2glb.py <file-or-folder>... [--texture path/to/atlas.png]

Each .fbx becomes a .glb beside it. Only the standard library is used.

What carries over: every mesh, placed with its node transforms (translation,
rotation, scale, pre-rotation) and the file's unit scale, converted to meters;
texture coordinates; and, with --texture, that image embedded as the base
color texture. Everything else (animation, skinning, normals, cameras, lights)
is dropped. Polygons are triangulated as fans.
"""

import argparse
import json
import math
import struct
import sys
import zlib
from pathlib import Path

# ---------------------------------------------------------------------------
# Binary FBX reading
# ---------------------------------------------------------------------------

MAGIC = b"Kaydara FBX Binary  \x00"


class Node:
    def __init__(self, name, props, children):
        self.name = name
        self.props = props
        self.children = children

    def child(self, name):
        return next((c for c in self.children if c.name == name), None)

    def all(self, name):
        return [c for c in self.children if c.name == name]


def _read_array(data, pos, fmt):
    length, encoding, comp_len = struct.unpack_from("<III", data, pos)
    pos += 12
    raw = data[pos:pos + comp_len]
    pos += comp_len
    if encoding == 1:
        raw = zlib.decompress(raw)
    return list(struct.unpack(f"<{length}{fmt}", raw)), pos


def _read_prop(data, pos):
    kind = chr(data[pos])
    pos += 1
    scalars = {"Y": "h", "C": "?", "I": "i", "F": "f", "D": "d", "L": "q"}
    if kind in scalars:
        fmt = "<" + scalars[kind]
        return struct.unpack_from(fmt, data, pos)[0], pos + struct.calcsize(fmt)
    arrays = {"f": "f", "d": "d", "l": "q", "i": "i", "b": "?"}
    if kind in arrays:
        return _read_array(data, pos, arrays[kind])
    if kind in "SR":
        (length,) = struct.unpack_from("<I", data, pos)
        pos += 4
        raw = data[pos:pos + length]
        return (raw.decode("utf-8", "replace") if kind == "S" else raw), pos + length
    raise ValueError(f"unknown FBX property type {kind!r} at byte {pos - 1}")


def _read_node(data, pos, wide):
    head = "<QQQB" if wide else "<IIIB"
    end, num_props, _, name_len = struct.unpack_from(head, data, pos)
    pos += struct.calcsize(head)
    if end == 0:
        return None, pos
    name = data[pos:pos + name_len].decode("ascii", "replace")
    pos += name_len
    props = []
    for _ in range(num_props):
        value, pos = _read_prop(data, pos)
        props.append(value)
    children = []
    while pos < end:
        child, pos = _read_node(data, pos, wide)
        if child is None:
            break
        children.append(child)
    return Node(name, props, children), end


def read_fbx(path):
    data = Path(path).read_bytes()
    if not data.startswith(MAGIC):
        raise ValueError("not a binary FBX (ASCII FBX isn't supported)")
    (version,) = struct.unpack_from("<I", data, 23)
    wide = version >= 7500
    pos, top = 27, []
    while pos < len(data):
        node, pos = _read_node(data, pos, wide)
        if node is None:
            break
        top.append(node)
    return Node("", [], top)


def properties70(node):
    """Name -> list of values from a node's Properties70 block."""
    out = {}
    block = node.child("Properties70") if node else None
    for p in block.all("P") if block else []:
        out[p.props[0]] = p.props[4:]
    return out


# ---------------------------------------------------------------------------
# Small 4x4 matrix helpers (column vectors, row-major lists)
# ---------------------------------------------------------------------------

def mat_mul(a, b):
    return [[sum(a[i][k] * b[k][j] for k in range(4)) for j in range(4)] for i in range(4)]


def translation(x, y, z):
    return [[1, 0, 0, x], [0, 1, 0, y], [0, 0, 1, z], [0, 0, 0, 1]]


def scaling(x, y, z):
    return [[x, 0, 0, 0], [0, y, 0, 0], [0, 0, z, 0], [0, 0, 0, 1]]


def euler_xyz(degrees):
    """FBX's default rotation order: X first, then Y, then Z."""
    rx, ry, rz = (math.radians(d) for d in degrees)
    cx, sx, cy, sy, cz, sz = math.cos(rx), math.sin(rx), math.cos(ry), math.sin(ry), math.cos(rz), math.sin(rz)
    x = [[1, 0, 0, 0], [0, cx, -sx, 0], [0, sx, cx, 0], [0, 0, 0, 1]]
    y = [[cy, 0, sy, 0], [0, 1, 0, 0], [-sy, 0, cy, 0], [0, 0, 0, 1]]
    z = [[cz, -sz, 0, 0], [sz, cz, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]
    return mat_mul(z, mat_mul(y, x))


def transform_point(m, p):
    return [sum(m[i][k] * v for k, v in enumerate((*p, 1.0))) for i in range(3)]


IDENTITY = translation(0, 0, 0)


def local_matrix(model):
    props = properties70(model)
    t = props.get("Lcl Translation", [0, 0, 0])
    pre = props.get("PreRotation", [0, 0, 0])
    r = props.get("Lcl Rotation", [0, 0, 0])
    s = props.get("Lcl Scaling", [1, 1, 1])
    return mat_mul(translation(*t), mat_mul(euler_xyz(pre), mat_mul(euler_xyz(r), scaling(*s))))


# ---------------------------------------------------------------------------
# FBX -> triangles
# ---------------------------------------------------------------------------

def geometry_triangles(geom):
    """(positions, uvs) per triangle corner, in the geometry's own space."""
    verts = geom.child("Vertices").props[0]
    poly = geom.child("PolygonVertexIndex").props[0]

    uv_values = uv_index = None
    uv_by_vertex = False
    layer = geom.child("LayerElementUV")
    if layer:
        uv_values = layer.child("UV").props[0]
        mapping = layer.child("MappingInformationType").props[0]
        reference = layer.child("ReferenceInformationType").props[0]
        uv_by_vertex = mapping in ("ByVertice", "ByVertex", "ByControlPoint")
        if reference == "IndexToDirect" and layer.child("UVIndex"):
            uv_index = layer.child("UVIndex").props[0]

    def uv_at(corner, vertex):
        if uv_values is None:
            return (0.0, 0.0)
        i = vertex if uv_by_vertex else corner
        if uv_index is not None:
            i = uv_index[i]
        if i < 0 or 2 * i + 1 >= len(uv_values):
            return (0.0, 0.0)
        u, v = uv_values[2 * i], uv_values[2 * i + 1]
        return (u, 1.0 - v)  # FBX's V points up, glTF's down

    positions, uvs = [], []
    polygon = []  # (corner, vertex) of the polygon being read
    for corner, raw in enumerate(poly):
        last = raw < 0
        vertex = ~raw if last else raw
        polygon.append((corner, vertex))
        if last:
            for i in range(1, len(polygon) - 1):
                for c, v in (polygon[0], polygon[i], polygon[i + 1]):
                    positions.append(tuple(verts[3 * v:3 * v + 3]))
                    uvs.append(uv_at(c, v))
            polygon = []
    return positions, uvs


def convert(path):
    """All triangles in the file, in meters, Y up: (positions, uvs)."""
    root = read_fbx(path)
    objects = root.child("Objects")
    connections = root.child("Connections")
    if objects is None:
        raise ValueError("file has no objects")

    models = {m.props[0]: m for m in objects.all("Model")}
    geometries = {g.props[0]: g for g in objects.all("Geometry") if g.props[2] == "Mesh"}
    parent = {}
    for c in connections.all("C") if connections else []:
        if c.props[0] == "OO":
            parent.setdefault(c.props[1], c.props[2])

    def world(model_id, depth=0):
        if model_id not in models or depth > 64:
            return IDENTITY
        return mat_mul(world(parent.get(model_id), depth + 1), local_matrix(models[model_id]))

    settings = properties70(root.child("GlobalSettings"))
    # FBX units are centimeters scaled by UnitScaleFactor; the game uses meters
    unit = settings.get("UnitScaleFactor", [1.0])[0] / 100.0
    to_meters = scaling(unit, unit, unit)

    positions, uvs = [], []
    for geom_id, geom in geometries.items():
        m = mat_mul(to_meters, world(parent.get(geom_id)))
        p, uv = geometry_triangles(geom)
        positions += [transform_point(m, v) for v in p]
        uvs += uv
    if not positions:
        raise ValueError("no meshes")
    return positions, uvs


# ---------------------------------------------------------------------------
# .glb writing
# ---------------------------------------------------------------------------

def write_glb(out, positions, uvs, texture_png=None):
    pos_bytes = b"".join(struct.pack("<3f", *p) for p in positions)
    uv_bytes = b"".join(struct.pack("<2f", *uv) for uv in uvs)
    buffer = bytearray(pos_bytes + uv_bytes)
    views = [
        {"buffer": 0, "byteOffset": 0, "byteLength": len(pos_bytes), "target": 34962},
        {"buffer": 0, "byteOffset": len(pos_bytes), "byteLength": len(uv_bytes), "target": 34962},
    ]
    lo = [min(p[i] for p in positions) for i in range(3)]
    hi = [max(p[i] for p in positions) for i in range(3)]
    gltf = {
        "asset": {"version": "2.0", "generator": "rpg_game tools/fbx2glb.py"},
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": [{"mesh": 0}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "TEXCOORD_0": 1}, "material": 0}]}],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": len(positions), "type": "VEC3", "min": lo, "max": hi},
            {"bufferView": 1, "componentType": 5126, "count": len(uvs), "type": "VEC2"},
        ],
        "bufferViews": views,
        "materials": [{"pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1], "metallicFactor": 0}}],
    }
    if texture_png:
        while len(buffer) % 4:
            buffer.append(0)
        views.append({"buffer": 0, "byteOffset": len(buffer), "byteLength": len(texture_png)})
        buffer += texture_png
        gltf["images"] = [{"bufferView": 2, "mimeType": "image/png"}]
        gltf["samplers"] = [{"magFilter": 9728, "minFilter": 9728}]
        gltf["textures"] = [{"source": 0, "sampler": 0}]
        gltf["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = {"index": 0}
    gltf["buffers"] = [{"byteLength": len(buffer)}]

    json_bytes = json.dumps(gltf, separators=(",", ":")).encode()
    json_bytes += b" " * (-len(json_bytes) % 4)
    buffer += b"\x00" * (-len(buffer) % 4)
    total = 12 + 8 + len(json_bytes) + 8 + len(buffer)
    with open(out, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, total))
        f.write(struct.pack("<I4s", len(json_bytes), b"JSON") + json_bytes)
        f.write(struct.pack("<I4s", len(buffer), b"BIN\x00") + bytes(buffer))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("paths", nargs="+", help=".fbx files or folders to search")
    parser.add_argument("--texture", help="PNG to embed as every model's texture")
    args = parser.parse_args()

    texture = Path(args.texture).read_bytes() if args.texture else None
    files = []
    for p in map(Path, args.paths):
        files += sorted(p.rglob("*.fbx")) if p.is_dir() else [p]

    failed = 0
    for fbx in files:
        try:
            positions, uvs = convert(fbx)
            write_glb(fbx.with_suffix(".glb"), positions, uvs, texture)
            lo = [min(p[i] for p in positions) for i in range(3)]
            hi = [max(p[i] for p in positions) for i in range(3)]
            size = " x ".join(f"{h - l:.2f}" for l, h in zip(lo, hi))
            print(f"ok    {fbx}  {len(positions) // 3} tris, {size} m")
        except Exception as e:  # keep going; report every bad file
            failed += 1
            print(f"FAIL  {fbx}: {e}", file=sys.stderr)
    print(f"{len(files) - failed} converted, {failed} failed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
