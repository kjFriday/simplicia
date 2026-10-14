// The menus — main, pause, options, text options and key binds — drawn as HUD
// text, plus the settings they change. W/S or the arrows move, Enter picks,
// A/D or the arrows change a value, Esc goes back.
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;

use super::minimap::{put, put_str};
use super::Framebuffer;

/// Something the player can bind a key to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Bind {
	Forward,
	Back,
	TurnLeft,
	TurnRight,
	Pause,
}

impl Bind {
	const ALL: [Bind; 5] = [Bind::Forward, Bind::Back, Bind::TurnLeft, Bind::TurnRight, Bind::Pause];

	fn label(self) -> &'static str {
		match self {
			Bind::Forward => "Forward",
			Bind::Back => "Back",
			Bind::TurnLeft => "Turn Left",
			Bind::TurnRight => "Turn Right",
			Bind::Pause => "Pause",
		}
	}

	fn default_key(self) -> KeyCode {
		match self {
			Bind::Forward => KeyCode::KeyW,
			Bind::Back => KeyCode::KeyS,
			Bind::TurnLeft => KeyCode::KeyA,
			Bind::TurnRight => KeyCode::KeyD,
			Bind::Pause => KeyCode::Escape,
		}
	}
}

/// Smallest and largest text size, in screen pixels per font pixel.
pub(super) const TEXT_SCALE_MIN: u32 = 1;
pub(super) const TEXT_SCALE_MAX: u32 = 4;

/// Everything the options menus change.
pub(super) struct Settings {
	/// Screen pixels per font pixel; 1 draws the 8x8 font at its real size
	pub(super) text_scale: u32,
	/// Whether the 3D view is drawn as text, or shown as it renders
	pub(super) text_filter: bool,
	/// One key per action, in `Bind::ALL` order
	keys: [KeyCode; 5],
}

impl Default for Settings {
	fn default() -> Self {
		Self { text_scale: 1, text_filter: true, keys: Bind::ALL.map(Bind::default_key) }
	}
}

impl Settings {
	pub(super) fn key(&self, bind: Bind) -> KeyCode {
		self.keys[bind as usize]
	}

	/// Binds `key` to `bind`. Whatever else had that key gets `bind`'s old
	/// key instead, so no two actions ever share one.
	fn rebind(&mut self, bind: Bind, key: KeyCode) {
		let old = self.keys[bind as usize];
		if let Some(other) = self.keys.iter().position(|&k| k == key) {
			self.keys[other] = old;
		}
		self.keys[bind as usize] = key;
	}
}

/// What the game is showing. `selected` is the highlighted line; `paused`
/// says whether the options were opened from the pause menu (true) or the
/// main menu (false), which is where Back returns to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Screen {
	MainMenu { selected: usize },
	Playing,
	Paused { selected: usize },
	Options { selected: usize, paused: bool },
	TextOptions { selected: usize, paused: bool },
	/// `waiting`: the highlighted action will take the next key pressed
	KeyBinds { selected: usize, paused: bool, waiting: bool },
}

/// What a menu asks the game to do; everything else the menus handle themselves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Request {
	NewGame,
	Quit,
}

/// What picking a line does.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Item {
	NewGame,
	Resume,
	MainMenu,
	Quit,
	Options,
	TextOptions,
	KeyBinds,
	TextScale,
	TextFilter,
	Rebind(Bind),
	ResetKeys,
	Back,
}

impl Screen {
	fn items(&self) -> Vec<Item> {
		match self {
			Screen::MainMenu { .. } => vec![Item::NewGame, Item::Options, Item::Quit],
			Screen::Paused { .. } => vec![Item::Resume, Item::Options, Item::MainMenu, Item::Quit],
			Screen::Options { .. } => vec![Item::TextOptions, Item::KeyBinds, Item::Back],
			Screen::TextOptions { .. } => vec![Item::TextScale, Item::TextFilter, Item::Back],
			Screen::KeyBinds { .. } => {
				let mut items: Vec<Item> = Bind::ALL.iter().map(|&b| Item::Rebind(b)).collect();
				items.extend([Item::ResetKeys, Item::Back]);
				items
			}
			Screen::Playing => vec![],
		}
	}

