// Shows the game in a Bevy window. Bevy draws the level's models in 3D
// into an offscreen image; the text filter shader turns that image into
// characters on a quad filling the window; the HUD (the minimap) is drawn as
// text on a sprite above it.
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::image::ImageSampler;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::engine::font::{GLYPH_H, GLYPH_W};
use crate::engine::hud::Cell;
use crate::engine::icons::Sprites;
use crate::engine::scene::{model_parts, ModelParts, FAR_PLANE, NEAR_PLANE};
use crate::engine::text_filter::{TextFilter, TextFilterPlugin};
use super::editor::{self, Editor};
use super::controls::{self, Bind};
use super::effects::{self, EnemyAnim, Motion};
use super::menu::{Request, Screen};
use super::player::{Action, SPRINT_MULTIPLIER};
use super::portals::{self, PortalKind};
use super::Game;


/// The 3D view renders at 1 / this of the screen's resolution, in each
/// direction. Lower is sharper in textured mode but slower.
const SCENE_DIVISOR: u32 = 4;

/// Torch light: brightness in lumens and reach in meters.
const TORCH_LUMENS: f32 = 200_000.0;
const TORCH_RANGE: f32 = 14.0;
/// Light everywhere, so unlit corners are dim rather than black.
const AMBIENT: f32 = 50.0;
/// With the fog off (debug binary, F3): a moderate fill, enough to see the
/// whole level without washing it out.
const AMBIENT_NO_FOG: f32 = 350.0;
/// With the fog on, the distance haze: clear up to FOG_START meters, gone
/// to darkness by FOG_END.
const FOG_START: f32 = 8.0;
const FOG_END: f32 = 30.0;
const FOG_COLOR: Color = Color::srgb(0.012, 0.011, 0.01);

/// The lantern the player carries: brightness in lumens and reach in meters.
const LANTERN_LUMENS: f32 = 250_000.0;
const LANTERN_RANGE: f32 = 16.0;

/// Radians the view turns per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.003;

/// Runs the game in the primary window. Insert a `Game` resource
/// (from `Game::new`) before running.
pub struct GamePlugin;

impl Plugin for GamePlugin {
	fn build(&self, app: &mut App) {
		app.add_plugins(TextFilterPlugin)
			.init_resource::<SpriteCache>()
			.add_systems(Startup, (setup, effects::setup).chain())
			.add_systems(
				Update,
				(
					handle_input,
					editor::update.run_if(resource_exists::<Editor>),
					mouse_look,
					sync_scene,
					effects::play_events,
					sync_enemies,
					sync_items,
					sync_chests,
					portals::animate,
					apply_fog,
					sync_camera,
					effects::animate_weapon,
					effects::update_particles,
					effects::ambient_particles,
					draw_frame,
				)
					.chain(),
			);
		// The level editor (F1) is only in the debug binary
		if crate::game::dev_tools() {
			app.init_resource::<Editor>();
		}
	}
}

/// The level whose models are spawned as Bevy 3D entities, and those entities.
#[derive(Resource, Default)]
struct Scene3d {
	/// (Game::run, depth) of the level shown
	shown: Option<(u32, usize)>,
	entities: Vec<Entity>,
	/// A closed chest's model and an open one's, for swapping when one opens
	chest_parts: Vec<ModelParts>,
}

/// The quads and per-icon materials sprites are drawn with, built once.
#[derive(Resource, Default)]
struct SpriteCache(Option<Sprites>);

/// Marks the camera that draws the 3D models under the text.
#[derive(Component)]
pub(super) struct WorldCamera;

/// Keeps the lighting in step with the fog. On: dim light and a haze that
/// swallows the distance. Off: a moderate, even light and no haze.
fn apply_fog(
	mut commands: Commands,
	game: Res<Game>,
	mut ambient: ResMut<GlobalAmbientLight>,
	cameras: Query<(Entity, Has<DistanceFog>), With<WorldCamera>>,
) {
	let brightness = if game.fog { AMBIENT } else { AMBIENT_NO_FOG };
	if ambient.brightness != brightness {
		ambient.brightness = brightness;
	}
	let Ok((camera, hazy)) = cameras.single() else { return };
	if game.fog && !hazy {
		commands.entity(camera).insert(DistanceFog {
			color: FOG_COLOR,
			falloff: FogFalloff::Linear { start: FOG_START, end: FOG_END },
			..default()
		});
	} else if !game.fog && hazy {
		commands.entity(camera).remove::<DistanceFog>();
	}
}

/// An enemy's sprite, by the enemy's id.
#[derive(Component)]
struct EnemyModel(u32);

