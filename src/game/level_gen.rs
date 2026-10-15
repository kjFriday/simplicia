// Level generation, in stages:
//   1. Lay out rooms and corridors (dungeons::mapgen).
//   2. Shape them: some rooms are raised or sunk (with a stair tile joining
//      them to their corridors), some are tall halls with galleries, and
//      some get ledges or pits.
//   3. Build every tile from the modular dungeon kit (models/Tiles).
//   4. Furnish it with cells: hand-made ones from cells/, then the ones
//      generated at startup from the asset catalog, matched to each room's
//      theme.
//   5. Check for clipping: anything that ends up inside something else or
//      in a wall is taken out.
//   6. Add the stairs, treasure, items, and monsters.
use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3};
use rand::{Rng, RngExt};

use super::catalog::{bounds, RoomTheme};
use super::cells::{self, Cell, CellKind};
use super::chests::Chest;
use super::combat::{Enemy, EnemyKind};
use super::items::{GroundItem, Hero, ItemKind};
use super::player::Player;
use super::presets::GenSettings;
use super::terrain::{Ramp, Terrain};
use super::world::*;
use crate::dungeons::mapgen::{generate_dungeon_with_options, MapGrid, Room};
use crate::engine::mesh::Mesh;
use crate::engine::scene::SceneObject;

// The kit's pieces are 4 x 4 m, centered on their origin with the floor at
// y = 0. Wall panels stand on a tile's north (-Z) edge and are turned to face
// the other sides.

/// A look for a level: which kit pieces build it, and how they're tinted
/// (the kit's shared texture isn't bundled, so color comes from tints).
pub(super) struct Theme {
	/// Floor and ceiling for one walkable tile
	pub(super) floor: &'static str,
	/// Wall panels; the first is the most common
	pub(super) walls: &'static [&'static str],
	/// An arch where a corridor opens into a room
	pub(super) passage: &'static str,
	pub(super) stone: Vec3,
}

pub(super) const DUNGEON: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Dungeon_Wall_Var1.glb", "Tiles/Dungeon_Wall_Var2.glb", "Tiles/Dungeon_Wall_Var3.glb"],
	passage: "Tiles/Dungeon_Passage.glb",
	stone: Vec3::new(0.62, 0.6, 0.58),
};

pub(super) const BASEMENT: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Basement_Wall_Var1.glb", "Tiles/Basement_Wall_Var2.glb", "Tiles/Basement_Wall_Var3.glb"],
	passage: "Tiles/Basement_Passage.glb",
	stone: Vec3::new(0.55, 0.5, 0.44),
};

pub(super) const PILLAR: &str = "Tiles/Pillar.glb";
pub(super) const TORCH: &str = "Lights/Torch_Wall.glb";
/// The way down sinks a full flight below the floor; the way up stands on it.
pub(super) const STAIRS_DOWN: &str = "Tiles/Stairs_High.glb";
pub(super) const STAIRS_UP: &str = "Tiles/Stairs_Low.glb";
/// Sunk this far, the top step of the way down is level with the floor.
pub(super) const STAIRS_DOWN_DEPTH: f32 = 5.0;
/// One flight of steps, 2.5 m high, for the stair tile into a raised or sunken room
const RAMP_STEPS: &str = "Tiles/Stairs_Low.glb";

/// Tall halls: a 10 m floor-to-ceiling tile, the same with a wall on its
/// north side, and with walls north and east.
const HALL_FLOOR: &str = "Tiles/Dungeon_Hall_Center.glb";
const HALL_WALL: &str = "Tiles/Dungeon_Hall_Wall.glb";
const HALL_CORNER: &str = "Tiles/Dungeon_Hall_Corner.glb";
const BALCONY: &str = "Tiles/Balcony.glb";
const RAILING: &str = "Tiles/Balcony_Railing.glb";
const CHANDELIER: &str = "Lights/Chandelier_Ceiling.glb";
/// A 4 m cube, squashed into ledges and pit walls
const BLOCK: &str = "terrain/canyon/SM_AP_Cube_4m_0.glb";

