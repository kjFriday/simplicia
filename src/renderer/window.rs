// Shows the renderer in a Bevy window. Bevy draws the level's models in 3D
// into an offscreen image; the text filter shader turns that image into
// characters on a quad filling the window; the HUD (the minimap) is drawn as
// text on a sprite above it.
use bevy::asset::{embedded_asset, embedded_path, AssetPath, RenderAssetUsages};
use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, Extent3d, TextureDimension, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{Material2d, Material2dPlugin};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::font::{GLYPH_H, GLYPH_W};
use super::menu::{Choice, Screen};
use super::{Action, Renderer};

/// Screen pixels per font pixel. 1 is the smallest the font can be drawn;
/// raise it for a chunkier look.
const PIXEL_SCALE: u32 = 1;

/// Radians the view turns per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.003;

/// Runs the game in the primary window. Insert a `Renderer` resource
/// (from `Renderer::new`) before running.
pub struct RendererPlugin;

impl Plugin for RendererPlugin {
	fn build(&self, app: &mut App) {
		embedded_asset!(app, "text_filter.wgsl");
		app.add_plugins(Material2dPlugin::<TextFilter>::default())
			.add_systems(Startup, setup)
			.add_systems(Update, (handle_input, mouse_look, sync_scene, sync_camera, draw_frame).chain());
	}
}

/// Draws the 3D scene image as characters; see text_filter.wgsl.
#[derive(Asset, TypePath, AsBindGroup, Clone)]
struct TextFilter {
	/// x, y: cells across and down
	#[uniform(0)]
	grid: Vec4,
	#[texture(1)]
	#[sampler(2)]
	scene: Handle<Image>,
	#[texture(3)]
	#[sampler(4)]
	font: Handle<Image>,
}

impl Material2d for TextFilter {
	fn fragment_shader() -> ShaderRef {
		ShaderRef::Path(AssetPath::from_path_buf(embedded_path!("text_filter.wgsl")).with_source("embedded"))
	}
}

/// The level whose models are spawned as Bevy 3D entities, and those entities.
#[derive(Resource, Default)]
struct Scene3d {
	/// (Renderer::run, depth) of the level shown
	shown: Option<(u32, usize)>,
	entities: Vec<Entity>,
}

/// Marks the camera that draws the 3D models under the text.
#[derive(Component)]
struct WorldCamera;

