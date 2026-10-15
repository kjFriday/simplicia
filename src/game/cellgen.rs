// Cell generation: at startup, builds a set of cells for every kind of room
// and hall tile out of the asset catalog. Each asset goes where it belongs:
// big furniture against a wall, tables mid-room with chairs around them,
// candles and shelves on walls, chandeliers from the ceiling, and small
// things (books, potions, plates) on top of whatever can hold them.
//
// Cells are built in their base orientation (see cells.rs): the tile spans
// -2..2 m on X and Z with its floor at y = 0, and north is -Z.
use glam::{Quat, Vec3};
use rand::{Rng, RngExt};

use super::catalog::{Asset, Catalog, Mount, RoomTheme};
use super::cells::{Cell, CellKind, CellObject};

/// Variants made of each kind of cell, for each room theme.
const VARIANTS: usize = 6;
/// How far from the tile's middle the walls' faces are, and how close to an
/// open side things may go (leaving room to walk past).
const WALL_FACE: f32 = 1.85;
const OPEN_EDGE: f32 = 1.7;
/// The underside of the ceiling, for things hung from it.
const CEILING: f32 = 4.4;
/// North, east, south, west as unit vectors on the floor.
const SIDES: [Vec3; 4] = [Vec3::NEG_Z, Vec3::X, Vec3::Z, Vec3::NEG_X];

/// Makes every generated cell: VARIANTS of each room kind for each theme,
/// and VARIANTS of each hall kind.
pub(super) fn generate(catalog: &Catalog, rng: &mut impl Rng) -> Vec<Cell> {
	let mut cells = Vec::new();
	for theme in RoomTheme::ALL {
		for kind in [CellKind::RoomCenter, CellKind::RoomEdge, CellKind::RoomCorner] {
			for _ in 0..VARIANTS {
				cells.push(CellDraft::new(kind, Some(theme)).fill(catalog, rng));
			}
		}
	}
	for kind in CellKind::ALL.into_iter().filter(|k| !k.is_room()) {
		for _ in 0..VARIANTS {
			cells.push(CellDraft::new(kind, None).fill(catalog, rng));
		}
	}
	cells
}

/// A cell being built: what's in it so far, and the floor space taken.
struct CellDraft {
	kind: CellKind,
	theme: Option<RoomTheme>,
	objects: Vec<CellObject>,
	/// Per object: whether things can stand on it, the height of its top,
	/// and its footprint (min, max on X/Z) if it stands on something
	holds: Vec<bool>,
	tops: Vec<f32>,
	feet: Vec<Option<(Vec3, Vec3)>>,
	/// Floor footprints taken so far
	taken: Vec<(Vec3, Vec3)>,
}

impl CellDraft {
	fn new(kind: CellKind, theme: Option<RoomTheme>) -> Self {
		Self { kind, theme, objects: Vec::new(), holds: Vec::new(), tops: Vec::new(), feet: Vec::new(), taken: Vec::new() }
	}

	/// Sides with a wall in this kind's base orientation.
	fn walls(&self) -> Vec<usize> {
		let sides = self.kind.base_sides();
		// Room kinds mark walls; hall kinds mark openings
		(0..4).filter(|&d| sides[d] == self.kind.is_room()).collect()
	}

	/// How far things may reach toward side `d`: to the wall's face, or short
	/// of an open side.
	fn limit(&self, d: usize) -> f32 {
		if self.walls().contains(&d) { WALL_FACE } else { OPEN_EDGE }
	}