/// Floors, walls and arches are stretched this much across the floor, so
/// neighboring pieces overlap and close the seams, and this much upward.
pub(super) const STRETCH: f32 = 1.04;
pub(super) const STRETCH_UP: f32 = 1.1;
/// The kit's ordinary floor-to-ceiling and wall heights, once stretched.
const CEILING: f32 = 4.36 * STRETCH_UP;
const WALL_HEIGHT: f32 = 5.0;
/// Tall halls: where the galleries run, and the ceiling.
const GALLERY: f32 = 5.0;
const HALL_CEILING: f32 = 9.4;

/// How far raised and sunken rooms sit from their corridors, and how high
/// ledges stand (pits sink the same).
const ROOM_RISE: f32 = 2.5;
const LEDGE: f32 = 1.2;
/// Chances, per room, of each shape (the rest stay level).
const TALL_CHANCE: f64 = 0.2;
const RAISED_CHANCE: f64 = 0.15;
const SUNKEN_CHANCE: f64 = 0.15;
const LEDGE_CHANCE: f64 = 0.35;
/// Share of a room's tiles that get a generated cell's furnishings, picked
/// per room between these; and the chance for a hall tile.
const ROOM_DENSITY: (f64, f64) = (0.35, 0.85);
const HALL_CELL_CHANCE: f64 = 0.4;

/// Chance that a wall panel facing a room or corridor gets a torch.
pub(super) const TORCH_CHANCE: f64 = 0.08;
/// Never more lights than this, so they stay cheap.
pub(super) const MAX_TORCHES: usize = 40;

pub(super) const WOOD: Vec3 = Vec3::new(0.55, 0.38, 0.22);
/// Chests: closed and open, the chance one turns up in a room (the last room,
/// by the way down, always has one).
pub(super) const CHEST: (&str, Vec3) = ("Furniture/Chest_Closed.glb", WOOD);
const CHEST_OPEN: &str = "Furniture/Chest_Open.glb";
const CHEST_CHANCE: f64 = 0.25;

/// Loads a kit model at its real size and tints it.
pub(super) fn load_kit_model(name: &str, tint: Vec3) -> Result<Mesh, String> {
	let mut mesh = Mesh::load(name)?;
	for v in &mut mesh.vertices {
		v.color *= tint;
	}
	Ok(mesh)
}

/// Turn about Y that points a north-facing kit piece toward DIRECTIONS[dir].
pub(super) fn facing_yaw(dir: usize) -> f32 {
	// North (-Z) needs none; each step clockwise (east, south, west) is a
	// quarter turn to the right, which is negative about +Y
	-(dir as f32) * std::f32::consts::FRAC_PI_2
}

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

// ---------------------------------------------------------------------------
// Placing models
// ---------------------------------------------------------------------------

/// Something placed to furnish the level, kept track of for the clipping check.
struct Prop {
	/// Index into `Builder::objects`
	object: usize,
	/// Its box in the world
	min: Vec3,
	max: Vec3,
	/// The prop it stands on, which it may touch
	parent: Option<usize>,
	/// Its light, as an index into the level's lights
	light: Option<usize>,
	/// Lies flat (a carpet): others may stand on it
	flat: bool,
	/// Part of the building (a kit torch): never removed, only avoided
	fixed: bool,
}

/// Loads models once and places copies of them.
pub(super) struct Builder {
	pub(super) meshes: Vec<Mesh>,
	/// (model, tint, normalized) -> index into meshes
	loaded: Vec<((String, [u32; 3], bool), usize)>,
	pub(super) objects: Vec<SceneObject>,
	props: Vec<Prop>,
}

impl Builder {
	fn new() -> Self {
		Self { meshes: Vec::new(), loaded: Vec::new(), objects: Vec::new(), props: Vec::new() }
	}

	pub(super) fn model(&mut self, name: &str, tint: Vec3) -> Result<usize, String> {
		self.load(name, tint, false)
	}

	/// Like `model`, but moved so its footprint is centered on its origin and
	/// its lowest point is at y = 0.
	fn model_normalized(&mut self, name: &str, tint: Vec3) -> Result<usize, String> {
		self.load(name, tint, true)
	}

