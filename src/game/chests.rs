// Chests: they open when the player walks up to one (or bumps it in combat),
// and their loot pops out onto the floor around them.
use rand::RngExt;

use super::items::{GroundItem, ItemKind};
use super::world::{Event, World};

/// How close the player has to get to open a chest, in meters.
const CHEST_REACH: f32 = 2.4;
/// Most things one chest holds.
const MAX_LOOT: usize = 3;

/// A chest on the map. `id` lets the view match its model to it.
pub(super) struct Chest {
	pub(super) id: u32,
	pub(super) x: i32,
	pub(super) y: i32,
	/// Its turn about Y, in radians
	pub(super) yaw: f32,
	pub(super) open: bool,
}

/// Opening chests.
impl World {
	pub(super) fn chest_at(&self, x: i32, y: i32) -> Option<usize> {
		self.chests.iter().position(|c| (c.x, c.y) == (x, y))
	}

	/// Opens every closed chest within reach of the player.
	pub(super) fn open_nearby_chests(&mut self) {
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
	pub(super) fn open_chest(&mut self, i: usize) {
		if self.chests[i].open {
			return;
		}
		self.chests[i].open = true;
		let (cx, cy) = (self.chests[i].x, self.chests[i].y);
		let at = self.tile_center_of(cx, cy);
		self.events.push(Event::ChestOpened { at });

		// Free floor next to the chest (diagonals too)
		let mut spots: Vec<(i32, i32)> = Vec::new();
		for dx in -1..=1 {
			for dy in -1..=1 {
				let (x, y) = (cx + dx, cy + dy);
				let busy = self.map.is_wall(x, y)
					|| self.items.iter().any(|it| (it.x, it.y) == (x, y))
					|| self.chest_at(x, y).is_some()
					|| self.enemy_at(x, y).is_some()
					|| (x, y) == (self.stairs_down.0, self.stairs_down.1)
					|| !self.terrain.can_step((cx, cy), (x, y)) && (dx == 0 || dy == 0);
				if !busy {
					spots.push((x, y));
				}
			}
		}
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
	}
}
