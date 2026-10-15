// Level presets: the generator's settings plus a seed. The debug build's
// Generator screen makes them and saves each one as a folder under levels/,
// with every room of the level in its own file. The game build plays only
// the saved presets, in order, one per depth; with none saved it falls back
// to the default settings and a random seed.
//
// The game passes a preset's settings to `World::generate_with` (mod.rs);
// the Generator screen (ui/) edits `GenSettings` with its sliders.
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::dungeons::mapgen::{generate_dungeon_with_options, MapGrid, Room};

/// Where presets are saved: one folder per level.
pub(crate) const LEVELS_DIR: &str = "levels";
const PRESET_FILE: &str = "level.preset";

/// Everything that shapes a generated level. The Generator screen has a
/// slider for each.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct GenSettings {
	pub(crate) width: usize,
	pub(crate) height: usize,
	pub(crate) max_rooms: usize,
	pub(crate) room_min: i32,
	pub(crate) room_max: i32,
	/// How many times the map is split before rooms go in; more is busier
	pub(crate) splits: usize,
	/// Percent of corridors that run diagonally
	pub(crate) diagonal: u32,
	/// Percent chance a room gets a monster, and an item
	pub(crate) enemies: u32,
	pub(crate) items: u32,
}

impl Default for GenSettings {
	fn default() -> Self {
		Self {
			width: 48,
			height: 32,
			max_rooms: 12,
			room_min: 4,
			room_max: 8,
			splits: 4,
			diagonal: 35,
			enemies: 50,
			items: 60,
		}
	}
}

/// One slider: its label, range, and step.
pub(crate) struct Slider {
	pub(crate) label: &'static str,
	pub(crate) min: i64,
	pub(crate) max: i64,
	pub(crate) step: i64,
}

impl GenSettings {
	pub(crate) const SLIDERS: [Slider; 9] = [
		Slider { label: "Width", min: 24, max: 96, step: 4 },
		Slider { label: "Height", min: 16, max: 64, step: 4 },
		Slider { label: "Rooms", min: 2, max: 30, step: 1 },
		Slider { label: "Room min", min: 3, max: 12, step: 1 },
		Slider { label: "Room max", min: 4, max: 16, step: 1 },
		Slider { label: "Splits", min: 1, max: 7, step: 1 },
		Slider { label: "Diagonal %", min: 0, max: 100, step: 5 },
		Slider { label: "Monsters %", min: 0, max: 100, step: 10 },
		Slider { label: "Items %", min: 0, max: 100, step: 10 },
	];

	pub(crate) fn get(&self, i: usize) -> i64 {
		match i {
			0 => self.width as i64,
			1 => self.height as i64,
			2 => self.max_rooms as i64,
			3 => self.room_min as i64,
			4 => self.room_max as i64,
			5 => self.splits as i64,
			6 => self.diagonal as i64,
			7 => self.enemies as i64,
			_ => self.items as i64,
		}
	}

	pub(crate) fn set(&mut self, i: usize, v: i64) {
		self.set_unchecked(i, v);
		self.fit_rooms();
	}

	/// Sets slider `i`, kept within its range, without fitting the rooms to
	/// the rest of the settings.
	fn set_unchecked(&mut self, i: usize, v: i64) {
		let s = &Self::SLIDERS[i];
		let v = v.clamp(s.min, s.max);
		match i {
			0 => self.width = v as usize,
			1 => self.height = v as usize,
			2 => self.max_rooms = v as usize,
			3 => self.room_min = v as i32,
			4 => self.room_max = v as i32,
			5 => self.splits = v as usize,
			6 => self.diagonal = v as u32,
			7 => self.enemies = v as u32,
			_ => self.items = v as u32,
		}
	}

	/// Rooms must fit: max at least min, and both well inside the map.
	fn fit_rooms(&mut self) {
		self.room_min = self.room_min.min(self.room_max);
		let fit = (self.width.min(self.height) as i32 / 2 - 2).max(3);
		self.room_max = self.room_max.min(fit).max(self.room_min);
	}

	/// Moves slider `i` by `steps` of its step size.
	pub(crate) fn nudge(&mut self, i: usize, steps: i64) {
		self.set(i, self.get(i) + steps * Self::SLIDERS[i].step);
	}

