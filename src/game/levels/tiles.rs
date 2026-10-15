// Stages 3 and 4: building every walkable tile from the dungeon kit and
// furnishing it. Each tile gets, in this order: a hand-made cell if one fits
// (and then nothing else), or else its floor piece, a ledge block, what
// stands on each edge (walls, torches, arches, pit faces), pillars in its
// corners, a hall's gallery and chandelier, and a generated cell's
// furnishings. The order matters: the clipping check lets earlier props win.
//
// mod.rs calls `build_tiles` with the shape from shape.rs; the models are
// placed through builder.rs.
use std::collections::HashMap;

use glam::Vec3;
use rand::{Rng, RngExt};

use super::builder::{place_cell, Builder};
use super::cells::{self, Cell, CellKind};
use super::kit::{
	facing_yaw, KitMeshes, GALLERY, HALL_CEILING, HALL_CELL_CHANCE, LEDGE, MAX_TORCHES, STAIRS_DOWN_DEPTH, STRETCH,
	STRETCH_UP, TORCH_CHANCE, WALL_HEIGHT,
};
use super::shape::{Feature, RoomIndex, RoomPlan};
use crate::dungeons::mapgen::{MapGrid, Room};
use crate::game::play::terrain::{Ramp, Terrain};
use crate::game::play::world::{
	open_facing, tile_center, Blocker, ARCH_OPENING, DIRECTIONS, TILE, TORCH_DEPTH, WALL_THICKNESS,
};

/// The level as laid out and shaped so far, which every tile is built from.
pub(super) struct Layout<'a> {
	pub(super) map: &'a MapGrid,
	pub(super) rooms: &'a [Room],
	pub(super) index: &'a RoomIndex,
	pub(super) plans: &'a [RoomPlan],
	pub(super) terrain: &'a Terrain,
	pub(super) features: &'a HashMap<(i32, i32), Feature>,
	pub(super) stairs_down: (i32, i32),
	pub(super) stairs_up: Option<(i32, i32)>,
	/// Every cell the level can be furnished from
	pub(super) cells: &'a [Cell],
}

/// What building the tiles makes besides models.
pub(super) struct Built {
	pub(super) lights: Vec<Vec3>,
	pub(super) blockers: Vec<Blocker>,
	/// Tiles a hand-made cell fills, which chests, items and monsters keep off
	pub(super) hand_made: Vec<(i32, i32)>,
}

/// One walkable tile, and what about it decides how it's built.
struct Tile {
	x: i32,
	y: i32,
	plan: Option<RoomPlan>,
	tall: bool,
	ramp: Option<Ramp>,
	feature: Option<Feature>,
	/// How much higher than usual its ceiling has to reach
	extra: f32,
	/// The middle of its floor piece
	center: Vec3,
	/// The up staircase is here
	up_here: bool,
}

impl Layout<'_> {
	fn in_room(&self, x: i32, y: i32) -> bool {
		self.index.of(x, y).is_some()
	}

	fn plan_of(&self, x: i32, y: i32) -> Option<RoomPlan> {
		self.index.of(x, y).map(|i| self.plans[i])
	}
}

impl Tile {
	fn new(level: &Layout, x: i32, y: i32) -> Self {
		let plan = level.plan_of(x, y);
		let tall = plan.is_some_and(|p| p.tall);
		let ramp = level.terrain.ramp(x, y);
		let feature = level.features.get(&(x, y)).copied();
		// The level the tile's floor piece and walls stand on, and how
		// much higher than usual its ceiling has to reach
		let (base, extra) = match (ramp, feature) {
			(Some(r), _) => (r.low, r.high - r.low),
			(None, Some(Feature::Pit)) => (level.terrain.tile(x, y), LEDGE),
			(None, Some(Feature::Ledge)) => (level.terrain.tile(x, y) - LEDGE, 0.0),
			(None, None) => (level.terrain.tile(x, y), 0.0),
		};
		let center = tile_center(x, y) + Vec3::Y * base;
		let up_here = level.stairs_up == Some((x, y));
		Self { x, y, plan, tall, ramp, feature, extra, center, up_here }
	}
}

