// The 3D scene: models placed in the world, and turning our meshes and
// textures into Bevy's so Bevy can draw them.
use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::mesh::Texture;

/// One model placed in the world.
pub(crate) struct SceneObject {
	/// Index into the level's list of meshes
	pub(crate) mesh: usize,
	/// Model space to world space
	pub(crate) transform: glam::Mat4,
}

/// The camera's near and far clipping distances, in meters.
/// The near one is short so a wall right in front of the player isn't cut away.
pub(crate) const NEAR_PLANE: f32 = 0.05;
pub(crate) const FAR_PLANE: f32 = 300.0;

/// What one of our meshes becomes in Bevy: a mesh and material per texture it
/// uses. Spawn every part with the same transform to show the model.
pub(crate) type ModelParts = Vec<(Handle<Mesh>, Handle<StandardMaterial>)>;

/// Uploads one of our textures as a Bevy image: pixelated and repeating.
pub(crate) fn to_bevy_image(texture: &Texture) -> Image {
	let mut image = Image::new(
		Extent3d { width: texture.width, height: texture.height, depth_or_array_layers: 1 },
		TextureDimension::D2,
		texture.rgba.clone(),
		TextureFormat::Rgba8UnormSrgb,
		RenderAssetUsages::RENDER_WORLD,
	);
	image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
		address_mode_u: ImageAddressMode::Repeat,
		address_mode_v: ImageAddressMode::Repeat,
		..ImageSamplerDescriptor::nearest()
	});
	image
}

/// Uploads one of our meshes and its textures to Bevy, one part per texture.
pub(crate) fn model_parts(
	mesh: &crate::engine::mesh::Mesh,
	meshes: &mut Assets<Mesh>,
	materials: &mut Assets<StandardMaterial>,
	images: &mut Assets<Image>,
) -> ModelParts {
	let textures: Vec<_> = mesh.textures.iter().map(|t| images.add(to_bevy_image(t))).collect();
	mesh.split_by_texture()
		.into_iter()
		.map(|(tex, tris)| {
			// Vertex colors carry the model colors and tint the texture; the
			// material just lets light shade them
			let material = materials.add(StandardMaterial {
				base_color: Color::WHITE,
				base_color_texture: textures.get(tex as usize).cloned(),
				perceptual_roughness: 1.0,
				cull_mode: None,
				double_sided: true,
				..default()
			});
			(meshes.add(to_bevy_mesh(mesh, &tris)), material)
		})
		.collect()
}

/// Turns one of our meshes into a Bevy mesh with vertex colors. Every triangle
/// gets its own three vertices so its normal is flat, matching the text shading.
fn to_bevy_mesh(mesh: &crate::engine::mesh::Mesh, triangles: &[[usize; 3]]) -> Mesh {
	use bevy::mesh::{Indices, PrimitiveTopology};
	let mut positions = Vec::with_capacity(triangles.len() * 3);
	let mut normals = Vec::with_capacity(positions.capacity());
	let mut colors = Vec::with_capacity(positions.capacity());
	let mut uvs = Vec::with_capacity(positions.capacity());
	for tri in triangles {
		let [a, b, c] = tri.map(|i| mesh.vertices[i]);
		let n = (b.pos - a.pos).cross(c.pos - a.pos).normalize_or_zero();
		for v in [a, b, c] {
			positions.push(v.pos.to_array());
			normals.push(n.to_array());
			colors.push([v.color.x, v.color.y, v.color.z, 1.0]);
			uvs.push(v.uv.to_array());
		}
	}
	let count = positions.len() as u32;
	Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
		.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
		.with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
		.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
		.with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
		.with_inserted_indices(Indices::U32((0..count).collect()))
}
