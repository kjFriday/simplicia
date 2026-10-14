// The main menu and the pause menu: lists of options drawn as HUD text, moved
// through with W/S or the arrows and picked with Enter.
use super::minimap::{put, put_str};
use super::Framebuffer;

/// What the game is showing.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Screen {
	/// The title screen; `selected` indexes MAIN_OPTIONS
	MainMenu { selected: usize },
	Playing,
	/// The game, frozen behind the pause menu; `selected` indexes PAUSE_OPTIONS
	Paused { selected: usize },
}

/// Something a menu option does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Choice {
	NewGame,
	Resume,
	MainMenu,
	Quit,
}

const MAIN_OPTIONS: &[(&str, Choice)] = &[("New Game", Choice::NewGame), ("Quit", Choice::Quit)];
const PAUSE_OPTIONS: &[(&str, Choice)] =
	&[("Resume", Choice::Resume), ("Main Menu", Choice::MainMenu), ("Quit", Choice::Quit)];

const TITLE: &str = "RPG GAME";
const TEXT: [u8; 3] = [200, 200, 200];
const HIGHLIGHT: [u8; 3] = [255, 220, 90];
const BORDER: [u8; 3] = [110, 110, 120];
const HINT: [u8; 3] = [110, 110, 110];

impl Screen {
	fn options(&self) -> &'static [(&'static str, Choice)] {
		match self {
			Screen::MainMenu { .. } => MAIN_OPTIONS,
			Screen::Paused { .. } => PAUSE_OPTIONS,
			Screen::Playing => &[],
		}
	}

	/// Moves the highlight up (-1) or down (+1), wrapping around.
	pub(super) fn move_selection(&mut self, step: i32) {
		let len = self.options().len() as i32;
		if let Screen::MainMenu { selected } | Screen::Paused { selected } = self {
			*selected = (*selected as i32 + step).rem_euclid(len) as usize;
		}
	}

	/// The highlighted option, if a menu is open.
	pub(super) fn choice(&self) -> Option<Choice> {
		match self {
			Screen::MainMenu { selected } | Screen::Paused { selected } => Some(self.options()[*selected].1),
			Screen::Playing => None,
		}
	}
}

/// Draws the open menu, if any, over the HUD. The main menu covers the whole
/// screen; the pause menu is a box in the middle with the game behind it.
pub(super) fn draw(screen: Screen, buffer: &mut Framebuffer) {
	let selected = match screen {
		Screen::MainMenu { selected } | Screen::Paused { selected } => selected,
		Screen::Playing => return,
	};
	let options = screen.options();
	let (w, h) = (buffer.width as i32, buffer.height as i32);

	let (title, box_w, box_h) = match screen {
		Screen::MainMenu { .. } => {
			// Solid black over everything
			for y in 0..h {
				for x in 0..w {
					put(buffer, x, y, ' ', TEXT);
				}
			}
			(TITLE, w, h)
		}
		_ => ("PAUSED", 24, options.len() as i32 + 6),
	};

	// Box, centered, with its border and a blank inside
	let (x0, y0) = ((w - box_w) / 2, (h - box_h) / 2);
	for y in y0..y0 + box_h {
		for x in x0..x0 + box_w {
			let edge_x = x == x0 || x == x0 + box_w - 1;
			let edge_y = y == y0 || y == y0 + box_h - 1;
			let ch = match (edge_x, edge_y) {
				(true, true) => match (x == x0, y == y0) {
					(true, true) => '┌',
					(false, true) => '┐',
					(true, false) => '└',
					(false, false) => '┘',
				},
				(true, false) => '│',
				(false, true) => '─',
				(false, false) => ' ',
			};
			put(buffer, x, y, ch, BORDER);
		}
	}

	let center = |text: &str| x0 + (box_w - text.chars().count() as i32) / 2;
	let mid = y0 + box_h / 2;
	let top = mid - options.len() as i32 / 2;
	put_str(buffer, center(title), top - 3, title, HIGHLIGHT);
	for (i, (label, _)) in options.iter().enumerate() {
		let line = if i == selected { format!("> {label} <") } else { label.to_string() };
		let rgb = if i == selected { HIGHLIGHT } else { TEXT };
		put_str(buffer, center(&line), top + i as i32, &line, rgb);
	}
	let hint = "W/S move  Enter select";
	put_str(buffer, center(hint), top + options.len() as i32 + 1, hint, HINT);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn selection_wraps_and_picks() {
		let mut screen = Screen::Paused { selected: 0 };
		screen.move_selection(-1);
		assert_eq!(screen.choice(), Some(Choice::Quit));
		screen.move_selection(1);
		assert_eq!(screen.choice(), Some(Choice::Resume));
		assert_eq!(Screen::Playing.choice(), None);
	}

	#[test]
	fn main_menu_covers_the_screen_and_pause_menu_does_not() {
		let mut fb = Framebuffer::new(80, 30);
		draw(Screen::MainMenu { selected: 0 }, &mut fb);
		assert!(fb.color_buffer.iter().all(|c| c.ui));
		let text: String = fb.color_buffer.iter().map(|c| c.ch).collect();
		assert!(text.contains("> New Game <") && text.contains(TITLE));

		let mut fb = Framebuffer::new(80, 30);
		draw(Screen::Paused { selected: 1 }, &mut fb);
		assert!(!fb.color_buffer[0].ui);
		let text: String = fb.color_buffer.iter().map(|c| c.ch).collect();
		assert!(text.contains("> Main Menu <") && text.contains("PAUSED"));
	}
}