	fn selected_mut(&mut self) -> Option<&mut usize> {
		match self {
			Screen::MainMenu { selected }
			| Screen::Paused { selected }
			| Screen::Options { selected, .. }
			| Screen::TextOptions { selected, .. }
			| Screen::KeyBinds { selected, .. } => Some(selected),
			Screen::Playing => None,
		}
	}

	fn selected(&self) -> usize {
		self.clone().selected_mut().map_or(0, |s| *s)
	}

	/// Moves the highlight up (-1) or down (+1), wrapping around.
	fn move_selection(&mut self, step: i32) {
		let len = self.items().len() as i32;
		if let Some(selected) = self.selected_mut() {
			*selected = (*selected as i32 + step).rem_euclid(len) as usize;
		}
	}

	/// Where Back (or Esc) goes from here.
	fn back(&self) -> Screen {
		match *self {
			Screen::Paused { .. } => Screen::Playing,
			Screen::Options { paused: true, .. } => Screen::Paused { selected: 1 },
			Screen::Options { paused: false, .. } => Screen::MainMenu { selected: 1 },
			Screen::TextOptions { paused, .. } => Screen::Options { selected: 0, paused },
			Screen::KeyBinds { paused, .. } => Screen::Options { selected: 1, paused },
			other => other,
		}
	}

	/// Whether the options were opened from the pause menu.
	fn paused(&self) -> bool {
		matches!(self, Screen::Paused { .. } | Screen::Options { paused: true, .. })
	}

	/// Handles one frame of keyboard input while a menu is open.
	pub(super) fn handle(&mut self, settings: &mut Settings, keys: &ButtonInput<KeyCode>) -> Option<Request> {
		// Rebinding: the next key pressed is the new one; Esc cancels... unless
		// it's the key being bound to Pause
		if let Screen::KeyBinds { selected, waiting: waiting @ true, .. } = self {
			let Some(&key) = keys.get_just_pressed().next() else { return None };
			if key != KeyCode::Escape || Bind::ALL[*selected] == Bind::Pause {
				settings.rebind(Bind::ALL[*selected], key);
			}
			*waiting = false;
			return None;
		}

		let up = keys.any_just_pressed([KeyCode::KeyW, KeyCode::ArrowUp]);
		let down = keys.any_just_pressed([KeyCode::KeyS, KeyCode::ArrowDown]);
		let left = keys.any_just_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]);
		let right = keys.any_just_pressed([KeyCode::KeyD, KeyCode::ArrowRight]);
		let pick = keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space]);
		let back = keys.just_pressed(KeyCode::Escape);

		if up {
			self.move_selection(-1);
		}
		if down {
			self.move_selection(1);
		}
		if back && !matches!(self, Screen::MainMenu { .. }) {
			*self = self.back();
			return None;
		}

		let item = self.items()[self.selected()];
		// Values change with left/right, and Enter steps them forward too
		let step = if left { -1 } else if right || pick { 1 } else { 0 };
		match item {
			Item::TextScale if step != 0 => {
				settings.text_scale =
					(settings.text_scale as i32 + step).clamp(TEXT_SCALE_MIN as i32, TEXT_SCALE_MAX as i32) as u32;
				return None;
			}
			Item::TextFilter if step != 0 => {
				settings.text_filter = !settings.text_filter;
				return None;
			}
			_ => {}
		}
		if !pick {
			return None;
		}

		let paused = self.paused();
		match item {
			Item::NewGame => return Some(Request::NewGame),
			Item::Quit => return Some(Request::Quit),
			Item::Resume => *self = Screen::Playing,
			Item::MainMenu => *self = Screen::MainMenu { selected: 0 },
			Item::Options => *self = Screen::Options { selected: 0, paused },
			Item::TextOptions => *self = Screen::TextOptions { selected: 0, paused },
			Item::KeyBinds => *self = Screen::KeyBinds { selected: 0, paused, waiting: false },
			Item::Rebind(_) => {
				if let Screen::KeyBinds { waiting, .. } = self {
					*waiting = true;
				}
			}
			Item::ResetKeys => settings.keys = Bind::ALL.map(Bind::default_key),
			Item::Back => *self = self.back(),
			Item::TextScale | Item::TextFilter => {}
		}
		None
	}
}