/// An item's sprite, by the item's id.
#[derive(Component)]
struct ItemModel(u32);

/// An item flying out of a chest: from where, and since when.
#[derive(Component)]
struct Pop {
	from: Vec3,
	start: f32,
}

/// How long an item takes to fly out of a chest, and how high it arcs.
const POP_TIME: f32 = 0.6;
const POP_HEIGHT: f32 = 1.4;

/// A part of a chest's model, by the chest's id, and whether it's the open one.
#[derive(Component)]
struct ChestModel {
	id: u32,
	open: bool,
}

/// Sprite sizes in meters, and how high items float above the floor.
const ENEMY_SIZE: f32 = 1.8;
const ITEM_SIZE: f32 = 0.8;
const ITEM_HOVER: f32 = 0.5;

/// A sprite standing at `foot` (the floor under its middle), `size` across,
/// turned about Y to face `eye`.
fn billboard(foot: Vec3, size: f32, eye: Vec3) -> Transform {
	let center = foot + Vec3::Y * (size / 2.0);
	let to_eye = Vec3::new(eye.x - center.x, 0.0, eye.z - center.z);
	// The quad faces +Z; turn that toward the eye
	let yaw = to_eye.x.atan2(to_eye.z);
	Transform::from_translation(center).with_rotation(Quat::from_rotation_y(yaw)).with_scale(Vec3::new(size, size, 1.0))
}

/// The camera's position: where the 3D view is drawn from.
fn eye_of(cameras: &Query<&Transform, With<WorldCamera>>) -> Vec3 {
	cameras.single().map(|t| t.translation).unwrap_or_default()
}

/// Floats each item on its tile, facing the camera; removes the ones picked
/// up, and adds sprites for new ones. Items out of a chest fly out of it in
/// an arc.
#[allow(clippy::too_many_arguments)]
fn sync_items(
	mut commands: Commands,
	mut game: ResMut<Game>,
	time: Res<Time>,
	mut scene: ResMut<Scene3d>,
	mut cache: ResMut<SpriteCache>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut images: ResMut<Assets<Image>>,
	cameras: Query<&Transform, With<WorldCamera>>,
	mut models: Query<(Entity, &ItemModel, Option<&Pop>, &mut Transform), Without<WorldCamera>>,
) {
	let game = &mut *game;
	let world = &mut game.levels[game.depth];
	let eye = eye_of(&cameras);
	let t = time.elapsed_secs();
	let mut shown = Vec::new();
	for (entity, model, pop, mut transform) in &mut models {
		let Some(item) = world.items.iter().find(|it| it.id == model.0) else {
			commands.entity(entity).despawn();
			continue;
		};
		shown.push(item.id);
		let floor = Vec3::from_array(world.tile_center_of(item.x, item.y).to_array());
		let bob = (t * 2.0 + item.id as f32).sin() * 0.1;
		let rest = floor + Vec3::Y * (ITEM_HOVER + bob);
		let at = match pop {
			Some(p) => {
				let k = ((t - p.start) / POP_TIME).clamp(0.0, 1.0);
				p.from.lerp(rest, k) + Vec3::Y * (std::f32::consts::PI * k).sin() * POP_HEIGHT
			}
			None => rest,
		};
		*transform = billboard(at, ITEM_SIZE, eye);
	}

	// Sprites for items that don't have one yet: a new level's, or loot. Loot
	// flies out of its chest once; after that it just lies there
	let new: Vec<(u32, u32, Option<Vec3>)> = world
		.items
		.iter()
		.filter(|it| !shown.contains(&it.id))
		.map(|it| {
			let lid = it.from.map(|(cx, cy)| Vec3::from_array(world.tile_center_of(cx, cy).to_array()) + Vec3::Y * 0.6);
			(it.id, it.kind.icon(), lid)
		})
		.collect();
	for it in world.items.iter_mut() {
		it.from = None;
	}
	let sprites = cache.0.get_or_insert_with(|| Sprites::new(&mut meshes));
	for (id, icon, lid) in new {
		let sprite = sprites.get(&game.icons, icon, &mut materials, &mut images);
		let mut entity = commands.spawn((sprite, Transform::from_scale(Vec3::ZERO), ItemModel(id)));
		if let Some(from) = lid {
			entity.insert(Pop { from, start: t });
		}
		scene.entities.push(entity.id());
	}
}