/// Builds and furnishes every walkable tile, then puts in the way up.
pub(super) fn build_tiles(
	rng: &mut impl Rng,
	b: &mut Builder,
	kit: &KitMeshes,
	level: &Layout,
) -> Result<Built, String> {
	let mut built = Built { lights: Vec::new(), blockers: Vec::new(), hand_made: Vec::new() };
	for x in 0..level.map.x as i32 {
		for y in 0..level.map.y as i32 {
			if level.map.is_wall(x, y) {
				continue;
			}
			build_tile(rng, b, &mut built, kit, level, &Tile::new(level, x, y))?;
		}
	}
	if let Some(up) = level.stairs_up {
		let at = tile_center(up.0, up.1) + Vec3::Y * level.terrain.tile(up.0, up.1);
		b.put(kit.stairs_up, at, facing_yaw(open_facing(level.map, (up.0, up.1, 0))));
	}
	Ok(built)
}

/// Builds one walkable tile, piece by piece (see the top of this file).
fn build_tile(
	rng: &mut impl Rng,
	b: &mut Builder,
	built: &mut Built,
	kit: &KitMeshes,
	level: &Layout,
	tile: &Tile,
) -> Result<(), String> {
	if place_hand_made(rng, b, &mut built.lights, level, tile)? {
		built.hand_made.push((tile.x, tile.y));
		return Ok(());
	}
	place_floor(b, kit, level, tile);
	place_ledge(b, kit, tile);
	for dir in 0..DIRECTIONS.len() {
		place_edge(rng, b, built, kit, level, tile, dir);
	}
	place_pillars(b, kit, level, tile);
	place_gallery_and_chandelier(b, &mut built.lights, kit, level, tile);
	furnish(rng, b, &mut built.lights, level, tile)
}

// ---------------------------------------------------------------------------
// The pieces of a tile, in the order they're built
// ---------------------------------------------------------------------------

/// A hand-made cell replaces the whole tile, where one fits. True if one did.
fn place_hand_made(
	rng: &mut impl Rng,
	b: &mut Builder,
	lights: &mut Vec<Vec3>,
	level: &Layout,
	tile: &Tile,
) -> Result<bool, String> {
	let (x, y) = (tile.x, tile.y);
	if (x, y) != level.stairs_down && !tile.up_here && tile.ramp.is_none() && tile.feature.is_none() && !tile.tall {
		if let Some((kind, turns)) = cells::fit(level.map, x, y, level.in_room(x, y)) {
			let made: Vec<&Cell> = level.cells.iter().filter(|c| c.structure && c.kind == kind).collect();
			if !made.is_empty() {
				let cell = made[rng.random_range(0..made.len())];
				place_cell(b, lights, cell, tile.center, turns)?;
				return Ok(true);
			}
		}
	}
	Ok(false)
}

/// The floor: the way down, a stair tile, a hall piece, or the plain floor.
fn place_floor(b: &mut Builder, kit: &KitMeshes, level: &Layout, tile: &Tile) {
	let center = tile.center;
	if (tile.x, tile.y) == level.stairs_down {
		// No floor: a flight of steps going down into the dark
		let facing = open_facing(level.map, (tile.x, tile.y, 0));
		b.put(kit.stairs_down, center - Vec3::Y * STAIRS_DOWN_DEPTH, facing_yaw(facing));
	} else if let Some(r) = tile.ramp {
		// Steps up to the high side; the floor piece's ceiling stretched to the higher one
		b.put(kit.ramp_steps, center, facing_yaw((r.dir + 3) % 4));
		b.put_stretched(kit.floor, center, 0.0, STRETCH_UP + tile.extra / 4.36);
	} else if tile.tall {
		let (piece, turns) = match cells::fit(level.map, tile.x, tile.y, true) {
			Some((CellKind::RoomEdge, t)) => (kit.hall[1], t),
			Some((CellKind::RoomCorner, t)) => (kit.hall[2], t),
			_ => (kit.hall[0], 0),
		};
		b.put_scaled(piece, center, facing_yaw(turns), Vec3::new(STRETCH, 1.0, STRETCH));
	} else {
		b.put_stretched(kit.floor, center, 0.0, STRETCH_UP + tile.extra / 4.36);
	}
}

