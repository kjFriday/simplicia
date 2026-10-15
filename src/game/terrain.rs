// Terrain: how high the floor is on every tile. Rooms can be raised or
// sunk, with a stair tile (a ramp) joining them to their corridors, and
// rooms can hold ledges and pits. Walking follows the floor up and down,
// and nothing can climb more than STEP_HEIGHT at once.
use glam::Vec3;

use super::world::{DIRECTIONS, TILE};

/// The biggest rise or drop a step can take, in meters. Ledges and pits are
/// a little less than this; whole raised rooms need a stair tile.
pub(super) const STEP_HEIGHT: f32 = 1.3;

/// A stair tile: the floor rises from `low` on the edge opposite `dir` to
/// `high` on the edge toward `dir` (0 north, 1 east, 2 south, 3 west).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Ramp {
	pub(super) dir: usize,
	pub(super) low: f32,
	pub(super) high: f32,
}

/// Floor heights for every tile of a map.
pub(super) struct Terrain {
	width: usize,
	height: usize,
	floor: Vec<f32>,
	ramps: Vec<Option<Ramp>>,
}

impl Terrain {
	/// Everything at height 0.
	pub(super) fn flat(width: usize, height: usize) -> Self {
		Self { width, height, floor: vec![0.0; width * height], ramps: vec![None; width * height] }
	}

	fn index(&self, x: i32, y: i32) -> Option<usize> {
		(x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height)
			.then(|| y as usize * self.width + x as usize)
	}

	/// The floor height of a flat tile; for a stair tile, the height halfway up.
	pub(super) fn tile(&self, x: i32, y: i32) -> f32 {
		let Some(i) = self.index(x, y) else { return 0.0 };
		match self.ramps[i] {
			Some(r) => (r.low + r.high) / 2.0,
			None => self.floor[i],
		}
	}

	pub(super) fn set(&mut self, x: i32, y: i32, h: f32) {
		if let Some(i) = self.index(x, y) {
			self.floor[i] = h;
			self.ramps[i] = None;
		}
	}

	pub(super) fn set_ramp(&mut self, x: i32, y: i32, ramp: Ramp) {
		if let Some(i) = self.index(x, y) {
			self.floor[i] = ramp.low;
			self.ramps[i] = Some(ramp);
		}
	}

	pub(super) fn ramp(&self, x: i32, y: i32) -> Option<Ramp> {
		self.index(x, y).and_then(|i| self.ramps[i])
	}

	/// The floor height on tile (x, y)'s edge toward `dir`.
	fn edge(&self, x: i32, y: i32, dir: usize) -> f32 {
		match self.ramp(x, y) {
			Some(r) if r.dir == dir => r.high,
			Some(r) if (r.dir + 2) % 4 == dir => r.low,
			_ => self.tile(x, y),
		}
	}

	/// The floor height under a point in the world.
	pub(super) fn at(&self, pos: Vec3) -> f32 {
		let (x, y) = ((pos.x / TILE).floor() as i32, (pos.z / TILE).floor() as i32);
		match self.ramp(x, y) {
			Some(r) => {
				// How far along the rise the point is, 0 at the low edge, 1 at the high
				let center = Vec3::new((x as f32 + 0.5) * TILE, 0.0, (y as f32 + 0.5) * TILE);
				let (dx, dy) = DIRECTIONS[r.dir];
				let along = (pos - center).dot(Vec3::new(dx as f32, 0.0, dy as f32)) / TILE + 0.5;
				r.low + (r.high - r.low) * along.clamp(0.0, 1.0)
			}
			None => self.tile(x, y),
		}
	}

	/// Whether one step from tile `from` to the neighboring tile `to` is
	/// small enough to take.
	pub(super) fn can_step(&self, from: (i32, i32), to: (i32, i32)) -> bool {
		let Some(dir) = DIRECTIONS.iter().position(|&d| d == (to.0 - from.0, to.1 - from.1)) else { return false };
		let out = self.edge(from.0, from.1, dir);
		let into = self.edge(to.0, to.1, (dir + 2) % 4);
		(out - into).abs() <= STEP_HEIGHT
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn ramps_join_heights_and_big_drops_block() {
		let mut t = Terrain::flat(4, 1);
		t.set(3, 0, 2.5);
		// A stair on tile 2 rising east, from 0 to 2.5
		t.set_ramp(2, 0, Ramp { dir: 1, low: 0.0, high: 2.5 });
		assert!(t.can_step((1, 0), (2, 0)) && t.can_step((2, 0), (3, 0)));
		assert_eq!(t.tile(2, 0), 1.25);
		// The west edge of the stair is low, the east edge high
		assert!((t.at(Vec3::new(2.0 * TILE + 0.01, 0.0, 2.0)) - 0.0).abs() < 0.02);
		assert!((t.at(Vec3::new(3.0 * TILE - 0.01, 0.0, 2.0)) - 2.5).abs() < 0.02);
		// Without the stair, 2.5 m is too high to step
		t.set(2, 0, 0.0);
		assert!(!t.can_step((2, 0), (3, 0)));
	}
}