/// Moves each enemy's sprite toward its tile, facing the camera and moving
/// as if alive (see `effects::animate_enemy`); removes the sprites of
/// enemies that have died.
fn sync_enemies(
	mut commands: Commands,
	game: Res<Game>,
	time: Res<Time>,
	cameras: Query<&Transform, With<WorldCamera>>,
	mut models: Query<(Entity, &EnemyModel, &mut EnemyAnim, &mut Transform), Without<WorldCamera>>,
) {
	let world = &game.levels[game.depth];
	let eye = eye_of(&cameras);
	let (t, dt) = (time.elapsed_secs(), time.delta_secs());
	for (entity, model, mut anim, mut transform) in &mut models {
		let Some(enemy) = world.enemies.iter().find(|e| e.id == model.0) else {
			commands.entity(entity).despawn();
			continue;
		};
		let floor = Vec3::from_array(world.tile_center_of(enemy.x, enemy.y).to_array());
		*transform = effects::animate_enemy(&mut anim, floor, |at| billboard(at, ENEMY_SIZE, eye), t, dt);
	}
}

/// Swaps a chest's model for the open one when it opens.
fn sync_chests(
	mut commands: Commands,
	game: Res<Game>,
	mut scene: ResMut<Scene3d>,
	models: Query<(Entity, &ChestModel)>,
) {
	let world = &game.levels[game.depth];
	for chest in world.chests.iter().filter(|c| c.open) {
		if !models.iter().any(|(_, m)| m.id == chest.id && !m.open) {
			continue;
		}
		for (entity, m) in &models {
			if m.id == chest.id {
				commands.entity(entity).despawn();
			}
		}
		let Some(parts) = scene.chest_parts.get(1).cloned() else { continue };
		let transform = chest_transform(world, chest);
		for (mesh, material) in parts {
			let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform, ChestModel { id: chest.id, open: true })).id();
			scene.entities.push(e);
		}
	}
}

fn chest_transform(world: &super::world::World, chest: &super::chests::Chest) -> Transform {
	let at = Vec3::from_array(world.tile_center_of(chest.x, chest.y).to_array());
	Transform::from_translation(at).with_rotation(Quat::from_rotation_y(chest.yaw))
}