fn place_ledge(b: &mut Builder, kit: &KitMeshes, tile: &Tile) {
	if tile.feature == Some(Feature::Ledge) {
		// A stone block the size of the tile, LEDGE high
		b.put_scaled(kit.block, tile.center, 0.0, Vec3::new(STRETCH, LEDGE / 4.0, STRETCH));
	}
}

/// What stands on the tile's edge toward DIRECTIONS[dir]: a wall panel
/// (maybe with a torch) against rock, an arch where a corridor opens into a
/// room, or a stone face down into a pit.
fn place_edge(
	rng: &mut impl Rng,
	b: &mut Builder,
	built: &mut Built,
	kit: &KitMeshes,
	level: &Layout,
	tile: &Tile,
	dir: usize,
) {
	let (x, y) = (tile.x, tile.y);
	let (dx, dy) = DIRECTIONS[dir];
	let (nx, ny) = (x + dx, y + dy);
	let out = Vec3::new(dx as f32, 0.0, dy as f32);
	if level.map.is_wall(nx, ny) {
		place_wall(rng, b, kit, level, tile, dir);
		if built.lights.len() < MAX_TORCHES && !tile.tall && tile.ramp.is_none() && rng.random_bool(TORCH_CHANCE) {
			place_torch(b, built, kit, level, tile, out);
		}
	} else if dir < 2 && level.in_room(x, y) != level.in_room(nx, ny) {
		place_arch(b, &mut built.blockers, kit, level, tile, dir);
	} else if dir < 2 && !level.map.is_wall(nx, ny) {
		place_pit_face(b, kit, level, tile, dir);
	}
}

fn place_wall(rng: &mut impl Rng, b: &mut Builder, kit: &KitMeshes, level: &Layout, tile: &Tile, dir: usize) {
	// Mostly the plain panel, sometimes a variant; in tall halls the
	// hall pieces are the walls, except where they don't fit
	let fits_hall = tile.tall && cells::fit(level.map, tile.x, tile.y, true).is_some();
	if !fits_hall {
		let walls = &kit.walls;
		let wall = if rng.random_bool(0.7) { walls[0] } else { walls[rng.random_range(0..walls.len())] };
		let up = STRETCH_UP + tile.extra / WALL_HEIGHT;
		b.put_stretched(wall, tile.center, facing_yaw(dir), up);
		if tile.tall {
			b.put_stretched(wall, tile.center + Vec3::Y * WALL_HEIGHT, facing_yaw(dir), STRETCH_UP);
		}
	}
}

fn place_torch(b: &mut Builder, built: &mut Built, kit: &KitMeshes, level: &Layout, tile: &Tile, out: Vec3) {
	// The torch model stands at its own height with its back at
	// x = 0, sticking out along +X; turn +X to point into the tile
	// and set its back on the wall's face
	let (x, y) = (tile.x, tile.y);
	let floor_y = level.terrain.tile(x, y);
	let face = tile_center(x, y) + Vec3::Y * floor_y + out * (TILE / 2.0 * STRETCH - WALL_THICKNESS);
	let o = b.put(kit.torch, face, (out.z).atan2(-out.x));
	b.add_prop(o, None, None, false, true);
	built.lights.push(face - out * 0.5 + Vec3::Y * 2.6);
	built.blockers.push(Blocker::on_edge(
		tile_center(x, y),
		out,
		(-0.2, 0.2),
		(TILE / 2.0 - WALL_THICKNESS - TORCH_DEPTH, TILE / 2.0),
	));
}