	fn load(&mut self, name: &str, tint: Vec3, normalize: bool) -> Result<usize, String> {
		let key = (name.to_string(), tint.to_array().map(f32::to_bits), normalize);
		if let Some(&(_, i)) = self.loaded.iter().find(|(k, _)| *k == key) {
			return Ok(i);
		}
		let mut mesh = load_kit_model(name, tint)?;
		if normalize {
			if let Some((lo, hi)) = bounds(&mesh) {
				let shift = Vec3::new(-(lo.x + hi.x) / 2.0, -lo.y, -(lo.z + hi.z) / 2.0);
				for v in &mut mesh.vertices {
					v.pos += shift;
				}
			}
		}
		self.meshes.push(mesh);
		self.loaded.push((key, self.meshes.len() - 1));
		Ok(self.meshes.len() - 1)
	}

	/// Places a model at `at` (world space), turned `yaw` about Y.
	pub(super) fn put(&mut self, mesh: usize, at: Vec3, yaw: f32) -> usize {
		self.put_scaled(mesh, at, yaw, Vec3::ONE)
	}

	/// Places a model scaled by `scale` (in its own axes) after turning it.
	fn put_scaled(&mut self, mesh: usize, at: Vec3, yaw: f32, scale: Vec3) -> usize {
		let transform = Mat4::from_translation(at) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(scale);
		self.objects.push(SceneObject { mesh, transform });
		self.objects.len() - 1
	}

	/// Like `put`, but stretched a little across the floor (and `up` times
	/// upward) so the piece overlaps its neighbors and no seams show.
	fn put_stretched(&mut self, mesh: usize, at: Vec3, yaw: f32, up: f32) -> usize {
		self.put_scaled(mesh, at, yaw, Vec3::new(STRETCH, up, STRETCH))
	}

	/// How far below its origin a model reaches. Hand-placed models are
	/// lifted by this, so they stand on the floor instead of sinking into it.
	pub(super) fn sink(&self, mesh: usize) -> f32 {
		bounds(&self.meshes[mesh]).map_or(0.0, |(lo, _)| (-lo.y).max(0.0))
	}

	/// The box in the world that `mesh` fills when placed by `transform`.
	fn box_of(&self, mesh: usize, transform: Mat4) -> (Vec3, Vec3) {
		let (lo, hi) = bounds(&self.meshes[mesh]).unwrap_or((Vec3::ZERO, Vec3::ZERO));
		let corners = (0..8).map(|i| {
			let c = Vec3::new(
				if i & 1 == 0 { lo.x } else { hi.x },
				if i & 2 == 0 { lo.y } else { hi.y },
				if i & 4 == 0 { lo.z } else { hi.z },
			);
			transform.transform_point3(c)
		});
		corners.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(a, b), p| (a.min(p), b.max(p)))
	}

	/// Records object `object` as a prop, with its box in the world.
	fn add_prop(&mut self, object: usize, parent: Option<usize>, light: Option<usize>, flat: bool, fixed: bool) -> usize {
		let o = &self.objects[object];
		let (min, max) = self.box_of(o.mesh, o.transform);
		self.props.push(Prop { object, min, max, parent, light, flat, fixed });
		self.props.len() - 1
	}

	/// Something drawn elsewhere (a chest) that props must stay out of.
	fn add_fixed_box(&mut self, min: Vec3, max: Vec3) {
		self.props.push(Prop { object: usize::MAX, min, max, parent: None, light: None, flat: false, fixed: true });
	}

	/// The clipping check: takes out every prop that pokes into a wall or
	/// overlaps another prop (other than what it stands on), along with
	/// anything standing on it and its light. Earlier props win, and fixed
	/// ones (kit torches, chests) always do. Returns how many were taken out.
	fn remove_clipping(&mut self, map: &MapGrid, lights: &mut Vec<Vec3>) -> usize {
		const SLACK: f32 = 0.03;
		let mut removed = vec![false; self.props.len()];
		for i in 0..self.props.len() {
			let p = &self.props[i];
			if p.fixed {
				continue;
			}
			let parent_gone = p.parent.is_some_and(|q| removed[q]);
			// Every corner of its footprint must be over floor, not rock
			let (lo, hi) = (p.min + Vec3::splat(SLACK), p.max - Vec3::splat(SLACK));
			let in_wall = [(lo.x, lo.z), (hi.x, lo.z), (lo.x, hi.z), (hi.x, hi.z)]
				.iter()
				.any(|&(x, z)| map.is_wall((x / TILE).floor() as i32, (z / TILE).floor() as i32));
			let related = |a: usize, b: usize| {
				let up = |mut k: usize, target: usize| {
					while let Some(next) = self.props[k].parent {
						if next == target {
							return true;
						}
						k = next;
					}
					false
				};
				up(a, b) || up(b, a)
			};
			// Fixed props count wherever they come: chests are added last
			let overlaps = !p.flat
				&& (0..self.props.len()).any(|j| {
					let q = &self.props[j];
					((j < i && !removed[j]) || q.fixed)
						&& !q.flat && !related(i, j)
						&& lo.x < q.max.x && hi.x > q.min.x
						&& lo.y < q.max.y && hi.y > q.min.y
						&& lo.z < q.max.z && hi.z > q.min.z
				});
			removed[i] = parent_gone || in_wall || overlaps;
		}

		let gone_objects: Vec<usize> = (0..self.props.len()).filter(|&i| removed[i]).map(|i| self.props[i].object).collect();
		let gone_lights: Vec<usize> = (0..self.props.len()).filter(|&i| removed[i]).filter_map(|i| self.props[i].light).collect();
		let mut index = 0;
		self.objects.retain(|_| {
			index += 1;
			!gone_objects.contains(&(index - 1))
		});
		let mut index = 0;
		lights.retain(|_| {
			index += 1;
			!gone_lights.contains(&(index - 1))
		});
		self.props.clear();
		gone_objects.len()
	}
}