	fn fill(mut self, catalog: &Catalog, rng: &mut impl Rng) -> Cell {
		let theme = self.theme;
		let walls = self.walls();
		let by_wall = catalog.pick(theme, |a| matches!(a.mount, Mount::Floor { wall: true, .. }));
		let on_wall = catalog.pick(theme, |a| a.mount == Mount::Wall && a.size.y < 1.5);
		let wall_lights = catalog.pick(None, |a| a.mount == Mount::Wall && a.light);
		let standing = catalog.pick(theme, |a| matches!(a.mount, Mount::Floor { wall: false, flat: false }));
		let holders = catalog.pick(theme, |a| a.holds && matches!(a.mount, Mount::Floor { .. }));
		let seats = catalog.pick(theme, |a| matches!(a.mount, Mount::Floor { wall: false, flat: false }) && a.size.y < 1.6 && !a.holds);
		let carpets = catalog.pick(theme, |a| matches!(a.mount, Mount::Floor { flat: true, .. }));
		let ceiling = catalog.pick(theme, |a| a.mount == Mount::Ceiling);
		let small = catalog.pick(None, |a| matches!(a.mount, Mount::Floor { wall: false, flat: false }) && a.size.max_element() < 1.0);

		match self.kind {
			CellKind::RoomEdge | CellKind::RoomCorner => {
				// Against the walls: one or two big pieces, sometimes something hung above
				for &side in &walls {
					if rng.random_bool(0.75) {
						let near_corner = (self.kind == CellKind::RoomCorner).then(|| walls[(walls.iter().position(|&w| w == side).unwrap() + 1) % walls.len()]);
						self.against_wall(pick(rng, &by_wall), side, near_corner, rng);
					}
					if rng.random_bool(0.5) {
						self.on_wall(pick(rng, &on_wall), side, rng);
					}
				}
				if self.kind == CellKind::RoomCorner && rng.random_bool(0.4) {
					// Something small tucked into the corner, at an angle
					let (a, b) = (SIDES[walls[0]], SIDES[walls[1 % walls.len()]]);
					let at = (a + b) * (WALL_FACE - 0.5 - rng.random_range(0.0..0.3));
					let asset = pick(rng, &small);
					let (turns, w) = (quarter(rng), wobble(rng, 30.0));
					self.stand(asset, at, turns, w, None);
				}
			}
			CellKind::RoomCenter => match rng.random_range(0..10) {
				// Empty floor, so rooms stay walkable
				0..=3 => {}
				// A table (or the like) with seats around it
				4..=7 => {
					// Off the exact middle, and a little askew
					let at = Vec3::new(rng.random_range(-0.35..0.35), 0.0, rng.random_range(-0.35..0.35));
					let asset = pick(rng, &holders);
					let (turns, w) = (quarter(rng), wobble(rng, 10.0));
					if let Some(table) = self.stand(asset, at, turns, w, None) {
						self.seats_around(table, &seats, rng);
					}
				}
				// A carpet, maybe with something on it
				8 => {
					let asset = pick(rng, &carpets);
					let (turns, w) = (quarter(rng), wobble(rng, 6.0));
					self.stand(asset, Vec3::ZERO, turns, w, None);
					if rng.random_bool(0.5) {
						let asset = pick(rng, &holders);
						let (turns, w) = (quarter(rng), wobble(rng, 10.0));
						self.stand(asset, Vec3::ZERO, turns, w, None);
					}
				}
				// A chandelier overhead
				_ => self.hang(pick(rng, &ceiling)),
			},
			// Halls: mostly bare, sometimes a light or a crate along a wall
			_ => {
				if let Some(&side) = walls.get(rng.random_range(0..walls.len().max(1))) {
					if rng.random_bool(0.35) {
						self.on_wall(pick(rng, &wall_lights), side, rng);
					} else if rng.random_bool(0.25) {
						self.against_wall(pick(rng, &small), side, None, rng);
					}
				}
			}
		}
		// Small things on top of everything that can hold them
		let surface = catalog.pick(theme, |a| a.mount == Mount::Surface);
		for i in 0..self.objects.len() {
			if self.holds[i] {
				for _ in 0..rng.random_range(1..=3) {
					self.on_top(i, pick(rng, &surface), rng);
				}
			}
		}
		Cell { kind: self.kind, theme, structure: false, objects: self.objects }
	}
}

