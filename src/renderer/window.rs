// Shows the renderer in a Bevy window: the 3D view is drawn into a character
// grid, the grid is drawn into a texture with the bitmap font, and the texture
// fills the window as one sprite.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;

use super::font::{GLYPH_H, GLYPH_W};
use super::{Action, Renderer};

/// Screen pixels per font pixel. 1 is the smallest the font can be drawn;
/// raise it for a chunkier look.
const PIXEL_SCALE: u32 = 1;

/// Runs the game in the primary window. Insert a `Renderer` resource
/// (from `Renderer::new`) before running.
pub struct RendererPlugin;

impl Plugin for RendererPlugin {
	fn build(&self, app: &mut App) {
		app.add_systems(Startup, setup)
			.add_systems(Update, (handle_input, draw_frame).chain());
	}
}

/// The texture the character grid is drawn into, and the sprite showing it.
#[derive(Resource)]
struct Screen {
	image: Handle<Image>,
	sprite: Entity,
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
	commands.spawn(Camera2d);

	let mut image = Image::new_fill(
		Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
		TextureDimension::D2,
		&[0, 0, 0, 255],
		TextureFormat::Rgba8UnormSrgb,
		// Keep the pixels on the CPU side too, so they can be redrawn every frame
		RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
	);
	// Crisp pixels when scaled, no blur
	image.sampler = ImageSampler::nearest();
	let image = images.add(image);
	let sprite = commands.spawn(Sprite::from_image(image.clone())).id();
	commands.insert_resource(Screen { image, sprite });
}

/// One key press, one turn. Holding a key does nothing more; press again.
fn handle_input(
	keys: Res<ButtonInput<KeyCode>>,
	mut renderer: ResMut<Renderer>,
	mut exit: MessageWriter<AppExit>,
) {
	if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyQ]) {
		exit.write(AppExit::Success);
		return;
	}
	let bindings = [
		([KeyCode::KeyW, KeyCode::ArrowUp], Action::Forward),
		([KeyCode::KeyS, KeyCode::ArrowDown], Action::Back),
		([KeyCode::KeyA, KeyCode::ArrowLeft], Action::TurnLeft),
		([KeyCode::KeyD, KeyCode::ArrowRight], Action::TurnRight),
	];
	for (keys_for_action, action) in bindings {
		if keys.any_just_pressed(keys_for_action) {
			if let Err(e) = renderer.act(action) {
				eprintln!("error: {e}");
				exit.write(AppExit::error());
				return;
			}
		}
	}
}

fn draw_frame(
	windows: Query<&Window, With<PrimaryWindow>>,
	screen: Res<Screen>,
	mut renderer: ResMut<Renderer>,
	mut images: ResMut<Assets<Image>>,
	mut sprites: Query<&mut Sprite>,
) {
	let Ok(window) = windows.single() else { return };

	// As many character cells as fit the window at the font's real pixel size
	let cell_w = GLYPH_W as u32 * PIXEL_SCALE;
	let cell_h = GLYPH_H as u32 * PIXEL_SCALE;
	let cols = (window.physical_width() / cell_w) as usize;
	let rows = (window.physical_height() / cell_h) as usize;
	if cols == 0 || rows == 0 {
		return;
	}
	renderer.render(cols, rows);

	let Some(mut image) = images.get_mut(&screen.image) else { return };
	let size = Extent3d {
		width: (cols * GLYPH_W) as u32,
		height: (rows * GLYPH_H) as u32,
		depth_or_array_layers: 1,
	};
	if image.texture_descriptor.size != size {
		image.resize(size);
	}
	let Some(pixels) = image.data.as_mut() else { return };
	renderer.glyphs.draw(&renderer.framebuffer, pixels);

	// Show the texture at PIXEL_SCALE screen pixels per font pixel. Sizes are in
	// logical pixels, so divide out the display's scale factor.
	if let Ok(mut sprite) = sprites.get_mut(screen.sprite) {
		let physical = Vec2::new((cols as u32 * cell_w) as f32, (rows as u32 * cell_h) as f32);
		sprite.custom_size = Some(physical / window.scale_factor());
	}
}
