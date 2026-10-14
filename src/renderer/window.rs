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
use super::editor::{self, Editor};
use super::menu::{Bind, Request, Screen};
use super::{Action, Renderer};


/// The 3D view renders at 1 / this of the screen's resolution, in each
/// direction. Lower is sharper in textured mode but slower.
const SCENE_DIVISOR: u32 = 4;

/// Torch light: brightness in lumens and reach in meters.
const TORCH_LUMENS: f32 = 60_000.0;
const TORCH_RANGE: f32 = 14.0;
/// Light everywhere, so unlit corners are dim rather than black.
const AMBIENT: f32 = 120.0;

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
			.add_systems(
				Update,
				(
					handle_input,
					editor::update.run_if(resource_exists::<Editor>),
					mouse_look,
					sync_scene,
					sync_camera,
					draw_frame,
				)
					.chain(),
			);
		// The level editor (F1) is only in debug builds
		if cfg!(debug_assertions) {
			app.init_resource::<Editor>();
		}
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

/// What one of our meshes becomes in Bevy: a mesh and material per texture it
/// uses. Spawn every part with the same transform to show the model.
pub(super) type ModelParts = Vec<(Handle<Mesh>, Handle<StandardMaterial>)>;

pub(super) fn model_parts(
	mesh: &super::Mesh,
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
	let parts: Vec<_> =
		world.meshes.iter().map(|m| model_parts(m, &mut meshes, &mut materials, &mut images)).collect();
	// A warm flickerless glow at every torch
	for &at in &world.lights {
		let entity = commands
			.spawn((
				PointLight {
					color: Color::srgb(1.0, 0.65, 0.35),
					intensity: TORCH_LUMENS,
					range: TORCH_RANGE,
					..default()
				},
				Transform::from_translation(Vec3::from_array(at.to_array())),
			))
			.id();
		scene.entities.push(entity);
	}
	for object in &world.objects {
		let transform = Transform::from_matrix(Mat4::from_cols_array(&object.transform.to_cols_array()));
		for (mesh, material) in &parts[object.mesh] {
			let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id();
			scene.entities.push(entity);
		}
	}
}

/// Puts the 3D camera where the player's eye is, looking where they look; or
/// at the editor's camera while it's open.
fn sync_camera(
	renderer: Res<Renderer>,
	editor: Option<Res<Editor>>,
	mut cameras: Query<&mut Transform, With<WorldCamera>>,
) {
	let Ok(mut transform) = cameras.single_mut() else { return };
	if let Some(at) = editor.as_deref().and_then(editor::camera) {
		*transform = at;
		return;
	}
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
	/// The HUD cells last drawn into `image`
	hud: Vec<super::Cell>,
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
		// Black beyond the walls, like the pit under the stairs down
		Camera { order: 0, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
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
		// Underground: only a faint fill, so shapes still read away from torches
		DirectionalLight { illuminance: 1_200.0, ..default() },
		Transform::default().looking_to(light_dir, Vec3::Y),
	));
	commands.insert_resource(GlobalAmbientLight { brightness: AMBIENT, ..default() });
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
	commands.insert_resource(Display { image, sprite, scene, filter, filter_quad, hud: Vec::new() });
}

