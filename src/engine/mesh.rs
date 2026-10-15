// Triangle meshes: the vertex and texture data every model is turned into,
// and the loaders that read them from bundled .obj and .glb files.
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;

use glam::{Mat4, Vec2, Vec3};

/// One corner of a triangle.
#[derive(Clone, Copy)]
pub(crate) struct Vertex {
	pub(crate) pos: Vec3,
	/// RGB, 0.0 to 1.0. Multiplies the texture when there is one.
	pub(crate) color: Vec3,
	/// Texture coordinates; meaningless without a texture
	pub(crate) uv: Vec2,
	/// Index into the mesh's `textures`, or `NO_TEXTURE`
	pub(crate) tex: u32,
}

/// A vertex's `tex` when it isn't textured.
pub(crate) const NO_TEXTURE: u32 = u32::MAX;

/// An RGBA8 image a mesh is painted with.
pub(crate) struct Texture {
	pub(crate) width: u32,
	pub(crate) height: u32,
	pub(crate) rgba: Vec<u8>,
}

/// A model: triangles over shared vertices, and the textures they use.
pub(crate) struct Mesh {
	pub(crate) vertices: Vec<Vertex>,
	pub(crate) indices: Vec<[usize; 3]>,
	pub(crate) textures: Vec<Texture>,
}

/// Used when a model gives no color at all.
const DEFAULT_COLOR: Vec3 = Vec3::ONE;

impl Mesh {
	/// Creates an empty mesh container.
	pub(crate) fn new() -> Self {
		Mesh {
			vertices: Vec::new(),
			indices: Vec::new(),
			textures: Vec::new(),
		}
	}

	/// Appends an untextured vertex and returns its assigned index.
	pub(crate) fn add_vertex(&mut self, pos: Vec3, color: Vec3) -> usize {
		self.add_textured_vertex(pos, color, Vec2::ZERO, NO_TEXTURE)
	}

	/// Appends a vertex painted with `textures[tex]` at `uv`, tinted by `color`.
	pub(crate) fn add_textured_vertex(&mut self, pos: Vec3, color: Vec3, uv: Vec2, tex: u32) -> usize {
		self.vertices.push(Vertex { pos, color, uv, tex });
		self.vertices.len() - 1
	}

	/// Adds a single triangular face using vertex indices.
	pub(crate) fn add_triangle(&mut self, i0: usize, i1: usize, i2: usize) {
		self.indices.push([i0, i1, i2]);
	}

	/// Merges another mesh into this mesh, offsetting indices accordingly.
	pub(crate) fn combine(&mut self, other: &Mesh) {
		let base_index = self.vertices.len();
		let base_tex = self.textures.len() as u32;
		self.textures.extend(
			other.textures.iter().map(|t| Texture { width: t.width, height: t.height, rgba: t.rgba.clone() }),
		);
		self.vertices.extend(other.vertices.iter().map(|&v| Vertex {
			tex: if v.tex == NO_TEXTURE { NO_TEXTURE } else { v.tex + base_tex },
			..v
		}));

		for tri in &other.indices {
			self.indices.push([
				tri[0] + base_index,
				tri[1] + base_index,
				tri[2] + base_index,
			]);
		}
	}

