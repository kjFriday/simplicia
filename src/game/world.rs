// A level: the map, the player in it, and everything placed on it.
use glam::Vec3;

use super::chests::Chest;
use super::combat::Enemy;
use super::items::{GroundItem, Hero};
use super::player::{Action, Player};
use super::terrain::Terrain;
use crate::dungeons::mapgen::MapGrid;
use crate::engine::mesh::Mesh;
use crate::engine::scene::SceneObject;

/// World units per map tile. Matches the 4 m cube used for walls.
pub(super) const TILE: f32 = 4.0;
/// Eye height above the floor.
pub(super) const EYE_HEIGHT: f32 = 2.0;

/// Compass directions in map space; map y grows south, which is world +Z.
pub(super) const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

/// The generated dungeon: the map for movement rules, and what to draw.
pub(super) struct World {
	pub(super) map: MapGrid,
	pub(super) player: Player,
	pub(super) meshes: Vec<Mesh>,
	pub(super) objects: Vec<SceneObject>,
	/// Torch flames: where to put a point light
	pub(super) lights: Vec<Vec3>,
	/// 0 is the top level; each staircase down adds one
	pub(super) depth: usize,
	/// Stepping here takes you one level deeper
	pub(super) stairs_down: (i32, i32, i32),
	/// Stepping here takes you back up; none on the top level
	pub(super) stairs_up: Option<(i32, i32, i32)>,
	/// Tiles the minimap shows, indexed [x * height + y]
	pub(super) seen: Vec<bool>,
	/// Solid things inside walkable tiles that free walking can't pass
	pub(super) blockers: Vec<Blocker>,
	/// How high the floor is on every tile
	pub(super) terrain: Terrain,
	/// Living enemies
	pub(super) enemies: Vec<Enemy>,
	/// Items lying on the floor
	pub(super) items: Vec<GroundItem>,
	/// The id the next item gets (items out of chests come after the rest)
	pub(super) next_item_id: u32,
	pub(super) chests: Vec<Chest>,
	/// Index into `meshes` of a closed chest and an open one
	pub(super) chest_meshes: [usize; 2],
	/// What happened since the view last looked, for it to show: swings,
	/// hits, chests opening. The view empties it.
	pub(super) events: Vec<Event>,
	/// HP, gold, and carried items
	pub(super) hero: Hero,
	/// True while an enemy is close: movement is tile by tile, turn by turn
	pub(super) combat: bool,
	/// What happened lately, newest last
	pub(super) log: Vec<String>,
}

/// A box on the floor plan (world X and Z; Y is ignored) the player can't
/// walk into.
#[derive(Clone, Copy, Debug)]
pub(super) struct Blocker {
	pub(super) min: Vec3,
	pub(super) max: Vec3,
}

impl Blocker {
	/// A box against tile edge `out` (a unit step from `center` toward the
	/// edge): from `a0` to `a1` along the edge, measured from its middle, and
	/// from `d0` to `d1` out from the tile's center.
	pub(super) fn on_edge(center: Vec3, out: Vec3, (a0, a1): (f32, f32), (d0, d1): (f32, f32)) -> Self {
		let along = Vec3::new(-out.z, 0.0, out.x);
		let p = center + along * a0 + out * d0;
		let q = center + along * a1 + out * d1;
		Self { min: p.min(q), max: p.max(q) }
	}

	/// Whether a square reaching `r` from `pos` on each side overlaps the box.
	pub(super) fn touches(&self, pos: Vec3, r: f32) -> bool {
		pos.x + r > self.min.x && pos.x - r < self.max.x && pos.z + r > self.min.z && pos.z - r < self.max.z
	}
}

/// Half the width of the opening in an arched passage; the stone beyond it
/// on both sides blocks walking.
pub(super) const ARCH_OPENING: f32 = 0.85;
/// How far the wall panels' faces stand in from the tile edge.
pub(super) const WALL_THICKNESS: f32 = 0.2;
/// How far into the tile a torch sticks out from the wall.
pub(super) const TORCH_DEPTH: f32 = 0.3;

/// How far around the player the minimap uncovers, in tiles.
pub(super) const REVEAL_RADIUS: i32 = 3;

/// Something the view should show happening.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Event {
	/// The player swung their weapon
	Swing,
	/// An enemy was hit, at `at` (about chest height)
	Hit { id: u32, at: Vec3 },
	/// An enemy died there
	Kill { at: Vec3 },
	/// The player was hit
	Hurt,
	ChestOpened { at: Vec3 },
}

/// Where a move left the player.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Arrival {
	Floor,
	StairsDown,
	StairsUp,
}

/// Moving around the map, and what the player has seen.
impl World {
	/// Whether the map shows tile (x, y). With `reveal_all` (fog off, in the
	/// debug binary) it shows everything.
	pub(super) fn has_seen(&self, x: i32, y: i32, reveal_all: bool) -> bool {
		let on_map = x >= 0 && y >= 0 && (x as usize) < self.map.x && (y as usize) < self.map.y;
		on_map && (reveal_all || self.seen[x as usize * self.map.y + y as usize])
	}

	pub(super) fn reveal_around_player(&mut self) {
		let (px, py) = (self.player.x, self.player.y);
		for x in px - REVEAL_RADIUS..=px + REVEAL_RADIUS {
			for y in py - REVEAL_RADIUS..=py + REVEAL_RADIUS {
				if x >= 0 && y >= 0 && (x as usize) < self.map.x && (y as usize) < self.map.y {
					self.seen[x as usize * self.map.y + y as usize] = true;
				}
			}
		}
	}

