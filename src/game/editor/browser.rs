// The level browser, opened with B inside the editor: a list of every room
// prefab and cell saved so far, plus "new" entries. edit.rs hands it the keys
// while it's open (`browse`), and mod.rs's `draw_hud` draws it (`draw_browser`).
// Opening a file goes through edit.rs's `open`.
use bevy::prelude::*;

use super::edit::{open, Spawner};
use super::{Editor, PREFAB_DIR};
use crate::engine::hud::{put_str, Framebuffer};
use crate::game::levels::cells::{CellKind, CELLS_DIR};
use crate::game::ui::controls::{self, editor as key};

/// The browser: every room prefab and cell saved so far, then a "new" entry
/// for a room and for each kind of cell. Each room or cell is its own file.
pub(super) struct Browser {
	/// Paths like prefabs/room.prefab or cells/room_edge_1.prefab
	files: Vec<String>,
	selected: usize,
}

impl Browser {
	/// Lists prefabs/ and cells/, highlighting `current` if it's there.
	pub(super) fn open(current: &str) -> Self {
		let mut files = Vec::new();
		for dir in [PREFAB_DIR, CELLS_DIR] {
			let mut names: Vec<String> = std::fs::read_dir(dir)
				.into_iter()
				.flatten()
				.flatten()
				.filter_map(|e| e.file_name().into_string().ok())
				.filter(|n| n.ends_with(".prefab"))
				.map(|n| format!("{dir}/{n}"))
				.collect();
			names.sort();
			files.extend(names);
		}
		let selected = files.iter().position(|f| f == current).unwrap_or(0);
		Self { files, selected }
	}

	/// Entries after the files: a new room, then a new cell of each kind.
	fn new_entries(&self) -> Vec<String> {
		let free = |prefix: &str| {
			(1..).map(|n| format!("{prefix}_{n}.prefab")).find(|n| !self.files.contains(n)).unwrap()
		};
		let mut entries = vec![free(&format!("{PREFAB_DIR}/room"))];
		entries.extend(CellKind::ALL.iter().map(|k| free(&format!("{CELLS_DIR}/{}", k.prefix()))));
		entries
	}

	fn len(&self) -> usize {
		self.files.len() + self.new_entries().len()
	}

	/// The highlighted file, and whether it's a new one.
	fn chosen(&self) -> (String, bool) {
		match self.files.get(self.selected) {
			Some(f) => (f.clone(), false),
			None => (self.new_entries()[self.selected - self.files.len()].clone(), true),
		}
	}
}

/// The level browser's turn at the keys while it's open: W/S pick a file,
/// Enter opens it (or starts a new level), Esc or B closes the browser.
pub(super) fn browse(editor: &mut Editor, spawner: &mut Spawner, keys: &ButtonInput<KeyCode>) {
	let Some(browser) = &mut editor.browser else { return };
	let count = browser.len();
	if keys.any_just_pressed(controls::MENU_UP) {
		browser.selected = (browser.selected + count - 1) % count;
	}
	if keys.any_just_pressed(controls::MENU_DOWN) {
		browser.selected = (browser.selected + 1) % count;
	}
	if keys.any_just_pressed([controls::MENU_BACK, key::BROWSER]) {
		editor.browser = None;
	} else if keys.any_just_pressed(controls::MENU_SELECT) {
		editor.file = browser.chosen().0;
		editor.browser = None;
		open(editor, spawner);
	}
}

/// The level browser: the saved prefabs, the open one marked, and "new level".
pub(super) fn draw_browser(browser: &Browser, open_file: &str, buffer: &mut Framebuffer) {
	const TEXT: [u8; 3] = [220, 220, 220];
	const HIGHLIGHT: [u8; 3] = [255, 220, 90];
	const HINT: [u8; 3] = [130, 130, 130];
	let mut lines: Vec<String> = browser
		.files
		.iter()
		.map(|f| if f == open_file { format!("{f}  (open)") } else { f.clone() })
		.collect();
	lines.extend(browser.new_entries().iter().map(|f| format!("+ new {f}")));
	let top = 2;
	put_str(buffer, 2, 0, "ROOMS and CELLS (cells are one 4 m tile, built facing north)", HIGHLIGHT);
	for (i, line) in lines.iter().enumerate() {
		let chosen = i == browser.selected;
		let text = if chosen { format!("> {line}") } else { format!("  {line}") };
		put_str(buffer, 2, top + i as i32, &text, if chosen { HIGHLIGHT } else { TEXT });
	}
	put_str(buffer, 2, top + lines.len() as i32 + 1, "W/S pick  Enter open  Esc/B close", HINT);
}