/// Respawns the 3D models whenever the player changes level.
#[allow(clippy::too_many_arguments)]
fn sync_scene(
	mut commands: Commands,
	game: Res<Game>,
	mut scene: ResMut<Scene3d>,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut images: ResMut<Assets<Image>>,
	mut cache: ResMut<SpriteCache>,
	particles: Query<Entity, With<effects::Particle>>,
) {
	if scene.shown == Some((game.run, game.depth)) {
		return;
	}
	// Some were already removed (dead enemies, picked-up items)
	for entity in scene.entities.drain(..) {
		if let Ok(mut e) = commands.get_entity(entity) {
			e.despawn();
		}
	}
	// Sparks and dust from the level left behind
	for entity in &particles {
		commands.entity(entity).despawn();
	}
	scene.shown = Some((game.run, game.depth));

	let world = &game.levels[game.depth];
	let parts: Vec<_> =
		world.meshes.iter().map(|m| model_parts(m, &mut meshes, &mut materials, &mut images)).collect();
	// Portals on the stairs: down always, up below the top level
	let down = world.tile_center_of(world.stairs_down.0, world.stairs_down.1);
	let mut portal_sites = vec![(down, PortalKind::Down)];
	if let Some(up) = world.stairs_up {
		portal_sites.push((world.tile_center_of(up.0, up.1), PortalKind::Up));
	}
	for (at, kind) in portal_sites {
		let at = Vec3::from_array(at.to_array());
		scene.entities.extend(portals::spawn(&mut commands, &mut meshes, &mut materials, at, kind));
	}

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
	// Enemies as sprites, tagged with their ids so `sync_enemies` can move
	// them and remove them when they die. (`sync_items` adds the items.)
	let sprites = cache.0.get_or_insert_with(|| Sprites::new(&mut meshes));
	for enemy in &world.enemies {
		let sprite = sprites.get(&game.icons, enemy.icon, &mut materials, &mut images);
		let at = Vec3::from_array(world.tile_center_of(enemy.x, enemy.y).to_array());
		let anim = EnemyAnim { id: enemy.id, pos: at, hurt: -10.0 };
		scene.entities.push(commands.spawn((sprite, Transform::default(), EnemyModel(enemy.id), anim)).id());
	}
	// Chests, closed or open as they were left
	scene.chest_parts = world.chest_meshes.iter().map(|&m| parts[m].clone()).collect();
	for chest in &world.chests {
		let transform = chest_transform(world, chest);
		for (mesh, material) in scene.chest_parts[chest.open as usize].clone() {
			let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform, ChestModel { id: chest.id, open: chest.open })).id();
			scene.entities.push(e);
		}
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
	game: Res<Game>,
	editor: Option<Res<Editor>>,
	time: Res<Time>,
	motion: Res<Motion>,
	mut cameras: Query<&mut Transform, With<WorldCamera>>,
) {
	let Ok(mut transform) = cameras.single_mut() else { return };
	if let Some(at) = editor.as_deref().and_then(editor::camera) {
		*transform = at;
		return;
	}
	let view = game.levels[game.depth].player.view_matrix();
	*transform = Transform::from_matrix(Mat4::from_cols_array(&view.inverse().to_cols_array()));
	// Knocked about for a moment when hurt
	let shake = effects::shake_offset(&motion, time.elapsed_secs());
	let shake_translation = transform.rotation * shake;
	transform.translation += shake_translation;
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
	hud: Vec<Cell>,
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
	game: Res<Game>,
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

	// Underground there's no sun: the lantern and torches light the way, over
	// a dim ambient fill so unlit corners aren't pitch black
	commands.insert_resource(GlobalAmbientLight { brightness: AMBIENT, ..default() });
	commands.init_resource::<Scene3d>();

	// The game opens on the main menu, so the cursor starts free
	set_cursor_locked(&mut cursors, game.screen == Screen::Playing);

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
	buttons: Res<ButtonInput<MouseButton>>,
	time: Res<Time>,
	mut game: ResMut<Game>,
	mut exit: MessageWriter<AppExit>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	editor: Option<Res<Editor>>,
) {
	// F2 switches between ASCII and textured views, in menus, play or the editor
	// F3 (debug binary): fog on or off; the light and the map follow it
	if crate::game::dev_tools() && keys.just_pressed(controls::TOGGLE_FOG) {
		game.fog = !game.fog;
	}
	if keys.just_pressed(controls::TOGGLE_ASCII) {
		game.settings.text_filter = !game.settings.text_filter;
	}
	// The editor has the other keys while it's open
	if editor.is_some_and(|e| e.active) {
		return;
	}
	let game = &mut *game;
	let before = game.screen;
	if let Screen::Inventory { selected } = game.screen {
		let world = &mut game.levels[game.depth];
		let count = world.hero.bag.len().max(1) as i32;
		let step = |s: usize, d: i32| (s as i32 + d).rem_euclid(count) as usize;
		if keys.any_just_pressed(controls::MENU_UP) {
			game.screen = Screen::Inventory { selected: step(selected, -1) };
		} else if keys.any_just_pressed(controls::MENU_DOWN) {
			game.screen = Screen::Inventory { selected: step(selected, 1) };
		} else if keys.any_just_pressed(controls::MENU_SELECT) {
			world.use_item(selected);
			let count = world.hero.bag.len().max(1);
			game.screen = Screen::Inventory { selected: selected.min(count - 1) };
		} else if keys.any_just_pressed([controls::MENU_BACK, game.settings.key(Bind::Inventory)]) {
			game.screen = Screen::Playing;
		}
		// A potion drunk in combat can be the last thing the player does
		game.check_death();
	} else if game.screen != Screen::Playing {
		match game.screen.handle(&mut game.settings, &keys) {
			Some(Request::NewGame) => {
				if let Err(e) = game.new_game() {
					eprintln!("error: {e}");
					exit.write(AppExit::error());
					return;
				}
				game.screen = Screen::Playing;
			}
			Some(Request::SavePreset) => {
				let message = game.save_preset();
				eprintln!("{message}");
			}
			Some(Request::PlayGenerated) => {
				if let Err(e) = game.play_generated() {
					eprintln!("error: {e}");
					exit.write(AppExit::error());
					return;
				}
				game.screen = Screen::Playing;
			}
			Some(Request::Quit) => {
				exit.write(AppExit::Success);
				return;
			}
			None => {}
		}
	} else if keys.just_pressed(game.settings.key(Bind::Pause)) {
		game.screen = Screen::Paused { selected: 0 };
	} else if keys.just_pressed(game.settings.key(Bind::Inventory)) {
		game.screen = Screen::Inventory { selected: 0 };
	} else if keys.just_pressed(game.settings.key(Bind::Map)) {
		game.settings.map = game.settings.map.next();
	} else if buttons.just_pressed(MouseButton::Left) || keys.just_pressed(game.settings.key(Bind::Attack)) {
		if let Err(e) = game.attack() {
			eprintln!("error: {e}");
			exit.write(AppExit::error());
			return;
		}
	} else {
		play_turn(&keys, game, &mut exit, time.delta_secs());
	}

	// Lock the cursor for mouselook while playing; free it in menus
	if (before == Screen::Playing) != (game.screen == Screen::Playing) {
		set_cursor_locked(&mut cursors, game.screen == Screen::Playing);
	}
}
fn set_cursor_locked(cursors: &mut Query<&mut CursorOptions, With<PrimaryWindow>>, locked: bool) {
	if let Ok(mut cursor) = cursors.single_mut() {
		cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
		cursor.visible = !locked;
	}
}

