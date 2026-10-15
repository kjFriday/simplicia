// Stage 2, the level's shape: decides what each room is like (level,
// raised, sunken, or a tall hall with a gallery), sets every tile's floor
// height, turns the corridor tiles at raised and sunken rooms' entrances into
// stair tiles, and drops in ledges and pits. tiles.rs then builds each tile
// to match.
use std::collections::HashMap;

use rand::{Rng, RngExt};

use super::catalog::RoomTheme;
use super::kit::{LEDGE, LEDGE_CHANCE, RAISED_CHANCE, ROOM_DENSITY, ROOM_RISE, SUNKEN_CHANCE, TALL_CHANCE};
use crate::dungeons::mapgen::{MapGrid, Room};
use crate::game::play::terrain::{Ramp, Terrain};
use crate::game::play::world::DIRECTIONS;

/// How a room is shaped and furnished.
#[derive(Clone, Copy, Debug)]
pub(super) struct RoomPlan {
	/// Floor height relative to the corridors
	pub(super) height: f32,
	pub(super) tall: bool,
	/// Tall halls: a gallery runs around the walls
	pub(super) gallery: bool,
	/// What it's furnished as; none for the room the player starts in
	pub(super) theme: Option<RoomTheme>,
	/// Share of its tiles that get furnished: some rooms are cluttered,
	/// some sparse
	pub(super) density: f64,
}

/// Raised or sunk parts of a room's floor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Feature {
	Ledge,
	Pit,
}

/// Which room each tile belongs to.
pub(super) struct RoomIndex {
	width: usize,
	rooms: Vec<Option<usize>>,
}

impl RoomIndex {
	pub(super) fn new(map: &MapGrid, rooms: &[Room]) -> Self {
		let mut index = vec![None; map.x * map.y];
		for (i, r) in rooms.iter().enumerate() {
			for x in r.x1..r.x2 {
				for y in r.y1..r.y2 {
					if x >= 0 && y >= 0 && (x as usize) < map.x && (y as usize) < map.y {
						index[y as usize * map.x + x as usize] = Some(i);
					}
				}
			}
		}
		Self { width: map.x, rooms: index }
	}

	pub(super) fn of(&self, x: i32, y: i32) -> Option<usize> {
		if x < 0 || y < 0 || x as usize >= self.width {
			return None;
		}
		self.rooms.get(y as usize * self.width + x as usize).copied().flatten()
	}
}

/// Decides each room's shape, then sets the floor heights: room floors, the
/// stair tiles into raised and sunken rooms, and ledges and pits. A raised
/// or sunken room whose entrances can't all take a stair tile stays level.
pub(super) fn shape_rooms(
	rng: &mut impl Rng,
	map: &MapGrid,
	rooms: &[Room],
	index: &RoomIndex,
	keep_flat: &[(i32, i32)],
) -> (Vec<RoomPlan>, Terrain, HashMap<(i32, i32), Feature>) {
	let mut terrain = Terrain::flat(map.x, map.y);
	let mut features = HashMap::new();
	let mut plans = plan_rooms(rng, rooms);
	place_stair_tiles(&mut plans, &mut terrain, map, rooms, index, keep_flat);
	set_floors(rng, &plans, &mut terrain, &mut features, rooms, keep_flat);
	(plans, terrain, features)
}

/// Rolls each room's theme, density, and shape. The first room, where the
/// player starts, stays plain.
fn plan_rooms(rng: &mut impl Rng, rooms: &[Room]) -> Vec<RoomPlan> {
	rooms
		.iter()
		.enumerate()
		.map(|(i, r)| {
			let (w, h) = (r.x2 - r.x1, r.y2 - r.y1);
			let mut plan = RoomPlan { height: 0.0, tall: false, gallery: false, theme: None, density: 0.0 };
			if i == 0 {
				return plan;
			}
			plan.theme = Some(RoomTheme::ALL[rng.random_range(0..RoomTheme::ALL.len())]);
			plan.density = rng.random_range(ROOM_DENSITY.0..ROOM_DENSITY.1);
			let roll = rng.random_range(0.0..1.0);
			if roll < TALL_CHANCE && w >= 3 && h >= 3 {
				plan.tall = true;
				plan.gallery = rng.random_bool(0.6);
			} else if roll < TALL_CHANCE + RAISED_CHANCE {
				plan.height = ROOM_RISE;
			} else if roll < TALL_CHANCE + RAISED_CHANCE + SUNKEN_CHANCE {
				plan.height = -ROOM_RISE;
			}
			plan
		})
		.collect()
}

