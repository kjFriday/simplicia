// Cells: hand-built pieces of rooms and halls, one 4 x 4 m map tile each,
// made in the F1 editor and saved under cells/ as `<kind>_<name>.prefab`.
// When a level is built, each floor tile picks a cell of the kind its walls
// call for (a room corner, a hall bend, ...) and turns it to fit. Tiles with
// no matching cell fall back to the dungeon kit.
//
// Every cell is built facing one way, its "base" orientation:
//   room_center  no walls             hall_straight  open north and south
//   room_edge    wall to the north    hall_turn      open north and east
//   room_corner  walls north and east hall_tee       open north, east, south
//                                     hall_cross     open on all sides
//                                     hall_end       open north only
//
// The game loads them at startup (along with the ones cellgen.rs makes), and
// tiles.rs places them; the editor (F1) saves them, read back by `parse`.
use glam::Vec3;

use super::catalog::RoomTheme;
use crate::game::editor::parse;
use crate::dungeons::mapgen::MapGrid;

/// Where cells are saved.
pub(crate) const CELLS_DIR: &str = "cells";

/// The kinds of cell. Rooms are made of the first three; halls of the rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CellKind {
	RoomCenter,
	RoomEdge,
	RoomCorner,
	HallStraight,
	HallTurn,
	HallTee,
	HallCross,
	HallEnd,
}

impl CellKind {
	pub(crate) const ALL: [CellKind; 8] = [
		CellKind::RoomCenter,
		CellKind::RoomEdge,
		CellKind::RoomCorner,
		CellKind::HallStraight,
		CellKind::HallTurn,
		CellKind::HallTee,
		CellKind::HallCross,
		CellKind::HallEnd,
	];

	/// The file-name prefix cells of this kind are saved under.
	pub(crate) fn prefix(self) -> &'static str {
		match self {
			CellKind::RoomCenter => "room_center",
			CellKind::RoomEdge => "room_edge",
			CellKind::RoomCorner => "room_corner",
			CellKind::HallStraight => "hall_straight",
			CellKind::HallTurn => "hall_turn",
			CellKind::HallTee => "hall_tee",
			CellKind::HallCross => "hall_cross",
			CellKind::HallEnd => "hall_end",
		}
	}

	pub(super) fn is_room(self) -> bool {
		matches!(self, CellKind::RoomCenter | CellKind::RoomEdge | CellKind::RoomCorner)
	}

	/// Which sides (0 north, 1 east, 2 south, 3 west) are open in the base
	/// orientation. Room cells count walls instead, so the same bits mean
	/// "wall here" for them.
	pub(super) fn base_sides(self) -> [bool; 4] {
		match self {
			CellKind::RoomCenter => [false; 4],
			CellKind::RoomEdge => [true, false, false, false],
			CellKind::RoomCorner => [true, true, false, false],
			CellKind::HallStraight => [true, false, true, false],
			CellKind::HallTurn => [true, true, false, false],
			CellKind::HallTee => [true, true, true, false],
			CellKind::HallCross => [true; 4],
			CellKind::HallEnd => [true, false, false, false],
		}
	}

	/// The kind whose prefix starts `file_name`.
	fn of_file(file_name: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|k| file_name.starts_with(&format!("{}_", k.prefix())))
	}
}

/// One model in a cell: its path under models/, where its origin goes from
/// the tile's center, and its turn in degrees.
pub(super) struct CellObject {
	pub(super) model: String,
	pub(super) pos: Vec3,
	pub(super) yaw_degrees: f32,
	pub(super) tint: Vec3,
	/// Lift it by however far it reaches below its origin, so it stands on
	/// the floor (hand-placed cells); generated cells place it exactly
	pub(super) grounded: bool,
	/// The object it stands on, by index in the cell, if any
	pub(super) parent: Option<usize>,
	/// Gives off light
	pub(super) light: bool,
}

pub(crate) struct Cell {
	pub(super) kind: CellKind,
	/// Generated cells suit one kind of room; hand-made ones suit any
	pub(super) theme: Option<RoomTheme>,
	/// Hand-made cells are the whole tile, floor and walls included; the
	/// generated ones are only what stands in it, on top of the dungeon kit
	pub(super) structure: bool,
	pub(super) objects: Vec<CellObject>,
}

/// Every saved cell. Files that don't parse are skipped.
pub(crate) fn load_all() -> Vec<Cell> {
	let mut cells = Vec::new();
	let mut files: Vec<_> =
		std::fs::read_dir(CELLS_DIR).into_iter().flatten().flatten().map(|e| e.path()).collect();
	files.sort();
	for path in files {
		let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
		let Some(kind) = CellKind::of_file(name) else { continue };
		let Ok(text) = std::fs::read_to_string(&path) else { continue };
		let Ok(entries) = parse(&text) else { continue };
		// A cell using a model that's no longer in models/ is skipped (the
		// tile falls back to the kit), rather than failing every level it's in
		let missing = entries.iter().find(|(model, ..)| crate::engine::models::get(model).is_none());
		if let Some((missing, ..)) = missing {
			if crate::game::dev_tools() {
				eprintln!("{name}: skipped, {missing} isn't in models/");
			}
			continue;
		}
		let objects = entries
			.into_iter()
			.map(|(model, pos, yaw_degrees)| CellObject {
				light: model.starts_with("Lights/"),
				model,
				pos: Vec3::from_array(pos.to_array()),
				yaw_degrees,
				tint: Vec3::ONE,
				grounded: true,
				parent: None,
			})
			.collect();
		cells.push(Cell { kind, theme: None, structure: true, objects });
	}
	cells
}

/// The cell kind a floor tile needs, and how many quarter turns clockwise
/// (north toward east) to turn the base cell so it fits. None when no kind
/// fits, like a room tile walled on three sides.
pub(crate) fn fit(map: &MapGrid, x: i32, y: i32, in_room: bool) -> Option<(CellKind, usize)> {
	const DIRS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
	// Rooms care where the walls are; halls where the openings are
	let sides: [bool; 4] = DIRS.map(|(dx, dy)| map.is_wall(x + dx, y + dy) == in_room);
	CellKind::ALL.into_iter().filter(|k| k.is_room() == in_room).find_map(|kind| {
		let base = kind.base_sides();
		(0..4).find(|&turns| (0..4).all(|d| base[d] == sides[(d + turns) % 4])).map(|turns| (kind, turns))
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tiles_get_the_right_kind_and_turn() {
		// A 3x3 room in a 7x7 map, with a hall leaving its east side
		let mut map = MapGrid::new(7, 7, 1);
		for x in 1..4 {
			for y in 1..4 {
				map.set_floor(x, y);
			}
		}
		map.set_floor(4, 2);
		map.set_floor(5, 2);
		assert_eq!(fit(&map, 2, 2, true), Some((CellKind::RoomCenter, 0)));
		assert_eq!(fit(&map, 2, 1, true), Some((CellKind::RoomEdge, 0)));
		// Top-left corner: walls north and west, the base (north, east) turned 3 times
		assert_eq!(fit(&map, 1, 1, true), Some((CellKind::RoomCorner, 3)));
		// The hall: open west and east is the straight piece turned once
		assert_eq!(fit(&map, 4, 2, false), Some((CellKind::HallStraight, 1)));
		// Its dead end opens west only: the base (north) turned 3 times
		assert_eq!(fit(&map, 5, 2, false), Some((CellKind::HallEnd, 3)));
		assert_eq!(CellKind::of_file("room_corner_mossy.prefab"), Some(CellKind::RoomCorner));
	}
}