/// Outside combat: walk freely while keys are held, with Left/Right
/// stepping sideways. In combat: one key press, one turn.
fn play_turn(keys: &ButtonInput<KeyCode>, game: &mut Game, exit: &mut MessageWriter<AppExit>, dt: f32) {
	// The bound key, plus the arrows, which always work
	let key = |bind| game.settings.key(bind);
	if !game.levels[game.depth].combat {
		let held = |bind: Bind| if keys.any_pressed([Some(key(bind)), bind.arrow()].into_iter().flatten()) { 1.0 } else { 0.0 };
		let forward = held(Bind::Forward) - held(Bind::Back);
		let strafe = held(Bind::TurnRight) - held(Bind::TurnLeft);
		if forward != 0.0 || strafe != 0.0 {
			let sprinting = keys.pressed(key(Bind::Sprint));
			let dt = if sprinting { dt * SPRINT_MULTIPLIER } else { dt };
			if let Err(e) = game.walk(forward, strafe, dt) {
				eprintln!("error: {e}");
				exit.write(AppExit::error());
			}
		}
		return;
	}
	let bindings = [
		(Bind::Forward, Action::Forward),
		(Bind::Back, Action::Back),
		(Bind::TurnLeft, Action::TurnLeft),
		(Bind::TurnRight, Action::TurnRight),
	];
	let pressed: Vec<Action> = bindings
		.into_iter()
		.filter(|&(bind, _)| keys.any_just_pressed([Some(key(bind)), bind.arrow()].into_iter().flatten()))
		.map(|(_, action)| action)
		.collect();
	for action in pressed {
		if let Err(e) = game.act(action) {
			eprintln!("error: {e}");
			exit.write(AppExit::error());
			return;
		}
	}
}

fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut game: ResMut<Game>, editor: Option<Res<Editor>>) {
	let editing = editor.is_some_and(|e| e.active);
	if motion.delta != Vec2::ZERO && game.screen == Screen::Playing && !editing {
		// Screen y grows downward, so moving the mouse up looks up
		game.look(motion.delta.x * MOUSE_SENSITIVITY, -motion.delta.y * MOUSE_SENSITIVITY);
	}
}

fn draw_frame(
	windows: Query<&Window, With<PrimaryWindow>>,
	mut screen: ResMut<Display>,
	mut game: ResMut<Game>,
	mut images: ResMut<Assets<Image>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut sprites: Query<&mut Sprite>,
	mut transforms: Query<&mut Transform, Without<WorldCamera>>,
	editor: Option<Res<Editor>>,
) {
	let Ok(window) = windows.single() else { return };

	// As many character cells as fit the window at the font's real pixel size
	let scale = game.settings.text_scale;
	let cell_w = GLYPH_W as u32 * scale;
	let cell_h = GLYPH_H as u32 * scale;
	let cols = (window.physical_width() / cell_w) as usize;
	let rows = (window.physical_height() / cell_h) as usize;
	if cols == 0 || rows == 0 {
		return;
	}
	game.render(cols, rows, editor.as_ref().is_some_and(|e| e.active));
	if let Some(editor) = &editor {
		editor::draw_hud(editor, &mut game.framebuffer);
	}

	let size = Extent3d {
		width: (cols * GLYPH_W) as u32,
		height: (rows * GLYPH_H) as u32,
		depth_or_array_layers: 1,
	};
	// Redrawn and re-uploaded only when the text changed: the image is several MB
	if screen.hud != game.framebuffer.color_buffer {
		let Some(mut image) = images.get_mut(&screen.image) else { return };
		if image.texture_descriptor.size != size {
			image.resize(size);
		}
		let Some(pixels) = image.data.as_mut() else { return };
		game.glyphs.draw(&game.framebuffer, pixels);
		screen.hud.clone_from(&game.framebuffer.color_buffer);
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
	let grid = Vec4::new(cols as f32, rows as f32, if game.settings.text_filter { 1.0 } else { 0.0 }, 0.0);
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