// ---------------------------------------------------------------------------
// The level's shape
// ---------------------------------------------------------------------------

/// How a room is shaped and furnished.
#[derive(Clone, Copy, Debug)]
struct RoomPlan {
	/// Floor height relative to the corridors
	height: f32,
	tall: bool,
	/// Tall halls: a gallery runs around the walls
	gallery: bool,
	/// What it's furnished as; none for the room the player starts in
	theme: Option<RoomTheme>,
	/// Share of its tiles that get furnished: some rooms are cluttered,
	/// some sparse
	density: f64,
}

/// Raised or sunk parts of a room's floor.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Feature {
	Ledge,
	Pit,
}

/// Which room each tile belongs to.
struct RoomIndex {
	width: usize,
	rooms: Vec<Option<usize>>,
}

impl RoomIndex {
	fn new(map: &MapGrid, rooms: &[Room]) -> Self {
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

	fn of(&self, x: i32, y: i32) -> Option<usize> {
		if x < 0 || y < 0 || x as usize >= self.width {
			return None;
		}
		self.rooms.get(y as usize * self.width + x as usize).copied().flatten()
	}
}

/// Decides each room's shape, then sets the floor heights: room floors, the
/// stair tiles into raised and sunken rooms, and ledges and pits. A raised
/// or sunken room whose entrances can't all take a stair tile stays level.
fn shape_rooms(
	rng: &mut impl Rng,
	map: &MapGrid,
	rooms: &[Room],
	index: &RoomIndex,
	keep_flat: &[(i32, i32)],
) -> (Vec<RoomPlan>, Terrain, HashMap<(i32, i32), Feature>) {
	let mut terrain = Terrain::flat(map.x, map.y);
	let mut features = HashMap::new();
	let mut plans: Vec<RoomPlan> = rooms
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
		.collect();

	// Stair tiles: each corridor tile next to a raised or sunken room becomes
	// one, if it's a clean entrance (one way in, corridor straight behind it)
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
				let into: Vec<usize> = (0..4).filter(|&d| index.of(x + DIRECTIONS[d].0, y + DIRECTIONS[d].1) == Some(i)).collect();
				if into.is_empty() {
					continue;
				}
				let d = into[0];
				let (bx, by) = (x - DIRECTIONS[d].0, y - DIRECTIONS[d].1);
				let other_room = (0..4).any(|k| index.of(x + DIRECTIONS[k].0, y + DIRECTIONS[k].1).is_some_and(|o| o != i));
				if into.len() != 1 || other_room || map.is_wall(bx, by) || index.of(bx, by).is_some() || keep_flat.contains(&(x, y)) {
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
	(plans, terrain, features)
}

// ---------------------------------------------------------------------------
// Building the level
// ---------------------------------------------------------------------------

impl World {
	/// Builds a level with the default settings and the dungeon kit only.
	pub(super) fn generate(rng: &mut impl Rng, depth: usize) -> Result<Self, String> {
		Self::generate_with(rng, depth, &GenSettings::default(), &[])
	}

	/// Builds a level from `settings`, furnished with `cells` (see the stages
	/// at the top of this file). The player starts in the first room, by the
	/// up staircase when `depth` > 0; the down staircase is in the last room.
	pub(super) fn generate_with(rng: &mut impl Rng, depth: usize, settings: &GenSettings, cells: &[Cell]) -> Result<Self, String> {
		let s = settings;
		let (map, rooms) = generate_dungeon_with_options(
			rng,
			s.width,
			s.height,
			s.splits,
			s.max_rooms,
			s.room_min,
			s.room_max,
			s.diagonal as f64 / 100.0,
		);
		if rooms.len() < 2 {
			return Err("the generator made fewer than 2 rooms, so there's nowhere for the stairs".into());
		}
		let start = rooms[0].center();
		let theme = if depth % 2 == 0 { &DUNGEON } else { &BASEMENT };
		let index = RoomIndex::new(&map, &rooms);
		let in_room = |x: i32, y: i32| index.of(x, y).is_some();

		// The way down is in the last room, kept off the room's center so the
		// corridor in doesn't land you on it by accident
		let last = rooms.len() - 1;
		let stairs_down = free_tile(rng, &rooms[last], &[rooms[last].center()]).unwrap_or(rooms[last].center());
		let stairs_up = (depth > 0).then_some(start);
		let (plans, terrain, features) = shape_rooms(rng, &map, &rooms, &index, &[start, stairs_down]);
		let plan_of = |x: i32, y: i32| index.of(x, y).map(|i| plans[i]);

		let mut b = Builder::new();
		let floor = b.model(theme.floor, theme.stone)?;
		let walls = theme.walls.iter().map(|&w| b.model(w, theme.stone)).collect::<Result<Vec<_>, _>>()?;
		let passage = b.model(theme.passage, theme.stone)?;
		let pillar = b.model(PILLAR, theme.stone)?;
		let torch = b.model(TORCH, WOOD)?;
		let stairs_down_model = b.model(STAIRS_DOWN, theme.stone)?;
		let stairs_up_model = b.model(STAIRS_UP, theme.stone)?;
		let ramp_steps = b.model(RAMP_STEPS, theme.stone)?;
		let hall = [b.model(HALL_FLOOR, theme.stone)?, b.model(HALL_WALL, theme.stone)?, b.model(HALL_CORNER, theme.stone)?];
		let balcony = b.model(BALCONY, WOOD)?;
		let railing = b.model(RAILING, WOOD)?;
		let chandelier = b.model(CHANDELIER, Vec3::new(0.45, 0.45, 0.5))?;
		let block = b.model_normalized(BLOCK, theme.stone)?;

		let mut lights = Vec::new();
		let mut blockers = Vec::new();
		for x in 0..map.x as i32 {
			for y in 0..map.y as i32 {
				if map.is_wall(x, y) {
					continue;
				}
				let plan = plan_of(x, y);
				let tall = plan.is_some_and(|p| p.tall);
				let ramp = terrain.ramp(x, y);
				let feature = features.get(&(x, y)).copied();
				// The level the tile's floor piece and walls stand on, and how
				// much higher than usual its ceiling has to reach
				let (base, extra) = match (ramp, feature) {
					(Some(r), _) => (r.low, r.high - r.low),
					(None, Some(Feature::Pit)) => (terrain.tile(x, y), LEDGE),
					(None, Some(Feature::Ledge)) => (terrain.tile(x, y) - LEDGE, 0.0),
					(None, None) => (terrain.tile(x, y), 0.0),
				};
				let center = tile_center(x, y) + Vec3::Y * base;
				let up_here = stairs_up == Some((x, y));

				// A hand-made cell replaces the whole tile, where one fits
				if (x, y) != stairs_down && !up_here && ramp.is_none() && feature.is_none() && !tall {
					if let Some((kind, turns)) = cells::fit(&map, x, y, in_room(x, y)) {
						let made: Vec<&Cell> = cells.iter().filter(|c| c.structure && c.kind == kind).collect();
						if !made.is_empty() {
							let cell = made[rng.random_range(0..made.len())];
							place_cell(&mut b, &mut lights, cell, center, turns)?;
							continue;
						}
					}
				}

				// The floor
				if (x, y) == stairs_down {
					// No floor: a flight of steps going down into the dark
					b.put(stairs_down_model, center - Vec3::Y * STAIRS_DOWN_DEPTH, facing_yaw(open_facing(&map, (x, y, 0))));
				} else if let Some(r) = ramp {
					// Steps up to the high side; the floor piece's ceiling stretched to the higher one
					b.put(ramp_steps, center, facing_yaw((r.dir + 3) % 4));
					b.put_stretched(floor, center, 0.0, STRETCH_UP + extra / 4.36);
				} else if tall {
					let (piece, turns) = match cells::fit(&map, x, y, true) {
						Some((CellKind::RoomEdge, t)) => (hall[1], t),
						Some((CellKind::RoomCorner, t)) => (hall[2], t),
						_ => (hall[0], 0),
					};
					b.put_scaled(piece, center, facing_yaw(turns), Vec3::new(STRETCH, 1.0, STRETCH));
				} else {
					b.put_stretched(floor, center, 0.0, STRETCH_UP + extra / 4.36);
				}
				if feature == Some(Feature::Ledge) {
					// A stone block the size of the tile, LEDGE high
					b.put_scaled(block, center, 0.0, Vec3::new(STRETCH, LEDGE / 4.0, STRETCH));
				}

				for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
					let (nx, ny) = (x + dx, y + dy);
					let out = Vec3::new(dx as f32, 0.0, dy as f32);
					if map.is_wall(nx, ny) {
						// Mostly the plain panel, sometimes a variant; in tall halls the
						// hall pieces are the walls, except where they don't fit
						let fits_hall = tall && cells::fit(&map, x, y, true).is_some();
						if !fits_hall {
							let wall = if rng.random_bool(0.7) { walls[0] } else { walls[rng.random_range(0..walls.len())] };
							let up = STRETCH_UP + extra / WALL_HEIGHT;
							b.put_stretched(wall, center, facing_yaw(dir), up);
							if tall {
								b.put_stretched(wall, center + Vec3::Y * WALL_HEIGHT, facing_yaw(dir), STRETCH_UP);
							}
						}
						if lights.len() < MAX_TORCHES && !tall && ramp.is_none() && rng.random_bool(TORCH_CHANCE) {
							// The torch model stands at its own height with its back at
							// x = 0, sticking out along +X; turn +X to point into the tile
							// and set its back on the wall's face
							let floor_y = terrain.tile(x, y);
							let face = tile_center(x, y) + Vec3::Y * floor_y + out * (TILE / 2.0 * STRETCH - WALL_THICKNESS);
							let o = b.put(torch, face, (out.z).atan2(-out.x));
							b.add_prop(o, None, None, false, true);
							lights.push(face - out * 0.5 + Vec3::Y * 2.6);
							blockers.push(Blocker::on_edge(
								tile_center(x, y),
								out,
								(-0.2, 0.2),
								(TILE / 2.0 - WALL_THICKNESS - TORCH_DEPTH, TILE / 2.0),
							));
						}
					} else if dir < 2 && in_room(x, y) != in_room(nx, ny) {
						// A corridor opens into a room: an arch on the shared edge, at
						// the room's floor. Only east and north edges, so each is built once.
						let (rx, ry) = if in_room(x, y) { (x, y) } else { (nx, ny) };
						let arch_y = terrain.tile(rx, ry);
						let edge = tile_center(x, y) + out * (TILE / 2.0);
						b.put_stretched(passage, edge.with_y(arch_y), facing_yaw(dir), STRETCH_UP);
						// Close the gap above the arch when either side's ceiling is higher
						let high_side = plan_of(rx, ry).is_some_and(|p| p.tall) || terrain.ramp(x, y).is_some() || terrain.ramp(nx, ny).is_some();
						if high_side {
							let panel = walls[0];
							for k in 1..=2 {
								b.put_stretched(panel, (tile_center(rx, ry) + Vec3::Y * (arch_y + WALL_HEIGHT * k as f32)).with_y(arch_y + WALL_HEIGHT * k as f32), facing_yaw(if (rx, ry) == (x, y) { dir } else { (dir + 2) % 4 }), STRETCH_UP);
							}
						}
						let half = TILE / 2.0;
						for span in [(-half, -ARCH_OPENING), (ARCH_OPENING, half)] {
							blockers.push(Blocker::on_edge(tile_center(x, y), out, span, (half - 0.35, half + 0.35)));
						}
					} else if dir < 2 && !map.is_wall(nx, ny) {
						// A drop inside a room (a pit's edge): a stone face from the low
						// floor up to the high one. Ledges are blocks, which have their own.
						let (here, there) = (terrain.tile(x, y), terrain.tile(nx, ny));
						let low_is_pit = if here < there { feature == Some(Feature::Pit) } else { features.get(&(nx, ny)) == Some(&Feature::Pit) };
						if (here - there).abs() > 0.01 && low_is_pit && ramp.is_none() && terrain.ramp(nx, ny).is_none() {
							let edge = tile_center(x, y) + out * (TILE / 2.0);
							let size = Vec3::new(if dx != 0 { 0.06 } else { STRETCH }, (here - there).abs() / 4.0, if dy != 0 { 0.06 } else { STRETCH });
							b.put_scaled(block, edge.with_y(here.min(there)), 0.0, size);
						}
					}
				}

				// Pillars cover the corners where wall panels meet. Inside a
				// room's corner both sides are wall. Where a corridor turns or
				// branches, both sides are open but the diagonal is rock, and
				// two panels' ends meet at the tile's corner point; only the
				// tile diagonally across from the rock sees it that way, so
				// each such corner gets one pillar
				let stack = if tall { 2 } else { 1 };
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
							b.put_scaled(pillar, center + at + Vec3::Y * (WALL_HEIGHT * k as f32), 0.0, Vec3::new(1.0, 1.0 + extra / WALL_HEIGHT, 1.0));
						}
					}
				}

				// Tall halls: a gallery along the walls, and a chandelier in the middle
				if let Some(p) = plan.filter(|p| p.tall) {
					let fit = cells::fit(&map, x, y, true);
					if p.gallery && matches!(fit, Some((CellKind::RoomEdge | CellKind::RoomCorner, _))) {
						b.put(balcony, center + Vec3::Y * GALLERY, 0.0);
						// Railings on its open sides that look over the hall's floor
						for (d, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
							let (nx, ny) = (x + dx, y + dy);
							let over_floor = !map.is_wall(nx, ny)
								&& index.of(nx, ny) == index.of(x, y)
								&& matches!(cells::fit(&map, nx, ny, true), Some((CellKind::RoomCenter, _)));
							if over_floor {
								let yaw = if dy != 0 { std::f32::consts::FRAC_PI_2 } else { 0.0 };
								let side = Vec3::new(dx as f32, 0.0, dy as f32) * 1.9;
								b.put(railing, center + side + Vec3::Y * GALLERY, yaw);
							}
						}
					}
					let middle = rooms[index.of(x, y).unwrap()].center();
					if (x, y) == middle {
						b.put(chandelier, center + Vec3::Y * (HALL_CEILING - 4.71), 0.0);
						lights.push(center + Vec3::Y * (HALL_CEILING - 3.5));
					}
				}

				// Furnishings from a generated cell, on top of the kit
				let room_theme = plan.and_then(|p| p.theme);
				let furnish = (x, y) != stairs_down && !up_here && ramp.is_none() && feature != Some(Feature::Pit);
				if furnish && (room_theme.is_some() || !in_room(x, y)) {
					if let Some((kind, turns)) = cells::fit(&map, x, y, in_room(x, y)) {
						// Tall halls' walls are thick hall pieces: only their middles get furnished
						let ok_kind = !tall || kind == CellKind::RoomCenter;
						let chance = plan.map_or(HALL_CELL_CHANCE, |p| p.density);
						let pool: Vec<&Cell> =
							cells.iter().filter(|c| !c.structure && c.kind == kind && c.theme == room_theme).collect();
						if ok_kind && !pool.is_empty() && rng.random_bool(chance) {
							let cell = pool[rng.random_range(0..pool.len())];
							let at = tile_center(x, y) + Vec3::Y * terrain.tile(x, y);
							place_cell(&mut b, &mut lights, cell, at, turns)?;
						}
					}
				}
			}
		}
		if let Some(up) = stairs_up {
			let at = tile_center(up.0, up.1) + Vec3::Y * terrain.tile(up.0, up.1);
			b.put(stairs_up_model, at, facing_yaw(open_facing(&map, (up.0, up.1, 0))));
		}

		// Chests: always one by the way down, sometimes one in other rooms.
		// They're drawn by the view (they open), but block walking, and the
		// clipping check keeps furniture out of them
		let mut taken = vec![start, stairs_down];
		let chest_meshes = [b.model(CHEST.0, CHEST.1)?, b.model(CHEST_OPEN, CHEST.1)?];
		let mut chests = Vec::new();
		for (i, room) in rooms.iter().enumerate().skip(1) {
			if i != last && !rng.random_bool(CHEST_CHANCE) {
				continue;
			}
			let Some(t) = free_tile(rng, room, &taken) else { continue };
			taken.push(t);
			let yaw = rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2;
			let at = tile_center(t.0, t.1) + Vec3::Y * terrain.tile(t.0, t.1);
			let (min, max) = b.box_of(chest_meshes[0], Mat4::from_translation(at) * Mat4::from_rotation_y(yaw));
			b.add_fixed_box(min, max);
			blockers.push(Blocker { min, max });
			chests.push(Chest { id: chests.len() as u32, x: t.0, y: t.1, yaw, open: false });
		}
		let removed = b.remove_clipping(&map, &mut lights);
		if crate::game::dev_tools() {
			eprintln!("level {}: {} rooms, {} things taken out for clipping", depth + 1, rooms.len(), removed);
		}

		// Items and monsters; deeper levels get more, and tougher, monsters
		let enemy_chance = (s.enemies as f64 / 100.0 + depth as f64 * 0.1).min(1.0);
		let mut enemies: Vec<Enemy> = Vec::new();
		let mut items: Vec<GroundItem> = Vec::new();
		for room in rooms.iter().skip(1) {
			if rng.random_bool(s.items as f64 / 100.0) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let kind = ItemKind::random(rng);
					items.push(GroundItem { id: items.len() as u32, x: t.0, y: t.1, kind, from: None });
				}
			}
			if rng.random_bool(enemy_chance) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let kind = EnemyKind::pick(rng, depth);
					enemies.push(Enemy::new(rng, enemies.len() as u32, kind, t.0, t.1));
				}
			}
		}

		// Tiles carry the level as z
		let z = depth as i32;
		let seen = vec![false; map.x * map.y];
		let mut world = Self {
			map,
			player: Player::new(start.0, start.1, z),
			meshes: b.meshes,
			objects: b.objects,
			lights,
			depth,
			stairs_down: (stairs_down.0, stairs_down.1, z),
			stairs_up: stairs_up.map(|(x, y)| (x, y, z)),
			seen,
			blockers,
			terrain,
			enemies,
			next_item_id: items.len() as u32,
			items,
			chests,
			chest_meshes,
			events: Vec::new(),
			hero: Hero::default(),
			combat: false,
			log: Vec::new(),
		};
		world.place_player((start.0, start.1, z));
		Ok(world)
	}
}

/// Places a cell's models on the tile whose floor's middle is `center`,
/// turned `turns` quarter turns clockwise, and records them for the
/// clipping check. Hand-made cells' models are lifted to stand on the floor.
fn place_cell(b: &mut Builder, lights: &mut Vec<Vec3>, cell: &Cell, center: Vec3, turns: usize) -> Result<(), String> {
	let yaw = facing_yaw(turns);
	let turn = Quat::from_rotation_y(yaw);
	let mut props = Vec::with_capacity(cell.objects.len());
	for o in &cell.objects {
		let mesh = b.model(&o.model, o.tint)?;
		let lift = if o.grounded { b.sink(mesh) } else { 0.0 };
		let at = center + turn * o.pos + Vec3::Y * lift;
		let object = b.put(mesh, at, yaw + o.yaw_degrees.to_radians());
		let light = (o.light && lights.len() < MAX_TORCHES).then(|| {
			lights.push(at + Vec3::Y * 0.8);
			lights.len() - 1
		});
		let parent = o.parent.and_then(|p| props.get(p).copied());
		let flat = o.model.contains("Carpet");
		props.push(b.add_prop(object, parent, light, flat, cell.structure));
	}
	Ok(())
}
