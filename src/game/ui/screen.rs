// Which screen the game is showing (a menu, the game, the inventory) and how
// the menus answer the keyboard: moving the highlight, changing values, and
// picking lines. What each menu looks like is drawn in menus.rs; the keys
// come from controls.rs, and the values changed live in settings.rs.
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;

use super::controls::{self, Bind};
use super::settings::{Settings, TEXT_SCALE_MAX, TEXT_SCALE_MIN};
use crate::game::levels::presets::GenSettings;

/// What the game is showing. `selected` is the highlighted line; `paused`
/// says whether the options were opened from the pause menu (true) or the
/// main menu (false), which is where Back returns to.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Screen {
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
	/// Debug builds: the level generator's sliders, with a preview
	Generator { selected: usize },
}

/// What a menu asks the game to do; everything else the menus handle themselves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Request {
	NewGame,
	Quit,
	/// Save the Generator screen's settings and seed as a level preset
	SavePreset,
	/// Start a game on the Generator screen's level
	PlayGenerated,
}

/// What picking a line does.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Item {
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
	Generator,
	Slider(usize),
	Reroll,
	SavePreset,
	PlayGenerated,
	Back,
}

impl Screen {
	/// The lines this screen's menu lists, top to bottom.
	pub(super) fn items(&self) -> Vec<Item> {
		match self {
			Screen::MainMenu { .. } if crate::game::dev_tools() => {
				vec![Item::NewGame, Item::Options, Item::Help, Item::Generator, Item::Quit]
			}
			Screen::MainMenu { .. } => vec![Item::NewGame, Item::Options, Item::Help, Item::Quit],
			Screen::Generator { .. } => {
				let mut items: Vec<Item> = (0..GenSettings::SLIDERS.len()).map(Item::Slider).collect();
				items.extend([Item::Reroll, Item::SavePreset, Item::PlayGenerated, Item::Back]);
				items
			}
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

	/// The highlighted line; 0 on screens without one.
	pub(super) fn selected(&self) -> usize {
		self.clone().selected_mut().map_or(0, |s| *s)
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
			Screen::Generator { .. } => Screen::MainMenu { selected: 3 },
			other => other,
		}
	}

	/// Whether this is the pause menu, or a menu opened from it.
	pub(crate) fn paused(&self) -> bool {
		matches!(
			self,
			Screen::Paused { .. }
				| Screen::Options { paused: true, .. }
				| Screen::TextOptions { paused: true, .. }
				| Screen::KeyBinds { paused: true, .. }
				| Screen::Help { paused: true }
		)
	}

	/// Handles one frame of keyboard input while a menu is open.
	pub(crate) fn handle(&mut self, settings: &mut Settings, keys: &ButtonInput<KeyCode>) -> Option<Request> {
		// Rebinding: the next key pressed is the new one; Esc cancels... unless
		// it's the key being bound to Pause
		if self.take_rebind_key(settings, keys) {
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
		if change_value(item, step, left || right, settings) {
			return None;
		}
		if !pick {
			return None;
		}
		self.pick(item, settings)
	}

	// -----------------------------------------------------------------------
	// Helpers for `handle`
	// -----------------------------------------------------------------------

	fn selected_mut(&mut self) -> Option<&mut usize> {
		match self {
			Screen::MainMenu { selected }
			| Screen::Paused { selected }
			| Screen::Options { selected, .. }
			| Screen::TextOptions { selected, .. }
			| Screen::KeyBinds { selected, .. }
			| Screen::Generator { selected } => Some(selected),
			Screen::Help { .. } => None,
			Screen::Playing | Screen::Inventory { .. } => None,
		}
	}

	/// Moves the highlight up (-1) or down (+1), wrapping around.
	fn move_selection(&mut self, step: i32) {
		let len = self.items().len() as i32;
		if let Some(selected) = self.selected_mut() {
			*selected = (*selected as i32 + step).rem_euclid(len) as usize;
		}
	}

	/// While the Key Binds screen waits for a key, the first one pressed is
	/// bound to the highlighted action (Esc cancels, unless that action is
	/// Pause). True while it is waiting, so nothing else happens this frame.
	fn take_rebind_key(&mut self, settings: &mut Settings, keys: &ButtonInput<KeyCode>) -> bool {
		let Screen::KeyBinds { selected, waiting: waiting @ true, .. } = self else { return false };
		let Some(&key) = keys.get_just_pressed().next() else { return true };
		if key != controls::MENU_BACK || Bind::ALL[*selected] == Bind::Pause {
			settings.rebind(Bind::ALL[*selected], key);
		}
		*waiting = false;
		true
	}

	/// What picking `item` (Enter on it) does: opens another screen, changes a
	/// setting, or asks the game to do something.
	fn pick(&mut self, item: Item, settings: &mut Settings) -> Option<Request> {
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
			Item::Generator => *self = Screen::Generator { selected: 0 },
			Item::Reroll => settings.generator_seed = rand::random(),
			Item::SavePreset => return Some(Request::SavePreset),
			Item::PlayGenerated => return Some(Request::PlayGenerated),
			Item::Slider(_) => {}
			Item::Back => *self = self.back(),
			Item::TextScale | Item::TextFilter => {}
		}
		None
	}
}

/// Steps the highlighted value by `step` (-1, 0, or +1), if `item` has one.
/// Sliders only move with left/right (`sideways`), not Enter. True if `item`
/// took the key press, so there is nothing left to pick.
fn change_value(item: Item, step: i32, sideways: bool, settings: &mut Settings) -> bool {
	match item {
		Item::TextScale if step != 0 => {
			settings.text_scale =
				(settings.text_scale as i32 + step).clamp(TEXT_SCALE_MIN as i32, TEXT_SCALE_MAX as i32) as u32;
			true
		}
		Item::TextFilter if step != 0 => {
			settings.text_filter = !settings.text_filter;
			true
		}
		Item::Slider(i) if sideways => {
			settings.generator.nudge(i, step as i64);
			true
		}
		_ => false,
	}
}