/// Turns one of our meshes into a Bevy mesh with vertex colors. Every triangle
/// gets its own three vertices so its normal is flat, matching the text shading.
fn to_bevy_mesh(mesh: &super::Mesh, triangles: &[[usize; 3]]) -> Mesh {
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

/// Uploads one of our textures as a Bevy image: pixelated and repeating.
fn to_bevy_image(texture: &super::Texture) -> Image {
	use bevy::image::{ImageAddressMode, ImageSamplerDescriptor};
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

/// Respawns the 3D models whenever the player changes level.
fn sync_scene(
	mut commands: Commands,
	renderer: Res<Renderer>,
	mut scene: ResMut<Scene3d>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut images: ResMut<Assets<Image>>,
) {
	if scene.shown == Some((renderer.run, renderer.depth)) {
		return;
	}
	for entity in scene.entities.drain(..) {
		commands.entity(entity).despawn();
	}
	scene.shown = Some((renderer.run, renderer.depth));

	let world = &renderer.levels[renderer.depth];
	// Vertex colors carry the model colors and tint the texture; the material
	// just lets light shade them
	let mut material_for = |texture: Option<Handle<Image>>| {
		materials.add(StandardMaterial {
			base_color: Color::WHITE,
			base_color_texture: texture,
			perceptual_roughness: 1.0,
			cull_mode: None,
			double_sided: true,
			..default()
		})
	};
	// Each model becomes one Bevy mesh + material per texture it uses
	let parts: Vec<Vec<(Handle<Mesh>, Handle<StandardMaterial>)>> = world
		.meshes
		.iter()
		.map(|m| {
			let textures: Vec<_> = m.textures.iter().map(|t| images.add(to_bevy_image(t))).collect();
			m.split_by_texture()
				.into_iter()
				.map(|(tex, tris)| {
					let texture = textures.get(tex as usize).cloned();
					(meshes.add(to_bevy_mesh(m, &tris)), material_for(texture))
				})
				.collect()
		})
		.collect();
	for object in &world.objects {
		let transform = Transform::from_matrix(Mat4::from_cols_array(&object.transform.to_cols_array()));
		for (mesh, material) in &parts[object.mesh] {
			let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id();
			scene.entities.push(entity);
		}
	}
}

/// Puts the 3D camera where the player's eye is, looking where they look.
fn sync_camera(renderer: Res<Renderer>, mut cameras: Query<&mut Transform, With<WorldCamera>>) {
	let Ok(mut transform) = cameras.single_mut() else { return };
	let view = renderer.levels[renderer.depth].player.view_matrix();
	*transform = Transform::from_matrix(Mat4::from_cols_array(&view.inverse().to_cols_array()));
}

/// The HUD texture and its sprite, and the 3D scene image and the text filter
/// quad that shows it.
#[derive(Resource)]
struct Display {
	image: Handle<Image>,
	sprite: Entity,
	scene: Handle<Image>,
	filter: Handle<TextFilter>,
	filter_quad: Entity,
}

/// An image the 3D camera can render into, one pixel per font pixel.
fn scene_target(width: u32, height: u32) -> Image {
	let mut image = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
	image.sampler = ImageSampler::nearest();
	image
}

fn setup(
	mut commands: Commands,
	mut images: ResMut<Assets<Image>>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	renderer: Res<Renderer>,
) {
	// The 3D view renders offscreen; it's resized to the cell grid each frame
	let scene = images.add(scene_target(1, 1));
	commands.spawn((
		Camera3d::default(),
		Camera { order: 0, ..default() },
		RenderTarget::Image(scene.clone().into()),
		// No tonemapping, so the colors stay as the models have them
		bevy::core_pipeline::tonemapping::Tonemapping::None,
		bevy::core_pipeline::tonemapping::DebandDither::Disabled,
		Projection::Perspective(PerspectiveProjection {
			fov: 60f32.to_radians(),
			near: super::NEAR_PLANE,
			far: super::FAR_PLANE,
			..default()
		}),
		WorldCamera,
	));
	commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() }));

	// The font sheet for the filter: crisp, no blending between glyphs
	let mut font = Image::new(
		Extent3d { width: 16 * GLYPH_W as u32, height: 16 * GLYPH_H as u32, depth_or_array_layers: 1 },
		TextureDimension::D2,
		renderer.glyphs.atlas_rgba(),
		TextureFormat::Rgba8Unorm,
		RenderAssetUsages::RENDER_WORLD,
	);
	font.sampler = ImageSampler::nearest();
	let font = images.add(font);
	let filter = filters.add(TextFilter { grid: Vec4::ONE, scene: scene.clone(), font });
	let filter_quad = commands
		.spawn((Mesh2d(meshes.add(Rectangle::new(1.0, 1.0))), MeshMaterial2d(filter.clone()), Transform::default()))
		.id();

	// The same sun the text shading uses, plus a little ambient light
	let light_dir = Vec3::from_array(renderer.light_dir.to_array());
	commands.spawn((
		DirectionalLight { illuminance: 8_000.0, ..default() },
		Transform::default().looking_to(light_dir, Vec3::Y),
	));
	commands.insert_resource(GlobalAmbientLight { brightness: 300.0, ..default() });
	commands.init_resource::<Scene3d>();

	// The game opens on the main menu, so the cursor starts free
	set_cursor_locked(&mut cursors, renderer.screen == Screen::Playing);

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
	commands.insert_resource(Display { image, sprite, scene, filter, filter_quad });
}

