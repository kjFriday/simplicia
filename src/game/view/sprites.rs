// Monsters and items are flat sprites (pictures from the Raven icon sheet)
// that turn to face the camera; chests are 3D models. scene.rs spawns them
// when a level is shown; the systems here keep them in step with the level
// every frame: moving monsters, removing the dead and picked up, adding loot
// as it flies out of a chest, and swapping a chest's model when it opens.
use bevy::prelude::*;

use crate::engine::icons::Sprites;
use crate::game::play::chests::Chest;
use crate::game::play::world::World;
use crate::game::Game;
use super::effects::{self, EnemyAnim};
use super::scene::{Scene3d, SpriteCache};
use super::WorldCamera;

/// How long an item takes to fly out of a chest, and how high it arcs.
const POP_TIME: f32 = 0.6;
const POP_HEIGHT: f32 = 1.4;

/// Sprite sizes in meters, and how high items float above the floor.
const ENEMY_SIZE: f32 = 1.8;
const ITEM_SIZE: f32 = 0.8;
const ITEM_HOVER: f32 = 0.5;

/// An enemy's sprite, by the enemy's id.
#[derive(Component)]
pub(super) struct EnemyModel(pub(super) u32);

/// An item's sprite, by the item's id.
#[derive(Component)]
pub(super) struct ItemModel(u32);

/// An item flying out of a chest: from where, and since when.
#[derive(Component)]
pub(super) struct Pop {
	from: Vec3,
	start: f32,
}

/// A part of a chest's model, by the chest's id, and whether it's the open one.
#[derive(Component)]
pub(super) struct ChestModel {
	pub(super) id: u32,
	pub(super) open: bool,
}

/// Moves each enemy's sprite toward its tile, facing the camera and moving
/// as if alive (see `effects::animate_enemy`); removes the sprites of
/// enemies that have died.
pub(super) fn sync_enemies(
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

/// Floats each item on its tile, facing the camera; removes the ones picked
/// up, and adds sprites for new ones. Items out of a chest fly out of it in
/// an arc.
#[allow(clippy::too_many_arguments)]
pub(super) fn sync_items(
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
		*transform = billboard(item_position(rest, pop, t), ITEM_SIZE, eye);
	}

	// Sprites for items that don't have one yet: a new level's, or loot. Loot
	// flies out of its chest once; after that it just lies there
	let new: Vec<(u32, u32, Option<Vec3>)> = world
		.items
		.iter()
		.filter(|it| !shown.contains(&it.id))
		.map(|it| {
			let lid = it
				.from
				.map(|(cx, cy)| Vec3::from_array(world.tile_center_of(cx, cy).to_array()) + Vec3::Y * 0.6);
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

/// Swaps a chest's model for the open one when it opens.
pub(super) fn sync_chests(
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
			let model = ChestModel { id: chest.id, open: true };
			let e = commands.spawn((Mesh3d(mesh), MeshMaterial3d(material), transform, model)).id();
			scene.entities.push(e);
		}
	}
}

/// Where a chest's model stands: on its tile, turned to face the way it faces.
pub(super) fn chest_transform(world: &World, chest: &Chest) -> Transform {
	let at = Vec3::from_array(world.tile_center_of(chest.x, chest.y).to_array());
	Transform::from_translation(at).with_rotation(Quat::from_rotation_y(chest.yaw))
}

/// A sprite standing at `foot` (the floor under its middle), `size` across,
/// turned about Y to face `eye`.
fn billboard(foot: Vec3, size: f32, eye: Vec3) -> Transform {
	let center = foot + Vec3::Y * (size / 2.0);
	let to_eye = Vec3::new(eye.x - center.x, 0.0, eye.z - center.z);
	// The quad faces +Z; turn that toward the eye
	let yaw = to_eye.x.atan2(to_eye.z);
	Transform::from_translation(center)
		.with_rotation(Quat::from_rotation_y(yaw))
		.with_scale(Vec3::new(size, size, 1.0))
}

/// The camera's position: where the 3D view is drawn from.
fn eye_of(cameras: &Query<&Transform, With<WorldCamera>>) -> Vec3 {
	cameras.single().map(|t| t.translation).unwrap_or_default()
}

/// Where an item floats at time `t`: at `rest` over its tile, or, while it's
/// flying out of a chest (`pop`), somewhere along the arc to there.
fn item_position(rest: Vec3, pop: Option<&Pop>, t: f32) -> Vec3 {
	match pop {
		Some(p) => {
			let k = ((t - p.start) / POP_TIME).clamp(0.0, 1.0);
			p.from.lerp(rest, k) + Vec3::Y * (std::f32::consts::PI * k).sin() * POP_HEIGHT
		}
		None => rest,
	}
}