/// Stair tiles: each corridor tile next to a raised or sunken room becomes
/// one, if it's a clean entrance (one way in, corridor straight behind it).
/// A room with any entrance that isn't clean is set back to level.
fn place_stair_tiles(
	plans: &mut [RoomPlan],
	terrain: &mut Terrain,
	map: &MapGrid,
	rooms: &[Room],
	index: &RoomIndex,
	keep_flat: &[(i32, i32)],
) {
	for (i, plan) in plans.iter_mut().enumerate() {
		if plan.height == 0.0 {
			continue;
		}
		let r = &rooms[i];
		let mut stairs = Vec::new();
		let mut clean = true;
		for x in r.x1 - 1..=r.x2 {
			for y in r.y1 - 1..=r.y2 {
				if map.is_wall(x, y) || index.of(x, y).is_some() {
					continue;
				}
				let into: Vec<usize> =
					(0..4).filter(|&d| index.of(x + DIRECTIONS[d].0, y + DIRECTIONS[d].1) == Some(i)).collect();
				if into.is_empty() {
					continue;
				}
				let d = into[0];
				let (bx, by) = (x - DIRECTIONS[d].0, y - DIRECTIONS[d].1);
				let other_room =
					(0..4).any(|k| index.of(x + DIRECTIONS[k].0, y + DIRECTIONS[k].1).is_some_and(|o| o != i));
				if into.len() != 1
					|| other_room
					|| map.is_wall(bx, by)
					|| index.of(bx, by).is_some()
					|| keep_flat.contains(&(x, y))
				{
					clean = false;
				}
				stairs.push((x, y, d));
			}
		}
		if !clean || stairs.is_empty() {
			plan.height = 0.0;
			continue;
		}
		for (x, y, d) in stairs {
			let ramp = if plan.height > 0.0 {
				Ramp { dir: d, low: 0.0, high: plan.height }
			} else {
				Ramp { dir: (d + 2) % 4, low: plan.height, high: 0.0 }
			};
			terrain.set_ramp(x, y, ramp);
		}
	}
}

/// Sets every room's floor to its height, then raises ledges and sinks pits
/// in some of them.
fn set_floors(
	rng: &mut impl Rng,
	plans: &[RoomPlan],
	terrain: &mut Terrain,
	features: &mut HashMap<(i32, i32), Feature>,
	rooms: &[Room],
	keep_flat: &[(i32, i32)],
) {
	for (i, r) in rooms.iter().enumerate() {
		for x in r.x1..r.x2 {
			for y in r.y1..r.y2 {
				terrain.set(x, y, plans[i].height);
			}
		}
		// Ledges and pits, away from the room's edges (so doorways stay level)
		let (w, h) = (r.x2 - r.x1, r.y2 - r.y1);
		if i == 0 || plans[i].tall || w < 4 || h < 4 || !rng.random_bool(LEDGE_CHANCE) {
			continue;
		}
		let feature = if rng.random_bool(0.5) { Feature::Ledge } else { Feature::Pit };
		let (fw, fh) = (rng.random_range(1..=(w - 2).min(3)), rng.random_range(1..=(h - 2).min(3)));
		let (fx, fy) = (rng.random_range(r.x1 + 1..=r.x2 - 1 - fw), rng.random_range(r.y1 + 1..=r.y2 - 1 - fh));
		for x in fx..fx + fw {
			for y in fy..fy + fh {
				if keep_flat.contains(&(x, y)) {
					continue;
				}
				let rise = if feature == Feature::Ledge { LEDGE } else { -LEDGE };
				terrain.set(x, y, plans[i].height + rise);
				features.insert((x, y), feature);
			}
		}
	}
}