/// In a menu: W/S or the arrows move, Enter picks, Esc backs out of pause.
/// Playing: one key press, one turn (holding a key does nothing more), and
/// Esc pauses. The cursor is only locked while playing.
fn handle_input(
	keys: Res<ButtonInput<KeyCode>>,
	mut renderer: ResMut<Renderer>,
	mut exit: MessageWriter<AppExit>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
	let before = renderer.screen;
	if renderer.screen != Screen::Playing {
		if keys.any_just_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
			renderer.screen.move_selection(-1);
		}
		if keys.any_just_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
			renderer.screen.move_selection(1);
		}
		let pick = if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]) {
			renderer.screen.choice()
		} else if matches!(renderer.screen, Screen::Paused { .. }) && keys.just_pressed(KeyCode::Escape) {
			Some(Choice::Resume)
		} else {
			None
		};
		match pick {
			Some(Choice::NewGame) => {
				if let Err(e) = renderer.new_game() {
					eprintln!("error: {e}");
					exit.write(AppExit::error());
					return;
				}
				renderer.screen = Screen::Playing;
			}
			Some(Choice::Resume) => renderer.screen = Screen::Playing,
			Some(Choice::MainMenu) => renderer.screen = Screen::MainMenu { selected: 0 },
			Some(Choice::Quit) => {
				exit.write(AppExit::Success);
				return;
			}
			None => {}
		}
	} else if keys.just_pressed(KeyCode::Escape) {
		renderer.screen = Screen::Paused { selected: 0 };
	} else {
		play_turn(&keys, &mut renderer, &mut exit);
	}

	// Lock the cursor for mouselook while playing; free it in menus
	if (before == Screen::Playing) != (renderer.screen == Screen::Playing) {
		set_cursor_locked(&mut cursors, renderer.screen == Screen::Playing);
	}
}

fn set_cursor_locked(cursors: &mut Query<&mut CursorOptions, With<PrimaryWindow>>, locked: bool) {
	if let Ok(mut cursor) = cursors.single_mut() {
		cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
		cursor.visible = !locked;
	}
}

/// One key press, one turn.
fn play_turn(keys: &ButtonInput<KeyCode>, renderer: &mut Renderer, exit: &mut MessageWriter<AppExit>) {
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

fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut renderer: ResMut<Renderer>) {
	if motion.delta != Vec2::ZERO && renderer.screen == Screen::Playing {
		// Screen y grows downward, so moving the mouse up looks up
		renderer.look(motion.delta.x * MOUSE_SENSITIVITY, -motion.delta.y * MOUSE_SENSITIVITY);
	}
}

fn draw_frame(
	windows: Query<&Window, With<PrimaryWindow>>,
	screen: Res<Display>,
	mut renderer: ResMut<Renderer>,
	mut images: ResMut<Assets<Image>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut sprites: Query<&mut Sprite>,
	mut transforms: Query<&mut Transform, Without<WorldCamera>>,
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

	let size = Extent3d {
		width: (cols * GLYPH_W) as u32,
		height: (rows * GLYPH_H) as u32,
		depth_or_array_layers: 1,
	};
	{
		let Some(mut image) = images.get_mut(&screen.image) else { return };
		if image.texture_descriptor.size != size {
			image.resize(size);
		}
		let Some(pixels) = image.data.as_mut() else { return };
		renderer.glyphs.draw(&renderer.framebuffer, pixels);
	}

	// The 3D view renders one pixel per font pixel, so each cell covers 8 x 8
	if images.get(&screen.scene).is_some_and(|s| s.texture_descriptor.size != size) {
		images.insert(&screen.scene, scene_target(size.width, size.height)).ok();
		if let Some(mut filter) = filters.get_mut(&screen.filter) {
			filter.grid = Vec4::new(cols as f32, rows as f32, 0.0, 0.0);
		}
	}

	// Show both at PIXEL_SCALE screen pixels per font pixel. Sizes are in
	// logical pixels, so divide out the display's scale factor.
	let logical = Vec2::new((cols as u32 * cell_w) as f32, (rows as u32 * cell_h) as f32) / window.scale_factor();
	if let Ok(mut sprite) = sprites.get_mut(screen.sprite) {
		sprite.custom_size = Some(logical);
	}
	if let Ok(mut transform) = transforms.get_mut(screen.filter_quad) {
		transform.scale = logical.extend(1.0);
	}
}
