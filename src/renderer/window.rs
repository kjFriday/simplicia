// Shows the renderer in a Bevy window. Bevy draws the level's models in 3D;
// on top, the same view drawn into a character grid with the bitmap font is
// laid over it as one see-through sprite, plus the minimap.
use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::font::{GLYPH_H, GLYPH_W};
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
		app.add_systems(Startup, setup)
			.add_systems(Update, (handle_input, mouse_look, sync_scene, sync_camera, draw_frame).chain());
	}
}

/// The level whose models are spawned as Bevy 3D entities, and those entities.
#[derive(Resource, Default)]
struct Scene3d {
	depth: Option<usize>,
	entities: Vec<Entity>,
}

/// Marks the camera that draws the 3D models under the text.
#[derive(Component)]
struct WorldCamera;

/// Turns one of our meshes into a Bevy mesh with vertex colors. Every triangle
/// gets its own three vertices so its normal is flat, matching the text shading.
fn to_bevy_mesh(mesh: &super::Mesh) -> Mesh {
	use bevy::mesh::{Indices, PrimitiveTopology};
	let mut positions = Vec::with_capacity(mesh.indices.len() * 3);
	let mut normals = Vec::with_capacity(positions.capacity());
	let mut colors = Vec::with_capacity(positions.capacity());
	for tri in &mesh.indices {
		let [a, b, c] = tri.map(|i| mesh.vertices[i]);
		let n = (b.pos - a.pos).cross(c.pos - a.pos).normalize_or_zero();
		for v in [a, b, c] {
			positions.push(v.pos.to_array());
			normals.push(n.to_array());
			colors.push([v.color.x, v.color.y, v.color.z, 1.0]);
		}
	}
	let count = positions.len() as u32;
	Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
		.with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
		.with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
		.with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
		.with_inserted_indices(Indices::U32((0..count).collect()))
}

/// Respawns the 3D models whenever the player changes level.
fn sync_scene(
	mut commands: Commands,
	renderer: Res<Renderer>,
	mut scene: ResMut<Scene3d>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
) {
	if scene.depth == Some(renderer.depth) {
		return;
	}
	for entity in scene.entities.drain(..) {
		commands.entity(entity).despawn();
	}
	scene.depth = Some(renderer.depth);

	let world = &renderer.levels[renderer.depth];
	// Vertex colors carry the model colors; the material just lets light shade them
	let material = materials.add(StandardMaterial {
		base_color: Color::WHITE,
		perceptual_roughness: 1.0,
		cull_mode: None,
		double_sided: true,
		..default()
	});
	let handles: Vec<_> = world.meshes.iter().map(|m| meshes.add(to_bevy_mesh(m))).collect();
	for object in &world.objects {
		let entity = commands
			.spawn((
				Mesh3d(handles[object.mesh].clone()),
				MeshMaterial3d(material.clone()),
				Transform::from_matrix(Mat4::from_cols_array(&object.transform.to_cols_array())),
			))
			.id();
		scene.entities.push(entity);
	}
}

/// Puts the 3D camera where the player's eye is, looking where they look.
fn sync_camera(renderer: Res<Renderer>, mut cameras: Query<&mut Transform, With<WorldCamera>>) {
	let Ok(mut transform) = cameras.single_mut() else { return };
	let view = renderer.levels[renderer.depth].player.view_matrix();
	*transform = Transform::from_matrix(Mat4::from_cols_array(&view.inverse().to_cols_array()));
}

/// The texture the character grid is drawn into, and the sprite showing it.
#[derive(Resource)]
struct Screen {
	image: Handle<Image>,
	sprite: Entity,
}

fn setup(
	mut commands: Commands,
	mut images: ResMut<Assets<Image>>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	renderer: Res<Renderer>,
) {
	// The 3D models first, then the text drawn over them without clearing
	commands.spawn((
		Camera3d::default(),
		Camera { order: 0, ..default() },
		// No tonemapping, so the 3D colors stay close to the text's colors
		bevy::core_pipeline::tonemapping::Tonemapping::None,
		Projection::Perspective(PerspectiveProjection {
			fov: 60f32.to_radians(),
			near: super::NEAR_PLANE,
			far: super::FAR_PLANE,
			..default()
		}),
		WorldCamera,
	));
	commands.spawn((Camera2d, Camera { order: 1, clear_color: ClearColorConfig::None, ..default() }));

	// The same sun the text shading uses, plus a little ambient light
	let light_dir = Vec3::from_array(renderer.light_dir.to_array());
	commands.spawn((
		DirectionalLight { illuminance: 8_000.0, ..default() },
		Transform::default().looking_to(light_dir, Vec3::Y),
	));
	commands.insert_resource(GlobalAmbientLight { brightness: 300.0, ..default() });
	commands.init_resource::<Scene3d>();

	// Mouselook: keep the cursor hidden and pinned to the window
	if let Ok(mut cursor) = cursors.single_mut() {
		cursor.grab_mode = CursorGrabMode::Locked;
		cursor.visible = false;
	}

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

fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut renderer: ResMut<Renderer>) {
	if motion.delta != Vec2::ZERO {
		// Screen y grows downward, so moving the mouse up looks up
		renderer.look(motion.delta.x * MOUSE_SENSITIVITY, -motion.delta.y * MOUSE_SENSITIVITY);
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