fn place_arch(b: &mut Builder, blockers: &mut Vec<Blocker>, kit: &KitMeshes, level: &Layout, tile: &Tile, dir: usize) {
	// A corridor opens into a room: an arch on the shared edge, at
	// the room's floor. Only east and north edges, so each is built once.
	let (x, y) = (tile.x, tile.y);
	let (dx, dy) = DIRECTIONS[dir];
	let (nx, ny) = (x + dx, y + dy);
	let out = Vec3::new(dx as f32, 0.0, dy as f32);
	let (rx, ry) = if level.in_room(x, y) { (x, y) } else { (nx, ny) };
	let arch_y = level.terrain.tile(rx, ry);
	let edge = tile_center(x, y) + out * (TILE / 2.0);
	b.put_stretched(kit.passage, edge.with_y(arch_y), facing_yaw(dir), STRETCH_UP);
	// Close the gap above the arch when either side's ceiling is higher
	let high_side = level.plan_of(rx, ry).is_some_and(|p| p.tall)
		|| level.terrain.ramp(x, y).is_some()
		|| level.terrain.ramp(nx, ny).is_some();
	if high_side {
		let panel = kit.walls[0];
		// On the room tile's edge toward the corridor
		let toward_corridor = if (rx, ry) == (x, y) { dir } else { (dir + 2) % 4 };
		for k in 1..=2 {
			let height = arch_y + WALL_HEIGHT * k as f32;
			let at = (tile_center(rx, ry) + Vec3::Y * height).with_y(height);
			b.put_stretched(panel, at, facing_yaw(toward_corridor), STRETCH_UP);
		}
	}
	let half = TILE / 2.0;
	for span in [(-half, -ARCH_OPENING), (ARCH_OPENING, half)] {
		blockers.push(Blocker::on_edge(tile_center(x, y), out, span, (half - 0.35, half + 0.35)));
	}
}

fn place_pit_face(b: &mut Builder, kit: &KitMeshes, level: &Layout, tile: &Tile, dir: usize) {
	// A drop inside a room (a pit's edge): a stone face from the low
	// floor up to the high one. Ledges are blocks, which have their own.
	let (x, y) = (tile.x, tile.y);
	let (dx, dy) = DIRECTIONS[dir];
	let (nx, ny) = (x + dx, y + dy);
	let out = Vec3::new(dx as f32, 0.0, dy as f32);
	let (here, there) = (level.terrain.tile(x, y), level.terrain.tile(nx, ny));
	let low_is_pit = if here < there {
		tile.feature == Some(Feature::Pit)
	} else {
		level.features.get(&(nx, ny)) == Some(&Feature::Pit)
	};
	if (here - there).abs() > 0.01 && low_is_pit && tile.ramp.is_none() && level.terrain.ramp(nx, ny).is_none() {
		let edge = tile_center(x, y) + out * (TILE / 2.0);
		let (thin_x, thin_z) = (if dx != 0 { 0.06 } else { STRETCH }, if dy != 0 { 0.06 } else { STRETCH });
		let size = Vec3::new(thin_x, (here - there).abs() / 4.0, thin_z);
		b.put_scaled(kit.block, edge.with_y(here.min(there)), 0.0, size);
	}
}

