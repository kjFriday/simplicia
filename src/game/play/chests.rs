// Chests: they open when the player walks up to one (or bumps it in combat),
// and their loot pops out onto the floor around them.
//
// Chests live in `World::chests` (world.rs); levels/ places them. Their loot
// is ordinary ground items from items.rs, and view/ shows the lid opening
// from the `Event::ChestOpened` this pushes.
use rand::RngExt;

use super::items::{GroundItem, ItemKind};
use super::world::{Event, World};

/// How close the player has to get to open a chest, in meters.
const CHEST_REACH: f32 = 2.4;
/// Most things one chest holds.
const MAX_LOOT: usize = 3;

/// A chest on the map. `id` lets the view match its model to it.
pub(crate) struct Chest {
	pub(crate) id: u32,
	pub(crate) x: i32,
	pub(crate) y: i32,
	/// Its turn about Y, in radians
	pub(crate) yaw: f32,
	pub(crate) open: bool,
}

/// Opening chests.
impl World {
	/// The index of the chest on tile (x, y), if there is one.
	pub(crate) fn chest_at(&self, x: i32, y: i32) -> Option<usize> {
		self.chests.iter().position(|c| (c.x, c.y) == (x, y))
	}

	/// Opens every closed chest within reach of the player.
	pub(crate) fn open_nearby_chests(&mut self) {
		let feet = self.player.pos;
		let near: Vec<usize> = (0..self.chests.len())
			.filter(|&i| {
				let c = &self.chests[i];
				!c.open && self.tile_center_of(c.x, c.y).distance(feet) < CHEST_REACH
			})
			.collect();
		for i in near {
			self.open_chest(i);
		}
	}

	/// Opens chest `i`: 1 to MAX_LOOT items fly out onto the free floor around it.
	pub(crate) fn open_chest(&mut self, i: usize) {
		if self.chests[i].open {
			return;
		}
		self.chests[i].open = true;
		let (cx, cy) = (self.chests[i].x, self.chests[i].y);
		let at = self.tile_center_of(cx, cy);
		self.events.push(Event::ChestOpened { at });

		let mut spots = self.loot_spots(cx, cy);
		let mut rng = rand::rng();
		let count = rng.random_range(1..=MAX_LOOT).min(spots.len());
		for _ in 0..count {
			let (x, y) = spots.swap_remove(rng.random_range(0..spots.len()));
			let kind = ItemKind::random(&mut rng);
			let id = self.next_item_id;
			self.next_item_id += 1;
			self.items.push(GroundItem { id, x, y, kind, from: Some((cx, cy)) });
		}
		self.say(match count {
			0 => "The chest is empty.".to_string(),
			1 => "Something tumbles out of the chest!".to_string(),
			n => format!("{n} things tumble out of the chest!"),
		});
		self.pick_up();
	}

	/// The tiles loot from the chest on (cx, cy) can land on.
	fn loot_spots(&self, cx: i32, cy: i32) -> Vec<(i32, i32)> {
		// Free floor next to the chest (diagonals too). The player's own tile
		// is the last choice: what lands there is picked up straight away
		let mut spots: Vec<(i32, i32)> = Vec::new();
		let mut under_player = false;
		for dx in -1..=1 {
			for dy in -1..=1 {
				let (x, y) = (cx + dx, cy + dy);
				let busy = self.map.is_wall(x, y)
					|| self.items.iter().any(|it| (it.x, it.y) == (x, y))
					|| self.chest_at(x, y).is_some()
					|| self.enemy_at(x, y).is_some()
					|| (x, y) == (self.stairs_down.0, self.stairs_down.1)
					|| !self.terrain.can_step((cx, cy), (x, y)) && (dx == 0 || dy == 0);
				if busy {
					continue;
				}
				if (x, y) == (self.player.x, self.player.y) {
					under_player = true;
				} else {
					spots.push((x, y));
				}
			}
		}
		if spots.is_empty() && under_player {
			spots.push((self.player.x, self.player.y));
		}
		spots
	}
}