/// A key's name as shown in the key binds menu: "W", "Space", "Up".
fn key_name(key: KeyCode) -> String {
	let name = format!("{key:?}");
	let name = name.strip_prefix("Key").or(name.strip_prefix("Digit")).unwrap_or(&name);
	name.strip_prefix("Arrow").unwrap_or(name).to_string()
}

/// One line of a menu, with its current value where it has one.
fn line(item: Item, screen: Screen, settings: &Settings) -> String {
	match item {
		Item::NewGame => "New Game".into(),
		Item::Resume => "Resume".into(),
		Item::MainMenu => "Main Menu".into(),
		Item::Quit => "Quit".into(),
		Item::Options => "Options".into(),
		Item::TextOptions => "Text".into(),
		Item::KeyBinds => "Key Binds".into(),
		Item::TextScale => format!("Text Size   < {}x >", settings.text_scale),
		Item::TextFilter => format!("Text Filter < {} >", if settings.text_filter { "On" } else { "Off" }),
		Item::Rebind(bind) => {
			let waiting = matches!(screen, Screen::KeyBinds { selected, waiting: true, .. } if Bind::ALL[selected] == bind);
			let key = if waiting { "press a key".into() } else { key_name(settings.key(bind)) };
			format!("{:<11}{:>11}", bind.label(), key)
		}
		Item::ResetKeys => "Reset to Defaults".into(),
		Item::Back => "Back".into(),
	}
}

const TITLE: &str = "RPG GAME";
const TEXT: [u8; 3] = [200, 200, 200];
const HIGHLIGHT: [u8; 3] = [255, 220, 90];
const BORDER: [u8; 3] = [110, 110, 120];
const HINT: [u8; 3] = [110, 110, 110];