	/// Centers the mesh on the origin and scales it so its farthest point sits
	/// at `radius`. Models come in every size; this makes them all fit the camera.
	pub(crate) fn fit_to_radius(&mut self, radius: f32) {
		if self.vertices.is_empty() {
			return;
		}
		let (min, max) = self.vertices.iter().fold(
			(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
			|(min, max), v| (min.min(v.pos), max.max(v.pos)),
		);
		let center = (min + max) * 0.5;
		let extent = self.vertices.iter().map(|v| (v.pos - center).length()).fold(0.0, f32::max);
		let scale = if extent > 0.0 { radius / extent } else { 1.0 };
		for v in &mut self.vertices {
			v.pos = (v.pos - center) * scale;
		}
	}

	/// Loads a bundled model by its path relative to models/ (e.g. "cube.obj").
	/// The file extension picks the loader.
	pub(crate) fn load(name: &str) -> Result<Self, String> {
		let bytes = super::models::get(name).ok_or_else(|| format!("no bundled model named {name:?}"))?;
		let ext = Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
		let mesh = match ext.as_str() {
			"obj" => Self::from_obj(bytes, name),
			"glb" | "gltf" => Self::from_gltf(bytes),
			_ => Err(format!("unsupported model format {ext:?}")),
		};
		mesh.map_err(|e| format!("{name}: {e}"))
	}

	/// Parses Wavefront OBJ with `tobj`. Colors come from vertex colors
	/// (`v x y z r g b`) when present, otherwise from the material's diffuse
	/// color (`Kd`) in a bundled `.mtl` file, otherwise white.
	/// `name` is the OBJ's own path, so `mtllib` can be found beside it.
	pub(crate) fn from_obj(bytes: &[u8], name: &str) -> Result<Self, String> {
		let options = tobj::LoadOptions {
			triangulate: true,
			single_index: true,
			..Default::default()
		};
		let dir = Path::new(name).parent().unwrap_or(Path::new(""));

		let (models, materials) = tobj::load_obj_buf(&mut Cursor::new(bytes), &options, |mtl_path| {
			// mtllib paths are relative to the OBJ; look them up among the bundled files
			let full = dir.join(mtl_path).to_string_lossy().replace('\\', "/");
			let data = super::models::get(&full).ok_or(tobj::LoadError::OpenFileFailed)?;
			tobj::load_mtl_buf(&mut Cursor::new(data))
		})
		.map_err(|e| e.to_string())?;
		// A missing or broken .mtl is not fatal: the model just falls back to white
		let materials = materials.unwrap_or_default();

		let mut mesh = Self::new();
		for model in &models {
			let m = &model.mesh;
			let material_color = m
				.material_id
				.and_then(|id| materials.get(id))
				.and_then(|mat| mat.diffuse)
				.map(Vec3::from_array)
				.unwrap_or(DEFAULT_COLOR);

			let base = mesh.vertices.len();
			for (i, p) in m.positions.chunks_exact(3).enumerate() {
				let color = m
					.vertex_color
					.get(i * 3..i * 3 + 3)
					.map(Vec3::from_slice)
					.unwrap_or(material_color);
				mesh.add_vertex(Vec3::from_slice(p), color);
			}
			for tri in m.indices.chunks_exact(3) {
				mesh.add_triangle(base + tri[0] as usize, base + tri[1] as usize, base + tri[2] as usize);
			}
		}
		Ok(mesh)
	}

	/// Parses binary glTF (.glb) with the `gltf` crate. Every triangle mesh in
	/// the default scene is placed with its node transform and merged into one.
	/// Colors are the vertex colors (COLOR_0) times the material's base color,
	/// times its texture when it has one embedded in the file.
	/// Animation and skinning are ignored.
	pub(crate) fn from_gltf(bytes: &[u8]) -> Result<Self, String> {
		let gltf = gltf::Gltf::from_slice(bytes).map_err(|e| e.to_string())?;
		let blob = gltf.blob.as_deref();

		let mut mesh = Self::new();
		// glTF image index -> index into mesh.textures, so shared images load once
		let mut texture_slots = HashMap::new();
		let scene = gltf.default_scene().or_else(|| gltf.scenes().next()).ok_or("file has no scene")?;
		let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();

		while let Some((node, parent)) = stack.pop() {
			let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
			stack.extend(node.children().map(|child| (child, transform)));

			let Some(node_mesh) = node.mesh() else { continue };
			for primitive in node_mesh.primitives() {
				mesh.add_gltf_primitive(&gltf, blob, &primitive, transform, &mut texture_slots)?;
			}
		}
		Ok(mesh)
	}

	/// The triangles grouped by the texture they're painted with (by their first
	/// corner), for renderers that draw one texture at a time.
	pub(crate) fn split_by_texture(&self) -> Vec<(u32, Vec<[usize; 3]>)> {
		let mut groups: Vec<(u32, Vec<[usize; 3]>)> = Vec::new();
		for &tri in &self.indices {
			let tex = self.vertices[tri[0]].tex;
			match groups.iter_mut().find(|(t, _)| *t == tex) {
				Some((_, tris)) => tris.push(tri),
				None => groups.push((tex, vec![tri])),
			}
		}
		groups
	}

	/// Generates a UV Sphere with configurable rings and segments.
	pub(crate) fn uv_sphere(radius: f32, rings: usize, segments: usize) -> Self {
		let mut mesh = Self::new();

		for i in 0..=rings {
			let v = i as f32 / rings as f32;
			let phi = v * std::f32::consts::PI;

			for j in 0..=segments {
				let u = j as f32 / segments as f32;
				let theta = u * std::f32::consts::TAU;

				let x = radius * phi.sin() * theta.cos();
				let y = radius * phi.cos();
				let z = radius * phi.sin() * theta.sin();

				mesh.add_vertex(Vec3::new(x, y, z), DEFAULT_COLOR);
			}
		}

		let stride = segments + 1;
		for i in 0..rings {
			for j in 0..segments {
				let next_ring = i + 1;
				let next_segment = j + 1;

				let v0 = i * stride + j;
				let v1 = next_ring * stride + j;
				let v2 = next_ring * stride + next_segment;
				let v3 = i * stride + next_segment;

				if i != 0 {
					mesh.add_triangle(v0, v1, v2);
				}
				if i != rings - 1 {
					mesh.add_triangle(v0, v2, v3);
				}
			}
		}

		mesh
	}

	// ---------------------------------------------------------------
	// glTF helpers, used by `from_gltf`
	// ---------------------------------------------------------------

	/// Adds one glTF primitive, placed by `transform`. Anything that isn't a
	/// triangle list, or has no positions, is skipped.
	fn add_gltf_primitive(
		&mut self,
		gltf: &gltf::Gltf,
		blob: Option<&[u8]>,
		primitive: &gltf::Primitive,
		transform: Mat4,
		texture_slots: &mut HashMap<usize, u32>,
	) -> Result<(), String> {
		if primitive.mode() != gltf::mesh::Mode::Triangles {
			return Ok(());
		}
		let reader = primitive.reader(|buffer| gltf_buffer_data(buffer, blob));
		let Some(positions) = reader.read_positions() else { return Ok(()) };

		let material = primitive.material();
		let (texture, tint) = primitive_texture(&material);
		let tex = match &texture {
			Some(info) => self.gltf_texture_slot(gltf, blob, texture_slots, info.texture().source().index()),
			None => NO_TEXTURE,
		};
		let mut uvs = texture.as_ref().and_then(|t| reader.read_tex_coords(t.tex_coord())).map(|c| c.into_f32());
		let mut colors = reader.read_colors(0).map(|c| c.into_rgb_f32());

		let base = self.vertices.len();
		for p in positions {
			let vertex_color =
				colors.as_mut().and_then(|c| c.next()).map(Vec3::from_array).unwrap_or(Vec3::ONE);
			let uv = uvs.as_mut().and_then(|c| c.next()).map(Vec2::from_array);
			let pos = transform.transform_point3(Vec3::from_array(p));
			match uv {
				Some(uv) => self.add_textured_vertex(pos, vertex_color * tint, uv, tex),
				None => self.add_vertex(pos, vertex_color * tint),
			};
		}
		let count = self.vertices.len() - base;

		// Primitives without indices draw their vertices in order
		let indices: Vec<usize> = match reader.read_indices() {
			Some(idx) => idx.into_u32().map(|i| i as usize).collect(),
			None => (0..count).collect(),
		};
		self.add_primitive_triangles(base, count, &indices)
	}

	/// Where glTF image `image` sits in `textures`, loading it the first time
	/// it's used. `NO_TEXTURE` if it can't be loaded.
	fn gltf_texture_slot(
		&mut self,
		gltf: &gltf::Gltf,
		blob: Option<&[u8]>,
		texture_slots: &mut HashMap<usize, u32>,
		image: usize,
	) -> u32 {
		match texture_slots.get(&image) {
			Some(&slot) => slot,
			None => {
				// A texture we can't read (e.g. a separate file) leaves the model flat-colored
				let slot = match load_texture(gltf, image, blob) {
					Ok(t) => {
						self.textures.push(t);
						self.textures.len() as u32 - 1
					}
					Err(_) => NO_TEXTURE,
				};
				texture_slots.insert(image, slot);
				slot
			}
		}
	}

	/// Adds a primitive's triangles. `indices` count from `base`, the primitive's
	/// first vertex, and must stay below `count`, its number of vertices.
	fn add_primitive_triangles(&mut self, base: usize, count: usize, indices: &[usize]) -> Result<(), String> {
		for tri in indices.chunks_exact(3) {
			if tri.iter().any(|&i| i >= count) {
				return Err("triangle index out of range".into());
			}
			self.add_triangle(base + tri[0], base + tri[1], base + tri[2]);
		}
		Ok(())
	}
}

/// The bytes of a glTF buffer. Only the .glb's own binary chunk is available.
fn gltf_buffer_data<'s>(buffer: gltf::Buffer, blob: Option<&'s [u8]>) -> Option<&'s [u8]> {
	match buffer.source() {
		gltf::buffer::Source::Bin => blob,
		// External .bin files and data URIs aren't supported; .glb keeps everything inside
		gltf::buffer::Source::Uri(_) => None,
	}
}