	/// The map and rooms these settings make from `seed`.
	pub(crate) fn layout(&self, seed: u64) -> (MapGrid, Vec<Room>) {
		use rand::SeedableRng;
		let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
		generate_dungeon_with_options(
			&mut rng,
			self.width,
			self.height,
			self.splits,
			self.max_rooms,
			self.room_min,
			self.room_max,
			self.diagonal as f64 / 100.0,
		)
	}
}

/// A saved level: its settings and seed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Preset {
	pub(crate) settings: GenSettings,
	pub(crate) seed: u64,
}

impl Preset {
	fn to_text(&self) -> String {
		let mut text = String::from("# level preset: generator settings and seed\n");
		let _ = writeln!(text, "seed {}", self.seed);
		for (i, s) in GenSettings::SLIDERS.iter().enumerate() {
			let _ = writeln!(text, "{} {}", s.label.replace(' ', "_"), self.settings.get(i));
		}
		text
	}

	fn from_text(text: &str) -> Result<Self, String> {
		let mut preset = Preset { settings: GenSettings::default(), seed: 0 };
		for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
			let (key, value) = line.split_once(' ').ok_or_else(|| format!("bad line: {line}"))?;
			let value = value.trim();
			let bad = || format!("bad number: {line}");
			// Seeds use all 64 bits, so they're read unsigned
			if key == "seed" {
				preset.seed = value.parse().map_err(|_| bad())?;
			} else if let Some(i) = GenSettings::SLIDERS.iter().position(|s| s.label.replace(' ', "_") == key) {
				preset.settings.set_unchecked(i, value.parse().map_err(|_| bad())?);
			}
		}
		// Fitted once everything's in: a value read early can't be checked
		// against one that comes later
		preset.settings.fit_rooms();
		Ok(preset)
	}

	/// Saves the preset as a new folder `levels/level_NN/`: the settings in
	/// level.preset, and each room in its own room_NN.room file (its
	/// position and size on the map). Returns the folder's name.
	pub(crate) fn save(&self) -> std::io::Result<String> {
		let free = |n: &String| !Path::new(LEVELS_DIR).join(n).exists();
		let name = (1..).map(|n| format!("level_{n:02}")).find(free).unwrap();
		let dir = Path::new(LEVELS_DIR).join(&name);
		std::fs::create_dir_all(&dir)?;
		std::fs::write(dir.join(PRESET_FILE), self.to_text())?;
		let (_, rooms) = self.settings.layout(self.seed);
		for (i, r) in rooms.iter().enumerate() {
			let (w, h) = (r.x2 - r.x1, r.y2 - r.y1);
			let text = format!("# room: x y width height, in map tiles\n{} {} {w} {h}\n", r.x1, r.y1);
			std::fs::write(dir.join(format!("room_{:02}.room", i + 1)), text)?;
		}
		Ok(name)
	}

	/// Every saved preset, in folder-name order.
	pub(crate) fn load_all() -> Vec<Preset> {
		let mut dirs: Vec<PathBuf> = std::fs::read_dir(LEVELS_DIR)
			.into_iter()
			.flatten()
			.flatten()
			.map(|e| e.path())
			.filter(|p| p.join(PRESET_FILE).is_file())
			.collect();
		dirs.sort();
		dirs.iter()
			.filter_map(|d| std::fs::read_to_string(d.join(PRESET_FILE)).ok())
			.filter_map(|t| Preset::from_text(&t).ok())
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn presets_round_trip_and_sliders_keep_rooms_valid() {
		let mut settings = GenSettings::default();
		settings.nudge(3, 100); // room min past room max
		assert!(settings.room_min <= settings.room_max);
		let preset = Preset { settings, seed: 1234 };
		assert_eq!(Preset::from_text(&preset.to_text()).unwrap(), preset);
		let (map, rooms) = preset.settings.layout(preset.seed);
		assert_eq!(map.x, settings.width);
		assert!(!rooms.is_empty());

		// Seeds past i64::MAX, and a room min bigger than the default room max
		let mut big = GenSettings::default();
		big.set(4, 12);
		big.set(3, 12);
		assert_eq!((big.room_min, big.room_max), (12, 12));
		let big = Preset { settings: big, seed: u64::MAX };
		assert_eq!(Preset::from_text(&big.to_text()).unwrap(), big);
	}
}