/// Draws the open menu, if any, over the HUD. Menus opened from the main menu
/// cover the whole screen; the pause menu and its options are a box in the
/// middle with the game behind it.
pub(super) fn draw(screen: Screen, settings: &Settings, buffer: &mut Framebuffer) {
	if screen == Screen::Playing {
		return;
	}
	let selected = screen.selected();
	let lines: Vec<String> = screen.items().into_iter().map(|item| line(item, screen, settings)).collect();
	let (w, h) = (buffer.width as i32, buffer.height as i32);

	let title = match screen {
		Screen::MainMenu { .. } => TITLE,
		Screen::Paused { .. } => "PAUSED",
		Screen::Options { .. } => "OPTIONS",
		Screen::TextOptions { .. } => "TEXT",
		_ => "KEY BINDS",
	};
	let hint = match screen {
		Screen::KeyBinds { waiting: true, .. } => "Press the new key",
		Screen::TextOptions { .. } => "A/D change  Esc back",
		Screen::MainMenu { .. } => "W/S move  Enter select",
		_ => "Enter select  Esc back",
	};

	let (box_w, box_h) = if screen.paused() || matches!(screen, Screen::Paused { .. }) {
		let widest = lines.iter().chain([&title.to_string(), &hint.to_string()]).map(|l| l.chars().count()).max();
		(widest.unwrap_or(0) as i32 + 8, lines.len() as i32 + 8)
	} else {
		// Solid black over everything
		for y in 0..h {
			for x in 0..w {
				put(buffer, x, y, ' ', TEXT);
			}
		}
		(w, h)
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
	let top = y0 + box_h / 2 - lines.len() as i32 / 2;
	put_str(buffer, center(title), top - 3, title, HIGHLIGHT);
	for (i, text) in lines.iter().enumerate() {
		let text = if i == selected { format!("> {text} <") } else { text.clone() };
		let rgb = if i == selected { HIGHLIGHT } else { TEXT };
		put_str(buffer, center(&text), top + i as i32, &text, rgb);
	}
	put_str(buffer, center(hint), top + lines.len() as i32 + 1, hint, HINT);
}

#[cfg(test)]
mod tests {
	use super::*;

	fn press(screen: &mut Screen, settings: &mut Settings, key: KeyCode) -> Option<Request> {
		let mut keys = ButtonInput::default();
		keys.press(key);
		screen.handle(settings, &keys)
	}

	#[test]
	fn selection_wraps_and_back_returns_where_you_came_from() {
		let mut settings = Settings::default();
		let mut screen = Screen::Paused { selected: 0 };
		press(&mut screen, &mut settings, KeyCode::ArrowUp);
		assert_eq!(screen, Screen::Paused { selected: 3 });
		press(&mut screen, &mut settings, KeyCode::ArrowUp);
		press(&mut screen, &mut settings, KeyCode::ArrowUp);
		press(&mut screen, &mut settings, KeyCode::Enter);
		assert_eq!(screen, Screen::Options { selected: 0, paused: true });
		press(&mut screen, &mut settings, KeyCode::Enter);
		assert_eq!(screen, Screen::TextOptions { selected: 0, paused: true });
		press(&mut screen, &mut settings, KeyCode::Escape);
		press(&mut screen, &mut settings, KeyCode::Escape);
		assert_eq!(screen, Screen::Paused { selected: 1 });
		assert_eq!(press(&mut Screen::MainMenu { selected: 0 }, &mut settings, KeyCode::Enter), Some(Request::NewGame));
	}

	#[test]
	fn text_options_change_and_clamp() {
		let mut settings = Settings::default();
		let mut screen = Screen::TextOptions { selected: 0, paused: false };
		for _ in 0..10 {
			press(&mut screen, &mut settings, KeyCode::ArrowRight);
		}
		assert_eq!(settings.text_scale, TEXT_SCALE_MAX);
		press(&mut screen, &mut settings, KeyCode::ArrowLeft);
		assert_eq!(settings.text_scale, TEXT_SCALE_MAX - 1);
		press(&mut screen, &mut settings, KeyCode::ArrowDown);
		press(&mut screen, &mut settings, KeyCode::Enter);
		assert!(!settings.text_filter);
	}

	#[test]
	fn rebinding_takes_the_next_key_and_swaps_duplicates() {
		let mut settings = Settings::default();
		let mut screen = Screen::KeyBinds { selected: 0, paused: false, waiting: false };
		press(&mut screen, &mut settings, KeyCode::Enter);
		assert!(matches!(screen, Screen::KeyBinds { waiting: true, .. }));
		// D was Turn Right; it moves to Forward, and Turn Right gets W
		press(&mut screen, &mut settings, KeyCode::KeyD);
		assert_eq!(settings.key(Bind::Forward), KeyCode::KeyD);
		assert_eq!(settings.key(Bind::TurnRight), KeyCode::KeyW);
		// Esc cancels a rebind
		press(&mut screen, &mut settings, KeyCode::Enter);
		press(&mut screen, &mut settings, KeyCode::Escape);
		assert_eq!(settings.key(Bind::Forward), KeyCode::KeyD);
	}

	#[test]
	fn main_menu_covers_the_screen_and_pause_menu_does_not() {
		let settings = Settings::default();
		let mut fb = Framebuffer::new(80, 30);
		draw(Screen::MainMenu { selected: 0 }, &settings, &mut fb);
		assert!(fb.color_buffer.iter().all(|c| c.ui));
		let text: String = fb.color_buffer.iter().map(|c| c.ch).collect();
		assert!(text.contains("> New Game <") && text.contains(TITLE));

		let mut fb = Framebuffer::new(80, 30);
		draw(Screen::Paused { selected: 2 }, &settings, &mut fb);
		assert!(!fb.color_buffer[0].ui);
		let text: String = fb.color_buffer.iter().map(|c| c.ch).collect();
		assert!(text.contains("> Main Menu <") && text.contains("PAUSED"));
	}
}