/// The texture that paints a material, if any, and the color that tints it.
fn primitive_texture<'a>(material: &gltf::Material<'a>) -> (Option<gltf::texture::Info<'a>>, Vec3) {
	// The base color texture paints the model. Some packs leave the base
	// color black and put the art in the emissive texture instead; use
	// that when it's all there is.
	let pbr = material.pbr_metallic_roughness();
	match (pbr.base_color_texture(), material.emissive_texture()) {
		(Some(t), _) => (Some(t), Vec3::from_slice(&pbr.base_color_factor()[..3])),
		(None, Some(t)) => (Some(t), Vec3::from_array(material.emissive_factor())),
		(None, None) => (None, Vec3::from_slice(&pbr.base_color_factor()[..3])),
	}
}

/// Decodes glTF image `index`, which must be embedded in the .glb's binary chunk.
fn load_texture(gltf: &gltf::Gltf, index: usize, blob: Option<&[u8]>) -> Result<Texture, String> {
	let image = gltf.images().nth(index).ok_or("texture image missing")?;
	let bytes = match image.source() {
		gltf::image::Source::View { view, .. } => {
			let blob = blob.ok_or("texture data missing")?;
			blob.get(view.offset()..view.offset() + view.length()).ok_or("texture data out of range")?
		}
		gltf::image::Source::Uri { uri, .. } => {
			return Err(format!("external texture {uri:?} isn't supported"));
		}
	};
	let rgba = image::load_from_memory(bytes).map_err(|e| format!("texture: {e}"))?.to_rgba8();
	Ok(Texture { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

#[cfg(test)]
mod tests;
