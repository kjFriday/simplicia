// Stage 6: what's in the level besides the building. Chests go in first
// (before the clipping check, so furniture keeps out of them), then items and
// monsters once the check is done. Each takes a free floor tile in a room;
// `taken` keeps them off each other, the stairs, and hand-made cells.
use glam::{Mat4, Vec3};
use rand::{Rng, RngExt};

use super::builder::Builder;
use super::kit::{CHEST, CHEST_CHANCE, CHEST_OPEN};
use super::presets::GenSettings;
use crate::dungeons::mapgen::Room;
use crate::game::play::chests::Chest;
use crate::game::play::combat::{Enemy, EnemyKind};
use crate::game::play::items::{GroundItem, ItemKind};
use crate::game::play::terrain::Terrain;
use crate::game::play::world::{tile_center, Blocker};

/// Random floor tile inside a room, avoiding `taken` tiles. None if the room is full.
pub(super) fn free_tile(rng: &mut impl Rng, room: &Room, taken: &[(i32, i32)]) -> Option<(i32, i32)> {
	for _ in 0..32 {
		let tile = (rng.random_range(room.x1..room.x2), rng.random_range(room.y1..room.y2));
		if !taken.contains(&tile) {
			return Some(tile);
		}
	}
	None
}

/// Chests: always one by the way down, sometimes one in other rooms.
/// They're drawn by the view (they open), but block walking, and the
/// clipping check keeps furniture out of them. Returns the chests and the
/// meshes of a closed and an open one.
pub(super) fn place_chests(
	rng: &mut impl Rng,
	b: &mut Builder,
	rooms: &[Room],
	terrain: &Terrain,
	taken: &mut Vec<(i32, i32)>,
	blockers: &mut Vec<Blocker>,
) -> Result<(Vec<Chest>, [usize; 2]), String> {
	let last = rooms.len() - 1;
	let chest_meshes = [b.model(CHEST.0, CHEST.1)?, b.model(CHEST_OPEN, CHEST.1)?];
	let mut chests = Vec::new();
	for (i, room) in rooms.iter().enumerate().skip(1) {
		if i != last && !rng.random_bool(CHEST_CHANCE) {
			continue;
		}
		let Some(t) = free_tile(rng, room, taken) else { continue };
		taken.push(t);
		let yaw = rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2;
		let at = tile_center(t.0, t.1) + Vec3::Y * terrain.tile(t.0, t.1);
		let (min, max) = b.box_of(chest_meshes[0], Mat4::from_translation(at) * Mat4::from_rotation_y(yaw));
		b.add_fixed_box(min, max);
		blockers.push(Blocker { min, max });
		chests.push(Chest { id: chests.len() as u32, x: t.0, y: t.1, yaw, open: false });
	}
	Ok((chests, chest_meshes))
}

/// Items and monsters; deeper levels get more, and tougher, monsters.
pub(super) fn place_items_and_monsters(
	rng: &mut impl Rng,
	rooms: &[Room],
	settings: &GenSettings,
	depth: usize,
	taken: &mut Vec<(i32, i32)>,
) -> (Vec<GroundItem>, Vec<Enemy>) {
	let s = settings;
	let enemy_chance = (s.enemies as f64 / 100.0 + depth as f64 * 0.1).min(1.0);
	let mut enemies: Vec<Enemy> = Vec::new();
	let mut items: Vec<GroundItem> = Vec::new();
	for room in rooms.iter().skip(1) {
		if rng.random_bool(s.items as f64 / 100.0) {
			if let Some(t) = free_tile(rng, room, taken) {
				taken.push(t);
				let kind = ItemKind::random(rng);
				items.push(GroundItem { id: items.len() as u32, x: t.0, y: t.1, kind, from: None });
			}
		}
		if rng.random_bool(enemy_chance) {
			if let Some(t) = free_tile(rng, room, taken) {
				taken.push(t);
				let kind = EnemyKind::pick(rng, depth);
				enemies.push(Enemy::new(rng, enemies.len() as u32, kind, t.0, t.1));
			}
		}
	}
	(items, enemies)
}
