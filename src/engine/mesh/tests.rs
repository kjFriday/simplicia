// Tests for the mesh loaders and helpers in mesh.rs.
use super::*;

/// Triangles as corner positions, so tests don't depend on how a loader numbers vertices.
fn triangles(mesh: &Mesh) -> Vec<[Vec3; 3]> {
	mesh.indices.iter().map(|t| t.map(|i| mesh.vertices[i].pos)).collect()
}

#[test]
fn cube_matches_old_hardcoded_data_and_has_vertex_colors() {
	let old = [
		Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, -1.0, -1.0),
		Vec3::new(1.0, 1.0, -1.0), Vec3::new(-1.0, 1.0, -1.0),
		Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, -1.0, 1.0),
		Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, 1.0, 1.0),
	];
	let old_tris: Vec<[Vec3; 3]> = [
		[0, 1, 2], [0, 2, 3], [5, 4, 7], [5, 7, 6], [3, 2, 6], [3, 6, 7],
		[4, 5, 1], [4, 1, 0], [1, 5, 6], [1, 6, 2], [4, 0, 3], [4, 3, 7],
	]
	.iter()
	.map(|t: &[usize; 3]| t.map(|i| old[i]))
	.collect();

	let cube = Mesh::load("cube.obj").unwrap();
	assert_eq!(triangles(&cube), old_tris);
	assert!(cube.vertices.iter().any(|v| v.color != DEFAULT_COLOR));
}

#[test]
fn pyramid_takes_colors_from_its_mtl() {
	let pyramid = Mesh::load("pyramid.obj").unwrap();
	assert_eq!(pyramid.indices.len(), 6);
	assert!(pyramid.vertices.iter().all(|v| v.color != DEFAULT_COLOR));
}

#[test]
fn obj_without_colors_is_white() {
	let mesh = Mesh::from_obj(b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n", "t.obj").unwrap();
	assert!(mesh.vertices.iter().all(|v| v.color == DEFAULT_COLOR));
}

/// Builds a minimal one-triangle .glb in memory: a red material and a node moved +2 on X.
fn tiny_glb() -> Vec<u8> {
	let mut bin = Vec::new();
	for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
		bin.extend_from_slice(&f.to_le_bytes());
	}
	let json = r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
		"nodes":[{"mesh":0,"translation":[2,0,0]}],
		"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
		"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}}],
		"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
		"bufferViews":[{"buffer":0,"byteLength":36}],
		"buffers":[{"byteLength":36}]}"#;
	let mut json = json.as_bytes().to_vec();
	while json.len() % 4 != 0 {
		json.push(b' ');
	}

	let mut glb = Vec::new();
	let total = 12 + 8 + json.len() + 8 + bin.len();
	glb.extend_from_slice(b"glTF");
	glb.extend_from_slice(&2u32.to_le_bytes());
	glb.extend_from_slice(&(total as u32).to_le_bytes());
	glb.extend_from_slice(&(json.len() as u32).to_le_bytes());
	glb.extend_from_slice(b"JSON");
	glb.extend_from_slice(&json);
	glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
	glb.extend_from_slice(b"BIN\0");
	glb.extend_from_slice(&bin);
	glb
}

#[test]
fn glb_loads_geometry_transform_and_material_color() {
	let mesh = Mesh::from_gltf(&tiny_glb()).unwrap();
	assert_eq!(mesh.indices, vec![[0, 1, 2]]);
	assert_eq!(mesh.vertices[1].pos, Vec3::new(3.0, 0.0, 0.0));
	assert!(mesh.vertices.iter().all(|v| v.color == Vec3::new(1.0, 0.0, 0.0)));
}

#[test]
fn fit_to_radius_centers_and_scales() {
	let mut cube = Mesh::load("cube.obj").unwrap();
	cube.fit_to_radius(1.0);
	let far = cube.vertices.iter().map(|v| v.pos.length()).fold(0.0, f32::max);
	assert!((far - 1.0).abs() < 1e-5);
}

#[test]
fn wall_model_gets_its_emissive_texture() {
	let wall = Mesh::load("terrain/canyon/SM_AP_Cube_4m_0.glb").unwrap();
	assert_eq!(wall.textures.len(), 1);
	assert!(wall.vertices.iter().all(|v| v.tex == 0));
	// Not black: the emissive factor tints it, not the black base color
	assert!(wall.vertices.iter().all(|v| v.color == Vec3::ONE));
}

#[test]
fn every_bundled_model_loads() {
	let failures: Vec<String> = super::super::models::MODEL_FILES
		.iter()
		.filter(|(n, _)| n.ends_with(".obj") || n.ends_with(".glb"))
		.filter_map(|(n, _)| Mesh::load(n).err())
		.collect();
	assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn load_errors_are_reported() {
	assert!(Mesh::load("missing.obj").is_err());
	assert!(Mesh::from_obj(b"v 0 0 0\nf 1 2 3\n", "t.obj").is_err());
	assert!(Mesh::from_gltf(b"not a glb").is_err());
}
