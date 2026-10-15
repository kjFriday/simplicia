// The 3D scene: what Bevy draws under the text. `sync_scene` spawns a level's
// models as Bevy entities whenever the player changes level (sprites.rs then
// keeps the monsters, items and chests in step turn by turn); `apply_fog`
// keeps the light and haze in step with the fog setting; and `sync_camera`
// puts the camera at the player's eye, or the editor's.
use bevy::prelude::*;

use crate::engine::icons::{IconSheet, Sprites};
use crate::engine::scene::{model_parts, ModelParts};
use crate::game::editor::{self, Editor};
use crate::game::play::world::World;
use crate::game::Game;
use super::effects::{self, EnemyAnim, Motion};
use super::portals::{self, PortalKind};
use super::setup::AMBIENT;
use super::sprites::{chest_transform, ChestModel, EnemyModel};
use super::WorldCamera;

/// Torch light: brightness in lumens and reach in meters.
const TORCH_LUMENS: f32 = 200_000.0;
const TORCH_RANGE: f32 = 14.0;
/// With the fog off (debug binary, F3): a moderate fill, enough to see the
/// whole level without washing it out.
const AMBIENT_NO_FOG: f32 = 350.0;
/// With the fog on, the distance haze: clear up to FOG_START meters, gone
/// to darkness by FOG_END.
const FOG_START: f32 = 8.0;
const FOG_END: f32 = 30.0;
const FOG_COLOR: Color = Color::srgb(0.012, 0.011, 0.01);

/// The level whose models are spawned as Bevy 3D entities, and those entities.
#[derive(Resource, Default)]
pub(super) struct Scene3d {
	/// (Game::run, depth) of the level shown
	shown: Option<(u32, usize)>,
	pub(super) entities: Vec<Entity>,
	/// A closed chest's model and an open one's, for swapping when one opens
	pub(super) chest_parts: Vec<ModelParts>,
}

/// The quads and per-icon materials sprites are drawn with, built once.
#[derive(Resource, Default)]
pub(super) struct SpriteCache(pub(super) Option<Sprites>);

/// Respawns the 3D models whenever the player changes level.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_scene(
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
	spawn_portals(&mut commands, &mut meshes, &mut materials, world, &mut scene);
	spawn_torches(&mut commands, world, &mut scene);
	// Enemies as sprites, tagged with their ids so `sync_enemies` can move
	// them and remove them when they die. (`sync_items` adds the items.)
	let sprites = cache.0.get_or_insert_with(|| Sprites::new(&mut meshes));
	spawn_enemies(&mut commands, world, &game.icons, sprites, &mut materials, &mut images, &mut scene);
	spawn_chests(&mut commands, world, &parts, &mut scene);
	spawn_objects(&mut commands, world, &parts, &mut scene);
}

/// Keeps the lighting in step with the fog. On: dim light and a haze that
/// swallows the distance. Off: a moderate, even light and no haze.
pub(super) fn apply_fog(
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

/// Puts the 3D camera where the player's eye is, looking where they look; or
/// at the editor's camera while it's open.
pub(super) fn sync_camera(
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

/// A glowing portal on each staircase.
fn spawn_portals(
	commands: &mut Commands,
	meshes: &mut Assets<Mesh>,
	materials: &mut Assets<StandardMaterial>,
	world: &World,
	scene: &mut Scene3d,
) {
	// Portals on the stairs: down always, up below the top level
	let down = world.tile_center_of(world.stairs_down.0, world.stairs_down.1);
	let mut portal_sites = vec![(down, PortalKind::Down)];
	if let Some(up) = world.stairs_up {
		portal_sites.push((world.tile_center_of(up.0, up.1), PortalKind::Up));
	}
	for (at, kind) in portal_sites {
		let at = Vec3::from_array(at.to_array());
		scene.entities.extend(portals::spawn(commands, meshes, materials, at, kind));
	}
}

/// A point light at every torch on the walls.
fn spawn_torches(commands: &mut Commands, world: &World, scene: &mut Scene3d) {
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
}

/// A sprite for every enemy on the level, standing on its tile.
fn spawn_enemies(
	commands: &mut Commands,
	world: &World,
	icons: &IconSheet,
	sprites: &mut Sprites,
	materials: &mut Assets<StandardMaterial>,
	images: &mut Assets<Image>,
	scene: &mut Scene3d,
) {
	for enemy in &world.enemies {
		let sprite = sprites.get(icons, enemy.icon, materials, images);
		let at = Vec3::from_array(world.tile_center_of(enemy.x, enemy.y).to_array());
		let anim = EnemyAnim { id: enemy.id, pos: at, hurt: -10.0 };
		scene.entities.push(commands.spawn((sprite, Transform::default(), EnemyModel(enemy.id), anim)).id());
	}
}

/// Every chest's model (`parts` holds the level's models), and the two chest
/// models kept for `sync_chests` to swap between.
fn spawn_chests(commands: &mut Commands, world: &World, parts: &[ModelParts], scene: &mut Scene3d) {
	// Chests, closed or open as they were left
	scene.chest_parts = world.chest_meshes.iter().map(|&m| parts[m].clone()).collect();
	for chest in &world.chests {
		let transform = chest_transform(world, chest);
		for (mesh, material) in scene.chest_parts[chest.open as usize].clone() {
			let model = ChestModel { id: chest.id, open: chest.open };
			let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform, model)).id();
			scene.entities.push(e);
		}
	}
}

/// The level's walls, floors and furniture, each a model placed by its transform.
fn spawn_objects(commands: &mut Commands, world: &World, parts: &[ModelParts], scene: &mut Scene3d) {
	for object in &world.objects {
		let transform = Transform::from_matrix(Mat4::from_cols_array(&object.transform.to_cols_array()));
		for (mesh, material) in &parts[object.mesh] {
			let entity = commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), transform)).id();
			scene.entities.push(entity);
		}
	}
}