fn place_pillars(b: &mut Builder, kit: &KitMeshes, level: &Layout, tile: &Tile) {
	// Pillars cover the corners where wall panels meet. Inside a
	// room's corner both sides are wall. Where a corridor turns or
	// branches, both sides are open but the diagonal is rock, and
	// two panels' ends meet at the tile's corner point; only the
	// tile diagonally across from the rock sees it that way, so
	// each such corner gets one pillar
	let (x, y) = (tile.x, tile.y);
	let map = level.map;
	let stack = if tile.tall { 2 } else { 1 };
	for (a, c) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
		let (ax, ay) = DIRECTIONS[a];
		let (cx, cy) = DIRECTIONS[c];
		let (wall_a, wall_c) = (map.is_wall(x + ax, y + ay), map.is_wall(x + cx, y + cy));
		let offset = Vec3::new((ax + cx) as f32, 0.0, (ay + cy) as f32);
		let at = if wall_a && wall_c {
			Some(offset * (TILE / 2.0 - 0.2))
		} else if !wall_a && !wall_c && map.is_wall(x + ax + cx, y + ay + cy) {
			Some(offset * (TILE / 2.0))
		} else {
			None
		};
		if let Some(at) = at {
			for k in 0..stack {
				let lift = Vec3::Y * (WALL_HEIGHT * k as f32);
				let scale = Vec3::new(1.0, 1.0 + tile.extra / WALL_HEIGHT, 1.0);
				b.put_scaled(kit.pillar, tile.center + at + lift, 0.0, scale);
			}
		}
	}
}

fn place_gallery_and_chandelier(b: &mut Builder, lights: &mut Vec<Vec3>, kit: &KitMeshes, level: &Layout, tile: &Tile) {
	// Tall halls: a gallery along the walls, and a chandelier in the middle
	let Some(p) = tile.plan.filter(|p| p.tall) else { return };
	let (x, y) = (tile.x, tile.y);
	let (map, center) = (level.map, tile.center);
	let fit = cells::fit(map, x, y, true);
	if p.gallery && matches!(fit, Some((CellKind::RoomEdge | CellKind::RoomCorner, _))) {
		b.put(kit.balcony, center + Vec3::Y * GALLERY, 0.0);
		// Railings on its open sides that look over the hall's floor
		for &(dx, dy) in &DIRECTIONS {
			let (nx, ny) = (x + dx, y + dy);
			let over_floor = !map.is_wall(nx, ny)
				&& level.index.of(nx, ny) == level.index.of(x, y)
				&& matches!(cells::fit(map, nx, ny, true), Some((CellKind::RoomCenter, _)));
			if over_floor {
				let yaw = if dy != 0 { std::f32::consts::FRAC_PI_2 } else { 0.0 };
				let side = Vec3::new(dx as f32, 0.0, dy as f32) * 1.9;
				b.put(kit.railing, center + side + Vec3::Y * GALLERY, yaw);
			}
		}
	}
	let middle = level.rooms[level.index.of(x, y).unwrap()].center();
	if (x, y) == middle {
		b.put(kit.chandelier, center + Vec3::Y * (HALL_CEILING - 4.71), 0.0);
		lights.push(center + Vec3::Y * (HALL_CEILING - 3.5));
	}
}

fn furnish(
	rng: &mut impl Rng,
	b: &mut Builder,
	lights: &mut Vec<Vec3>,
	level: &Layout,
	tile: &Tile,
) -> Result<(), String> {
	// Furnishings from a generated cell, on top of the kit
	let (x, y) = (tile.x, tile.y);
	let room_theme = tile.plan.and_then(|p| p.theme);
	let furnish =
		(x, y) != level.stairs_down && !tile.up_here && tile.ramp.is_none() && tile.feature != Some(Feature::Pit);
	if furnish && (room_theme.is_some() || !level.in_room(x, y)) {
		if let Some((kind, turns)) = cells::fit(level.map, x, y, level.in_room(x, y)) {
			// Tall halls' walls are thick hall pieces: only their middles get furnished
			let ok_kind = !tile.tall || kind == CellKind::RoomCenter;
			let chance = tile.plan.map_or(HALL_CELL_CHANCE, |p| p.density);
			let pool: Vec<&Cell> =
				level.cells.iter().filter(|c| !c.structure && c.kind == kind && c.theme == room_theme).collect();
			if ok_kind && !pool.is_empty() && rng.random_bool(chance) {
				let cell = pool[rng.random_range(0..pool.len())];
				let at = tile_center(x, y) + Vec3::Y * level.terrain.tile(x, y);
				place_cell(b, lights, cell, at, turns)?;
			}
		}
	}
	Ok(())
}
