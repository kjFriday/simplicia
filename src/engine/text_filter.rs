// The text filter: a 2D material that draws the rendered 3D scene as
// characters from the bitmap font; see text_filter.wgsl.
use bevy::asset::{embedded_asset, embedded_path, AssetPath};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};

/// Registers the text filter's shader and material.
pub struct TextFilterPlugin;

impl Plugin for TextFilterPlugin {
	fn build(&self, app: &mut App) {
		embedded_asset!(app, "text_filter.wgsl");
		app.add_plugins(Material2dPlugin::<TextFilter>::default());
	}
}

/// Draws the 3D scene image as characters; see text_filter.wgsl.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
pub(crate) struct TextFilter {
	/// x, y: cells across and down
	#[uniform(0)]
	pub(crate) grid: Vec4,
	#[texture(1)]
	#[sampler(2)]
	pub(crate) scene: Handle<Image>,
	#[texture(3)]
	#[sampler(4)]
	pub(crate) font: Handle<Image>,
}

impl Material2d for TextFilter {
	fn fragment_shader() -> ShaderRef {
		ShaderRef::Path(AssetPath::from_path_buf(embedded_path!("text_filter.wgsl")).with_source("embedded"))
	}
}
