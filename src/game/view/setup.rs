// Startup: runs once, before the first frame (and before `effects::setup`,
// which hangs the weapons on the 3D camera made here). It makes the 3D
// camera that renders offscreen, carrying the player's lantern; the 2D camera
// the window shows; the font sheet and the text filter quad; and the HUD
// image. `Display` keeps their handles so frame.rs can redraw and resize
// them every frame.
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorOptions, PrimaryWindow};

use crate::engine::font::{GLYPH_H, GLYPH_W};
use crate::engine::hud::Cell;
use crate::engine::scene::{FAR_PLANE, NEAR_PLANE};
use crate::engine::text_filter::TextFilter;
use crate::game::ui::screen::Screen;
use crate::game::Game;
use super::input::set_cursor_locked;
use super::scene::Scene3d;
use super::WorldCamera;

/// Light everywhere, so unlit corners are dim rather than black.
pub(super) const AMBIENT: f32 = 50.0;

/// The lantern the player carries: brightness in lumens and reach in meters.
const LANTERN_LUMENS: f32 = 250_000.0;
const LANTERN_RANGE: f32 = 16.0;

/// The HUD texture and its sprite, and the 3D scene image and the text filter
/// quad that shows it.
#[derive(Resource)]
pub(super) struct Display {
	pub(super) image: Handle<Image>,
	pub(super) sprite: Entity,
	pub(super) scene: Handle<Image>,
	pub(super) filter: Handle<TextFilter>,
	pub(super) filter_quad: Entity,
	/// The HUD cells last drawn into `image`
	pub(super) hud: Vec<Cell>,
}

/// Makes the cameras, the text filter, the light and the HUD image, and
/// frees or locks the cursor to suit the opening screen.
pub(super) fn setup(
	mut commands: Commands,
	mut images: ResMut<Assets<Image>>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	game: Res<Game>,
) {
	// The 3D view renders offscreen; it's resized to the cell grid each frame
	let scene = images.add(scene_target(1, 1));
	spawn_cameras(&mut commands, &scene);
	let (filter, filter_quad) = spawn_filter_quad(&mut commands, &mut images, &mut meshes, &mut filters, &game, &scene);

	// Underground there's no sun: the lantern and torches light the way, over
	// a dim ambient fill so unlit corners aren't pitch black
	commands.insert_resource(GlobalAmbientLight { brightness: AMBIENT, ..default() });
	commands.init_resource::<Scene3d>();

	// The game opens on the main menu, so the cursor starts free
	set_cursor_locked(&mut cursors, game.screen == Screen::Playing);

	let (image, sprite) = spawn_hud(&mut commands, &mut images);
	commands.insert_resource(Display { image, sprite, scene, filter, filter_quad, hud: Vec::new() });
}

/// An image the 3D camera can render into, one pixel per font pixel.
pub(super) fn scene_target(width: u32, height: u32) -> Image {
	let mut image = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
	image.sampler = ImageSampler::nearest();
	image
}

/// The 3D camera, rendering into `scene` with the lantern on it, and the 2D
/// camera that draws the window.
fn spawn_cameras(commands: &mut Commands, scene: &Handle<Image>) {
	commands.spawn((
		Camera3d::default(),
		// Black beyond the walls, like the pit under the stairs down
		Camera { order: 0, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
		RenderTarget::Image(scene.clone().into()),
		// No tonemapping, so the colors stay as the models have them
		bevy::core_pipeline::tonemapping::Tonemapping::None,
		bevy::core_pipeline::tonemapping::DebandDither::Disabled,
		Projection::Perspective(PerspectiveProjection {
			fov: 60f32.to_radians(),
			near: NEAR_PLANE,
			far: FAR_PLANE,
			..default()
		}),
		WorldCamera,
		// The player's lantern goes wherever the camera does
		children![(
			PointLight {
				color: Color::srgb(1.0, 0.85, 0.65),
				intensity: LANTERN_LUMENS,
				range: LANTERN_RANGE,
				..default()
			},
			Transform::from_xyz(0.3, -0.3, 0.0),
		)],
	));
	commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }));
}

/// The text filter, which turns the `scene` image into characters from the
/// font sheet, and the quad it's drawn on. Returns both.
fn spawn_filter_quad(
	commands: &mut Commands,
	images: &mut Assets<Image>,
	meshes: &mut Assets<Mesh>,
	filters: &mut Assets<TextFilter>,
	game: &Game,
	scene: &Handle<Image>,
) -> (Handle<TextFilter>, Entity) {
	// The font sheet for the filter: crisp, no blending between glyphs
	let mut font = Image::new(
		Extent3d { width: 16 * GLYPH_W as u32, height: 16 * GLYPH_H as u32, depth_or_array_layers: 1 },
		TextureDimension::D2,
		game.glyphs.atlas_rgba(),
		TextureFormat::Rgba8Unorm,
		RenderAssetUsages::RENDER_WORLD,
	);
	font.sampler = ImageSampler::nearest();
	let font = images.add(font);
	let filter = filters.add(TextFilter { grid: Vec4::ONE, scene: scene.clone(), font });
	let filter_quad = commands
		.spawn((Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial2d(filter.clone()), Transform::default()))
		.id();
	(filter, filter_quad)
}

/// The HUD image (resized and redrawn each frame) and the sprite that shows
/// it above the filter quad. Returns both.
fn spawn_hud(commands: &mut Commands, images: &mut Assets<Image>) -> (Handle<Image>, Entity) {
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
	// Above the filter quad
	let sprite = commands.spawn((Sprite::from_image(image.clone()), Transform::from_xyz(0.0, 0.0, 1.0))).id();
	(image, sprite)
}