/// Placing things in a draft.
impl CellDraft {
	/// Places `asset` with the middle of its footprint at `at` (on the floor
	/// unless `at.y` says otherwise), turned `turns` quarter turns and then
	/// `wobble` radians more, so things don't all sit square. Returns its
	/// index, or None if it doesn't fit.
	fn stand(&mut self, asset: Option<&Asset>, at: Vec3, turns: u32, wobble: f32, parent: Option<usize>) -> Option<usize> {
		let asset = asset?;
		let yaw = turns as f32 * std::f32::consts::FRAC_PI_2 + wobble;
		let (w, d) = if turns % 2 == 0 { (asset.size.x, asset.size.z) } else { (asset.size.z, asset.size.x) };
		let half = Vec3::new(w / 2.0, 0.0, d / 2.0);
		let (lo, hi) = (at - half, at + half);
		// Inside the tile, short of its walls and open sides
		if lo.x < -self.limit(3) || hi.x > self.limit(1) || lo.z < -self.limit(0) || hi.z > self.limit(2) {
			return None;
		}
		// Clear of other things on the floor (carpets and things on top don't count)
		let flat = matches!(asset.mount, Mount::Floor { flat: true, .. });
		if parent.is_none() && !flat && self.taken.iter().any(|&(a, b)| lo.x < b.x && hi.x > a.x && lo.z < b.z && hi.z > a.z) {
			return None;
		}
		if parent.is_none() && !flat {
			self.taken.push((lo, hi));
		}
		Some(self.add(asset, at + Quat::from_rotation_y(yaw) * asset.offset, yaw, parent, Some((lo, hi))))
	}

	fn add(&mut self, asset: &Asset, origin: Vec3, yaw: f32, parent: Option<usize>, foot: Option<(Vec3, Vec3)>) -> usize {
		self.objects.push(CellObject {
			model: asset.model.to_string(),
			pos: origin,
			yaw_degrees: yaw.to_degrees(),
			tint: asset.tint,
			grounded: false,
			parent,
			light: asset.light,
		});
		self.holds.push(asset.holds);
		self.tops.push(origin.y - asset.offset.y + asset.size.y);
		self.feet.push(foot);
		self.objects.len() - 1
	}

	/// Puts `asset` with its back to the wall on `side`, somewhere along it;
	/// pushed toward `near_corner`'s side when that's given.
	fn against_wall(&mut self, asset: Option<&Asset>, side: usize, near_corner: Option<usize>, rng: &mut impl Rng) {
		let Some(asset) = asset else { return };
		let inward = -SIDES[side];
		let turns = turns_facing(asset, inward);
		let yaw_size = if turns % 2 == 0 { asset.size } else { Vec3::new(asset.size.z, asset.size.y, asset.size.x) };
		let depth = yaw_size.x * SIDES[side].x.abs() + yaw_size.z * SIDES[side].z.abs();
		let along_len = yaw_size.x + yaw_size.z - depth;
		let along = Vec3::new(-SIDES[side].z, 0.0, SIDES[side].x);
		let room = (2.0 * OPEN_EDGE - along_len).max(0.0) / 2.0;
		let slide = match near_corner {
			Some(c) if SIDES[c].dot(along) > 0.0 => room,
			Some(_) => -room,
			None => rng.random_range(-room..=room),
		};
		// Not quite flush, not quite square
		let gap = 0.02 + rng.random_range(0.0..0.12);
		let at = SIDES[side] * (WALL_FACE - depth / 2.0 - gap) + along * slide;
		let w = wobble(rng, 4.0);
		self.stand(Some(asset), at, turns, w, None);
	}

	/// Hangs `asset` on the wall on `side` at its own height, back to the wall.
	fn on_wall(&mut self, asset: Option<&Asset>, side: usize, rng: &mut impl Rng) {
		let Some(asset) = asset else { return };
		let out = model_out(asset);
		let inward = -SIDES[side];
		let yaw = inward.x.atan2(inward.z) - out.x.atan2(out.z);
		let along = Vec3::new(-SIDES[side].z, 0.0, SIDES[side].x);
		let slide = rng.random_range(-0.8..=0.8);
		// The back stays where the model's origin is; center it along the wall
		let center_along = if asset.wall_axis_x { Vec3::new(0.0, 0.0, asset.offset.z) } else { Vec3::new(asset.offset.x, 0.0, 0.0) };
		let origin = SIDES[side] * WALL_FACE + along * slide + Quat::from_rotation_y(yaw) * center_along;
		// Its footprint against the wall, so wall shelves can hold things too
		let (depth, length) = if asset.wall_axis_x { (asset.size.x, asset.size.z) } else { (asset.size.z, asset.size.x) };
		let middle = SIDES[side] * (WALL_FACE - depth / 2.0) + along * slide;
		let half = (along * length / 2.0).abs() + (SIDES[side] * depth / 2.0).abs();
		self.add(asset, origin, yaw, None, Some((middle - half, middle + half)));
	}

