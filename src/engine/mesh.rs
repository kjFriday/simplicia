// Triangle meshes: the vertex and texture data every model is turned into,
// and the loaders that read them from bundled .obj and .glb files.
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

/// Decodes glTF image `index`, which must be embedded in the .glb's binary chunk.
fn load_texture(gltf: &gltf::Gltf, index: usize, blob: Option<&[u8]>) -> Result<Texture, String> {
	let image = gltf.images().nth(index).ok_or("texture image missing")?;
	let bytes = match image.source() {
		gltf::image::Source::View { view, .. } => {
			let blob = blob.ok_or("texture data missing")?;
			blob.get(view.offset()..view.offset() + view.length()).ok_or("texture data out of range")?
		}
		gltf::image::Source::Uri { uri, .. } => return Err(format!("external texture {uri:?} isn't supported")),
	};
	let rgba = image::load_from_memory(bytes).map_err(|e| format!("texture: {e}"))?.to_rgba8();
	Ok(Texture { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() })
}

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
		self.textures.extend(other.textures.iter().map(|t| Texture { width: t.width, height: t.height, rgba: t.rgba.clone() }));
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
		let buffer_data = |buffer: gltf::Buffer| match buffer.source() {
			gltf::buffer::Source::Bin => blob,
			// External .bin files and data URIs aren't supported; .glb keeps everything inside
			gltf::buffer::Source::Uri(_) => None,
		};

		let mut mesh = Self::new();
		// glTF image index -> index into mesh.textures, so shared images load once
		let mut texture_slots = std::collections::HashMap::new();
		let scene = gltf.default_scene().or_else(|| gltf.scenes().next()).ok_or("file has no scene")?;
		let mut stack: Vec<(gltf::Node, Mat4)> = scene.nodes().map(|n| (n, Mat4::IDENTITY)).collect();

		while let Some((node, parent)) = stack.pop() {
			let transform = parent * Mat4::from_cols_array_2d(&node.transform().matrix());
			stack.extend(node.children().map(|child| (child, transform)));

			let Some(node_mesh) = node.mesh() else { continue };
			for primitive in node_mesh.primitives() {
				if primitive.mode() != gltf::mesh::Mode::Triangles {
					continue;
				}
				let reader = primitive.reader(buffer_data);
				let Some(positions) = reader.read_positions() else { continue };

				// The base color texture paints the model. Some packs leave the base
				// color black and put the art in the emissive texture instead; use
				// that when it's all there is.
				let material = primitive.material();
				let pbr = material.pbr_metallic_roughness();
				let (texture, tint) = match (pbr.base_color_texture(), material.emissive_texture()) {
					(Some(t), _) => (Some(t), Vec3::from_slice(&pbr.base_color_factor()[..3])),
					(None, Some(t)) => (Some(t), Vec3::from_array(material.emissive_factor())),
					(None, None) => (None, Vec3::from_slice(&pbr.base_color_factor()[..3])),
				};
				let tex = match &texture {
					Some(info) => {
						let image = info.texture().source().index();
						match texture_slots.get(&image) {
							Some(&slot) => slot,
							None => {
								// A texture we can't read (e.g. a separate file) leaves the model flat-colored
								let slot = match load_texture(&gltf, image, blob) {
									Ok(t) => {
										mesh.textures.push(t);
										mesh.textures.len() as u32 - 1
									}
									Err(_) => NO_TEXTURE,
								};
								texture_slots.insert(image, slot);
								slot
							}
						}
					}
					None => NO_TEXTURE,
				};
				let mut uvs = texture.as_ref().and_then(|t| reader.read_tex_coords(t.tex_coord())).map(|c| c.into_f32());
				let mut colors = reader.read_colors(0).map(|c| c.into_rgb_f32());

				let base = mesh.vertices.len();
				for p in positions {
					let vertex_color = colors.as_mut().and_then(|c| c.next()).map(Vec3::from_array).unwrap_or(Vec3::ONE);
					let uv = uvs.as_mut().and_then(|c| c.next()).map(Vec2::from_array);
					let pos = transform.transform_point3(Vec3::from_array(p));
					match uv {
						Some(uv) => mesh.add_textured_vertex(pos, vertex_color * tint, uv, tex),
						None => mesh.add_vertex(pos, vertex_color * tint),
					};
				}
				let count = mesh.vertices.len() - base;

				// Primitives without indices draw their vertices in order
				let indices: Vec<usize> = match reader.read_indices() {
					Some(idx) => idx.into_u32().map(|i| i as usize).collect(),
					None => (0..count).collect(),
				};
				for tri in indices.chunks_exact(3) {
					if tri.iter().any(|&i| i >= count) {
						return Err("triangle index out of range".into());
					}
					mesh.add_triangle(base + tri[0], base + tri[1], base + tri[2]);
				}
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
}

#[cfg(test)]
mod tests {
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
}
