// The menus (main, pause, options, text options, key binds, help) drawn as
// HUD text, plus the settings they change. Their keys are in controls.rs.
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;

use super::controls::{self, help, key_name, Bind, HelpLine};
use super::items::{Hero, ItemKind};
use crate::engine::hud::{put, put_str};
use crate::engine::hud::Framebuffer;

/// How the map is shown.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum MapView {
	/// A panel in the top-right corner, around the player
	Mini,
	/// The whole level, filling the screen
	Full,
	Hidden,
}

impl MapView {
	pub(super) fn next(self) -> Self {
		match self {
			MapView::Mini => MapView::Full,
			MapView::Full => MapView::Hidden,
			MapView::Hidden => MapView::Mini,
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
	/// ASCII mode: the 3D view drawn as text; off shows it textured, as it
	/// renders. F2 toggles it anywhere.
	pub(super) text_filter: bool,
	/// One key per action, in `Bind::ALL` order
	keys: [KeyCode; Bind::ALL.len()],
	/// How the map is shown; the Map key cycles it
	pub(super) map: MapView,
}

impl Default for Settings {
	fn default() -> Self {
		Self { text_scale: 4, text_filter: false, keys: Bind::ALL.map(Bind::default_key), map: MapView::Mini }
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
	/// The bag, over the game; `selected` indexes `Hero::bag`
	Inventory { selected: usize },
	Options { selected: usize, paused: bool },
	TextOptions { selected: usize, paused: bool },
	/// `waiting`: the highlighted action will take the next key pressed
	KeyBinds { selected: usize, paused: bool, waiting: bool },
	/// Every control, listed
	Help { paused: bool },
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
	Help,
	TextScale,
	TextFilter,
	Rebind(Bind),
	ResetKeys,
	Back,
}

impl Screen {
	fn items(&self) -> Vec<Item> {
		match self {
			Screen::MainMenu { .. } => vec![Item::NewGame, Item::Options, Item::Help, Item::Quit],
			Screen::Paused { .. } => vec![Item::Resume, Item::Options, Item::Help, Item::MainMenu, Item::Quit],
			Screen::Help { .. } => vec![Item::Back],
			Screen::Options { .. } => vec![Item::TextOptions, Item::KeyBinds, Item::Back],
			Screen::TextOptions { .. } => vec![Item::TextScale, Item::TextFilter, Item::Back],
			Screen::KeyBinds { .. } => {
				let mut items: Vec<Item> = Bind::ALL.iter().map(|&b| Item::Rebind(b)).collect();
				items.extend([Item::ResetKeys, Item::Back]);
				items
			}
			Screen::Playing | Screen::Inventory { .. } => vec![],
		}
	}

	fn selected_mut(&mut self) -> Option<&mut usize> {
		match self {
			Screen::MainMenu { selected }
			| Screen::Paused { selected }
			| Screen::Options { selected, .. }
			| Screen::TextOptions { selected, .. }
			| Screen::KeyBinds { selected, .. } => Some(selected),
			Screen::Help { .. } => None,
			Screen::Playing | Screen::Inventory { .. } => None,
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
			Screen::Help { paused: true } => Screen::Paused { selected: 2 },
			Screen::Help { paused: false } => Screen::MainMenu { selected: 2 },
			other => other,
		}
	}

	/// Whether the options were opened from the pause menu.
	fn paused(&self) -> bool {
		matches!(self, Screen::Paused { .. } | Screen::Options { paused: true, .. } | Screen::Help { paused: true })
	}

	/// Handles one frame of keyboard input while a menu is open.
	pub(super) fn handle(&mut self, settings: &mut Settings, keys: &ButtonInput<KeyCode>) -> Option<Request> {
		// Rebinding: the next key pressed is the new one; Esc cancels... unless
		// it's the key being bound to Pause
		if let Screen::KeyBinds { selected, waiting: waiting @ true, .. } = self {
			let Some(&key) = keys.get_just_pressed().next() else { return None };
			if key != controls::MENU_BACK || Bind::ALL[*selected] == Bind::Pause {
				settings.rebind(Bind::ALL[*selected], key);
			}
			*waiting = false;
			return None;
		}

		let up = keys.any_just_pressed(controls::MENU_UP);
		let down = keys.any_just_pressed(controls::MENU_DOWN);
		let left = keys.any_just_pressed(controls::MENU_LEFT);
		let right = keys.any_just_pressed(controls::MENU_RIGHT);
		let pick = keys.any_just_pressed(controls::MENU_SELECT);
		let back = keys.just_pressed(controls::MENU_BACK);

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
			Item::Help => *self = Screen::Help { paused },
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
		Item::Help => "Help".into(),
		Item::TextScale => format!("Text Size   < {}x >", settings.text_scale),
		Item::TextFilter => format!("ASCII Mode  < {} >", if settings.text_filter { "On" } else { "Off" }),
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
	match screen {
		Screen::Playing | Screen::Inventory { .. } => return,
		Screen::Help { .. } => return draw_help(settings, buffer),
		_ => {}
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
		Screen::TextOptions { .. } => "A/D change  F2 ASCII  Esc back",
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

/// The help screen: every control, grouped, over solid black, from
/// `controls::help` so it always shows the current key binds.
fn draw_help(settings: &Settings, buffer: &mut Framebuffer) {
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	for y in 0..h {
		for x in 0..w {
			put(buffer, x, y, ' ', TEXT);
		}
	}
	let lines = help(|bind| settings.key(bind));
	let keys_width = lines
		.iter()
		.filter_map(|l| if let HelpLine::Control(keys, _) = l { Some(keys.chars().count()) } else { None })
		.max()
		.unwrap_or(0) as i32;
	let text_width = lines
		.iter()
		.map(|l| match l {
			HelpLine::Heading(t) => t.chars().count() as i32,
			HelpLine::Control(_, what) => keys_width + 2 + what.chars().count() as i32,
		})
		.max()
		.unwrap_or(0);
	// Centered, title on top and Back at the bottom; long lists start at the top
	let x0 = ((w - text_width) / 2).max(1);
	let mut y = ((h - lines.len() as i32 - 4) / 2).max(0);
	put_str(buffer, (w - 4) / 2, y, "HELP", HIGHLIGHT);
	y += 2;
	for line in &lines {
		match line {
			HelpLine::Heading(t) => put_str(buffer, x0, y, t, HIGHLIGHT),
			HelpLine::Control(keys, what) => {
				put_str(buffer, x0, y, keys, TEXT);
				put_str(buffer, x0 + keys_width + 2, y, what, HINT);
			}
		}
		y += 1;
	}
	let back = format!("> Back <   ({} / Enter)", key_name(controls::MENU_BACK));
	put_str(buffer, (w - back.chars().count() as i32) / 2, (y + 1).min(h - 1), &back, HIGHLIGHT);
}

/// The inventory: a box listing what the hero carries, with equipped gear
/// marked. Enter uses or equips the highlighted item.
pub(super) fn draw_inventory(hero: &Hero, selected: usize, buffer: &mut Framebuffer) {
	let mut lines: Vec<String> = hero
		.bag
		.iter()
		.map(|&kind| {
			let worn = match kind {
				ItemKind::Sword => hero.sword,
				ItemKind::Shield => hero.shield,
				_ => false,
			};
			if worn { format!("{} (equipped)", kind.name()) } else { kind.name().to_string() }
		})
		.collect();
	if lines.is_empty() {
		lines.push("(empty)".into());
	}
	let title = format!("INVENTORY  Gold {}", hero.gold);
	let hint = "Enter use/equip  Esc close";
	let widest = lines.iter().map(|l| l.chars().count() + 4).chain([title.len(), hint.len()]).max().unwrap_or(0);
	let (box_w, box_h) = (widest as i32 + 6, lines.len() as i32 + 7);
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	let (x0, y0) = ((w - box_w) / 2, (h - box_h) / 2);
	for y in y0..y0 + box_h {
		for x in x0..x0 + box_w {
			let edge = x == x0 || x == x0 + box_w - 1 || y == y0 || y == y0 + box_h - 1;
			put(buffer, x, y, if edge { '█' } else { ' ' }, BORDER);
		}
	}
	let center = |text: &str| x0 + (box_w - text.chars().count() as i32) / 2;
	put_str(buffer, center(&title), y0 + 2, &title, HIGHLIGHT);
	for (i, text) in lines.iter().enumerate() {
		let chosen = i == selected && !hero.bag.is_empty();
		let text = if chosen { format!("> {text} <") } else { text.clone() };
		put_str(buffer, center(&text), y0 + 4 + i as i32, &text, if chosen { HIGHLIGHT } else { TEXT });
	}
	put_str(buffer, center(hint), y0 + box_h - 2, hint, HINT);
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
		assert_eq!(screen, Screen::Paused { selected: 4 });
		for _ in 0..3 {
			press(&mut screen, &mut settings, KeyCode::ArrowUp);
		}
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
		let before = settings.text_filter;
		press(&mut screen, &mut settings, KeyCode::Enter);
		assert_eq!(settings.text_filter, !before);
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
		draw(Screen::Paused { selected: 3 }, &settings, &mut fb);
		assert!(!fb.color_buffer[0].ui);
		let text: String = fb.color_buffer.iter().map(|c| c.ch).collect();
		assert!(text.contains("> Main Menu <") && text.contains("PAUSED"));
	}
}