/// In a menu: W/S or the arrows move, Enter picks, A/D change values, Esc
/// goes back. Playing: one key press, one turn (holding a key does nothing
/// more), and the pause key pauses. The cursor is only locked while playing.
fn handle_input(
	keys: Res<ButtonInput<KeyCode>>,
	mut renderer: ResMut<Renderer>,
	mut exit: MessageWriter<AppExit>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	editor: Option<Res<Editor>>,
) {
	// F2 switches between ASCII and textured views, in menus, play or the editor
	if keys.just_pressed(KeyCode::F2) {
		renderer.settings.text_filter = !renderer.settings.text_filter;
	}
	// The editor has the other keys while it's open
	if editor.is_some_and(|e| e.active) {
		return;
	}
	let renderer = &mut *renderer;
	let before = renderer.screen;
	if renderer.screen != Screen::Playing {
		match renderer.screen.handle(&mut renderer.settings, &keys) {
			Some(Request::NewGame) => {
				if let Err(e) = renderer.new_game() {
					eprintln!("error: {e}");
					exit.write(AppExit::error());
					return;
				}
				renderer.screen = Screen::Playing;
			}
			Some(Request::Quit) => {
				exit.write(AppExit::Success);
				return;
			}
			None => {}
		}
	} else if keys.just_pressed(renderer.settings.key(Bind::Pause)) {
		renderer.screen = Screen::Paused { selected: 0 };
	} else {
		play_turn(&keys, renderer, &mut exit);
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
	// The bound key, plus the arrows, which always work
	let key = |bind| renderer.settings.key(bind);
	let bindings = [
		([key(Bind::Forward), KeyCode::ArrowUp], Action::Forward),
		([key(Bind::Back), KeyCode::ArrowDown], Action::Back),
		([key(Bind::TurnLeft), KeyCode::ArrowLeft], Action::TurnLeft),
		([key(Bind::TurnRight), KeyCode::ArrowRight], Action::TurnRight),
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

fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut renderer: ResMut<Renderer>, editor: Option<Res<Editor>>) {
	let editing = editor.is_some_and(|e| e.active);
	if motion.delta != Vec2::ZERO && renderer.screen == Screen::Playing && !editing {
		// Screen y grows downward, so moving the mouse up looks up
		renderer.look(motion.delta.x * MOUSE_SENSITIVITY, -motion.delta.y * MOUSE_SENSITIVITY);
	}
}

fn draw_frame(
	windows: Query<&Window, With<PrimaryWindow>>,
	mut screen: ResMut<Display>,
	mut renderer: ResMut<Renderer>,
	mut images: ResMut<Assets<Image>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut sprites: Query<&mut Sprite>,
	mut transforms: Query<&mut Transform, Without<WorldCamera>>,
	editor: Option<Res<Editor>>,
) {
	let Ok(window) = windows.single() else { return };

	// As many character cells as fit the window at the font's real pixel size
	let scale = renderer.settings.text_scale;
	let cell_w = GLYPH_W as u32 * scale;
	let cell_h = GLYPH_H as u32 * scale;
	let cols = (window.physical_width() / cell_w) as usize;
	let rows = (window.physical_height() / cell_h) as usize;
	if cols == 0 || rows == 0 {
		return;
	}
	renderer.render(cols, rows);
	if let Some(editor) = &editor {
		editor::draw_hud(editor, &mut renderer.framebuffer);
	}

	let size = Extent3d {
		width: (cols * GLYPH_W) as u32,
		height: (rows * GLYPH_H) as u32,
		depth_or_array_layers: 1,
	};
	// Redrawn and re-uploaded only when the text changed: the image is several MB
	if screen.hud != renderer.framebuffer.color_buffer {
		let Some(mut image) = images.get_mut(&screen.image) else { return };
		if image.texture_descriptor.size != size {
			image.resize(size);
		}
		let Some(pixels) = image.data.as_mut() else { return };
		renderer.glyphs.draw(&renderer.framebuffer, pixels);
		screen.hud.clone_from(&renderer.framebuffer.color_buffer);
	}

	// The 3D view renders at a fixed fraction of the screen, whatever the text
	// size; the filter averages however many of its pixels fall in a cell
	let scene_size = Extent3d {
		width: (cols as u32 * cell_w / SCENE_DIVISOR).max(1),
		height: (rows as u32 * cell_h / SCENE_DIVISOR).max(1),
		depth_or_array_layers: 1,
	};
	if images.get(&screen.scene).is_some_and(|s| s.texture_descriptor.size != scene_size) {
		images.insert(&screen.scene, scene_target(scene_size.width, scene_size.height)).ok();
	}
	let grid = Vec4::new(cols as f32, rows as f32, if renderer.settings.text_filter { 1.0 } else { 0.0 }, 0.0);
	if filters.get(&screen.filter).is_some_and(|f| f.grid != grid) {
		if let Some(mut filter) = filters.get_mut(&screen.filter) {
			filter.grid = grid;
		}
	}

	// Show both at the text scale's screen pixels per font pixel. Sizes are in
	// logical pixels, so divide out the display's scale factor.
	let logical = Vec2::new((cols as u32 * cell_w) as f32, (rows as u32 * cell_h) as f32) / window.scale_factor();
	if let Ok(mut sprite) = sprites.get_mut(screen.sprite) {
		sprite.custom_size = Some(logical);
	}
	if let Ok(mut transform) = transforms.get_mut(screen.filter_quad) {
		transform.scale = logical.extend(1.0);
	}
}