	/// The middle of a tile's floor, in world space, at the floor's height.
	pub(super) fn tile_center_of(&self, x: i32, y: i32) -> Vec3 {
		tile_center(x, y) + Vec3::Y * self.terrain.tile(x, y)
	}

	/// Whether the player can see what's on a tile right now, so the map
	/// shows enemies there: anything within twice the reveal radius, or
	/// anywhere with `reveal_all`.
	pub(super) fn can_see(&self, x: i32, y: i32, reveal_all: bool) -> bool {
		reveal_all || (x - self.player.x).abs().max((y - self.player.y).abs()) <= REVEAL_RADIUS * 2
	}

	/// Walks freely for `dt` seconds, outside combat. Says whether the player
	/// just stepped onto a staircase.
	pub(super) fn walk(&mut self, forward: f32, strafe: f32, dt: f32) -> Option<Arrival> {
		let before = (self.player.x, self.player.y);
		self.player.walk(&self.map, &self.blockers, &self.terrain, forward, strafe, dt);
		self.open_nearby_chests();
		if (self.player.x, self.player.y) == before {
			return None;
		}
		self.reveal_around_player();
		self.pick_up();
		self.update_combat();
		self.arrival()
	}

	/// What's under the player now.
	pub(super) fn arrival(&self) -> Option<Arrival> {
		let here = (self.player.x, self.player.y);
		Some(if here == (self.stairs_down.0, self.stairs_down.1) {
			Arrival::StairsDown
		} else if Some(here) == self.stairs_up.map(|s| (s.0, s.1)) {
			Arrival::StairsUp
		} else {
			Arrival::Floor
		})
	}

	/// Passes an action to the player, uncovers the map around them, and says
	/// whether they just stepped onto a staircase. In combat, stepping into an
	/// enemy attacks it, and every step or attack gives the enemies a turn.
	pub(super) fn act(&mut self, action: Action) -> Option<Arrival> {
		if action == Action::Attack {
			self.attack();
			return None;
		}
		if self.combat && matches!(action, Action::Forward | Action::Back) {
			let (dx, dy) = self.player.forward();
			let sign = if action == Action::Forward { 1 } else { -1 };
			let target = (self.player.x + dx * sign, self.player.y + dy * sign);
			if let Some(i) = self.enemy_at(target.0, target.1) {
				// Walking into an enemy attacks it
				self.events.push(Event::Swing);
				self.strike(i);
				self.enemies_act();
				self.update_combat();
				return None;
			}
			if let Some(c) = self.chest_at(target.0, target.1) {
				// Chests can't be walked onto; bumping a closed one opens it
				if !self.chests[c].open {
					self.open_chest(c);
					self.enemies_act();
					self.update_combat();
				}
				return None;
			}
		}

		let before = (self.player.x, self.player.y);
		self.player.act(action, &self.map, &self.terrain);
		let here = (self.player.x, self.player.y);
		if here == before {
			// Turning in place, or bumping a wall, never triggers stairs or
			// costs a turn
			return None;
		}
		self.reveal_around_player();
		self.pick_up();
		if self.combat {
			self.enemies_act();
		}
		self.update_combat();
		Some(if here == (self.stairs_down.0, self.stairs_down.1) {
			Arrival::StairsDown
		} else if Some(here) == self.stairs_up.map(|s| (s.0, s.1)) {
			Arrival::StairsUp
		} else {
			Arrival::Floor
		})
	}

	/// Puts the player on the first open tile next to `tile` (a staircase or
	/// start), facing away from it and looking straight ahead, so they aren't
	/// standing inside the stairs; a tile no chest or monster is on, if there
	/// is one. Uncovers the map around them.
	pub(super) fn place_player(&mut self, tile: (i32, i32, i32)) {
		let next_to = |f: usize| (tile.0 + DIRECTIONS[f].0, tile.1 + DIRECTIONS[f].1);
		let open: Vec<usize> = (0..4)
			.filter(|&f| {
				let (x, y) = next_to(f);
				!self.map.is_wall(x, y)
			})
			.collect();
		let free = open.iter().copied().find(|&f| {
			let (x, y) = next_to(f);
			self.chest_at(x, y).is_none() && self.enemy_at(x, y).is_none()
		});
		// Boxed in, the player stays on the tile itself, facing north
		let facing = free.or(open.first().copied());
		let (x, y) = facing.map_or((tile.0, tile.1), next_to);
		self.player.x = x;
		self.player.y = y;
		self.player.z = tile.2;
		self.player.pos = self.tile_center_of(x, y);
		self.player.facing = facing.unwrap_or(0);
		self.player.look_yaw = 0.0;
		self.player.look_pitch = 0.0;
		self.reveal_around_player();
	}
}

/// The first direction from `tile` that isn't a wall, so the view isn't a wall.
pub(super) fn open_facing(map: &MapGrid, tile: (i32, i32, i32)) -> usize {
	(0..4)
		.find(|&f| {
			let (dx, dy) = DIRECTIONS[f];
			!map.is_wall(tile.0 + dx, tile.1 + dy)
		})
		.unwrap_or(0)
}

pub(super) fn tile_center(x: i32, y: i32) -> Vec3 {
	Vec3::new((x as f32 + 0.5) * TILE, 0.0, (y as f32 + 0.5) * TILE)
}