	/// Hangs `asset` from the ceiling over the middle of the tile.
	fn hang(&mut self, asset: Option<&Asset>) {
		let Some(asset) = asset else { return };
		let at = Vec3::new(0.0, CEILING - asset.size.y, 0.0);
		self.add(asset, at + asset.offset, 0.0, None, None);
	}

	/// Sets `asset` somewhere on top of object `holder`.
	fn on_top(&mut self, holder: usize, asset: Option<&Asset>, rng: &mut impl Rng) {
		let Some(asset) = asset else { return };
		let Some(&(lo, hi)) = self.footprint_of(holder) else { return };
		let margin = asset.size.x.max(asset.size.z) / 2.0 + 0.05;
		if hi.x - lo.x < 2.0 * margin || hi.z - lo.z < 2.0 * margin {
			return;
		}
		let x = rng.random_range(lo.x + margin..=hi.x - margin);
		let z = rng.random_range(lo.z + margin..=hi.z - margin);
		let at = Vec3::new(x, self.tops[holder], z);
		// Small things lie at any angle
		let w = rng.random_range(0.0..std::f32::consts::TAU);
		self.stand(Some(asset), at, 0, w, Some(holder));
	}

	/// Seats (chairs, stools, benches) beside object `table`.
	fn seats_around(&mut self, table: usize, seats: &[&Asset], rng: &mut impl Rng) {
		let Some(&(lo, hi)) = self.footprint_of(table) else { return };
		for side in 0..4 {
			if !rng.random_bool(0.6) {
				continue;
			}
			let Some(seat) = pick(rng, seats) else { return };
			let mid = (lo + hi) / 2.0;
			let edge = Vec3::new(
				if SIDES[side].x > 0.0 { hi.x } else if SIDES[side].x < 0.0 { lo.x } else { mid.x },
				0.0,
				if SIDES[side].z > 0.0 { hi.z } else if SIDES[side].z < 0.0 { lo.z } else { mid.z },
			);
			// Pulled out by different amounts, turned a little: facing the table
			let pulled = rng.random_range(0.05..0.35);
			let at = edge + SIDES[side] * (seat.size.x.max(seat.size.z) / 2.0 + pulled);
			let w = wobble(rng, 18.0);
			self.stand(Some(seat), at, ((side + 2) % 4) as u32, w, None);
		}
	}

	/// The footprint of object `i`, if it stands on something.
	fn footprint_of(&self, i: usize) -> Option<&(Vec3, Vec3)> {
		self.feet.get(i)?.as_ref()
	}
}

/// The direction a wall-type model sticks out of its wall, in its own space.
fn model_out(asset: &Asset) -> Vec3 {
	let sign = if asset.out_positive { 1.0 } else { -1.0 };
	if asset.wall_axis_x { Vec3::X * sign } else { Vec3::Z * sign }
}

/// Quarter turns that point `asset`'s front (away from its back) along `inward`.
fn turns_facing(asset: &Asset, inward: Vec3) -> u32 {
	let out = model_out(asset);
	(0..4).find(|&t| (Quat::from_rotation_y(t as f32 * std::f32::consts::FRAC_PI_2) * out).distance(inward) < 0.1).unwrap_or(0)
}

/// A random turn of up to `degrees` either way, in radians.
fn wobble(rng: &mut impl Rng, degrees: f32) -> f32 {
	rng.random_range(-degrees..=degrees).to_radians()
}

fn quarter(rng: &mut impl Rng) -> u32 {
	rng.random_range(0..4)
}

fn pick<'a>(rng: &mut impl Rng, list: &[&'a Asset]) -> Option<&'a Asset> {
	(!list.is_empty()).then(|| list[rng.random_range(0..list.len())])
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::SeedableRng;

	#[test]
	fn generated_cells_stay_inside_their_tile() {
		let catalog = Catalog::load();
		let cells = generate(&catalog, &mut rand::rngs::StdRng::seed_from_u64(1));
		assert_eq!(cells.len(), RoomTheme::ALL.len() * 3 * VARIANTS + 5 * VARIANTS);
		let placed: usize = cells.iter().map(|c| c.objects.len()).sum();
		assert!(placed > 100, "only {placed} things placed");
		for c in &cells {
			for o in &c.objects {
				assert!(o.pos.x.abs() < 3.0 && o.pos.z.abs() < 3.0, "{} at {}", o.model, o.pos);
			}
		}
	}
}
