// Every control in the game, in one place: the actions players can rebind
// (with their defaults), the fixed keys for menus and toggles, the level
// editor's keys, and the help text that lists them all. Code that reads
// input uses these names instead of naming keys itself.
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;

// ---------------------------------------------------------------------------
// Rebindable actions
// ---------------------------------------------------------------------------

/// Something the player can bind a key to, in Options -> Key Binds.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Bind {
	Forward,
	Back,
	/// Turns in combat; steps sideways when walking freely
	TurnLeft,
	TurnRight,
	/// Hold to walk faster (outside combat)
	Sprint,
	Pause,
	Map,
	Inventory,
	/// Swing your weapon (the left mouse button always does too)
	Attack,
}

impl Bind {
	// Keys are stored in ALL's order and looked up by `bind as usize`, so
	// ALL must list the variants in declaration order (see the test below)
	pub(crate) const ALL: [Bind; 9] = [
		Bind::Forward,
		Bind::Back,
		Bind::TurnLeft,
		Bind::TurnRight,
		Bind::Sprint,
		Bind::Pause,
		Bind::Map,
		Bind::Inventory,
		Bind::Attack,
	];

	pub(crate) fn label(self) -> &'static str {
		match self {
			Bind::Forward => "Forward",
			Bind::Back => "Back",
			Bind::TurnLeft => "Left",
			Bind::TurnRight => "Right",
			Bind::Pause => "Pause",
			Bind::Map => "Map",
			Bind::Inventory => "Inventory",
			Bind::Attack => "Attack",
			Bind::Sprint => "Sprint",
		}
	}

	pub(crate) fn default_key(self) -> KeyCode {
		match self {
			Bind::Forward => KeyCode::KeyW,
			Bind::Back => KeyCode::KeyS,
			Bind::TurnLeft => KeyCode::KeyA,
			Bind::TurnRight => KeyCode::KeyD,
			Bind::Pause => KeyCode::Escape,
			Bind::Map => KeyCode::KeyM,
			Bind::Inventory => KeyCode::KeyE,
			Bind::Attack => KeyCode::KeyF,
			Bind::Sprint => KeyCode::ShiftLeft,
		}
	}

	/// The arrow key that always does this too, whatever it's bound to.
	pub(crate) fn arrow(self) -> Option<KeyCode> {
		match self {
			Bind::Forward => Some(KeyCode::ArrowUp),
			Bind::Back => Some(KeyCode::ArrowDown),
			Bind::TurnLeft => Some(KeyCode::ArrowLeft),
			Bind::TurnRight => Some(KeyCode::ArrowRight),
			_ => None,
		}
	}
}

// ---------------------------------------------------------------------------
// Fixed keys
// ---------------------------------------------------------------------------

/// Moving through menus and lists.
pub(crate) const MENU_UP: [KeyCode; 2] = [KeyCode::KeyW, KeyCode::ArrowUp];
pub(crate) const MENU_DOWN: [KeyCode; 2] = [KeyCode::KeyS, KeyCode::ArrowDown];
pub(crate) const MENU_LEFT: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];
pub(crate) const MENU_RIGHT: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];
pub(crate) const MENU_SELECT: [KeyCode; 3] = [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space];
pub(crate) const MENU_BACK: KeyCode = KeyCode::Escape;

/// Switches between the ASCII and textured views, anywhere.
pub(crate) const TOGGLE_ASCII: KeyCode = KeyCode::F2;
/// Debug binary: fog of war and darkness on or off.
pub(crate) const TOGGLE_FOG: KeyCode = KeyCode::F3;

/// The level editor (debug binary only).
pub(crate) mod editor {
	use super::{KeyCode, MouseButton};

	pub(crate) const TOGGLE: KeyCode = KeyCode::F1;
	pub(crate) const LEAVE: KeyCode = KeyCode::Escape;
	pub(crate) const FORWARD: KeyCode = KeyCode::KeyW;
	pub(crate) const BACK: KeyCode = KeyCode::KeyS;
	pub(crate) const LEFT: KeyCode = KeyCode::KeyA;
	pub(crate) const RIGHT: KeyCode = KeyCode::KeyD;
	pub(crate) const UP: KeyCode = KeyCode::Space;
	pub(crate) const DOWN: KeyCode = KeyCode::ControlLeft;
	pub(crate) const FAST: KeyCode = KeyCode::ShiftLeft;
	pub(crate) const NEXT_MODEL: KeyCode = KeyCode::KeyE;
	pub(crate) const PREVIOUS_MODEL: KeyCode = KeyCode::KeyQ;
	pub(crate) const ROTATE: KeyCode = KeyCode::KeyR;
	pub(crate) const BROWSER: KeyCode = KeyCode::KeyB;
	pub(crate) const RAISE: KeyCode = KeyCode::KeyX;
	pub(crate) const LOWER: KeyCode = KeyCode::KeyZ;
	pub(crate) const PLACE: (MouseButton, KeyCode) = (MouseButton::Left, KeyCode::Enter);
	pub(crate) const REMOVE: (MouseButton, KeyCode) = (MouseButton::Right, KeyCode::Backspace);
	pub(crate) const CLEAR: KeyCode = KeyCode::Delete;
	pub(crate) const SAVE: KeyCode = KeyCode::F5;
	pub(crate) const LOAD: KeyCode = KeyCode::F9;
}

// ---------------------------------------------------------------------------
// Names and help
// ---------------------------------------------------------------------------

/// One line of the help screen: a heading (no keys) or keys and what they do.
pub(crate) enum HelpLine {
	Heading(&'static str),
	Control(String, &'static str),
}

/// A key's name as shown to the player: "W", "Space", "Up", "F2".
pub(crate) fn key_name(key: KeyCode) -> String {
	let short = match key {
		KeyCode::ShiftLeft => "L Shift",
		KeyCode::ShiftRight => "R Shift",
		KeyCode::ControlLeft => "L Ctrl",
		KeyCode::ControlRight => "R Ctrl",
		KeyCode::AltLeft => "L Alt",
		KeyCode::AltRight => "R Alt",
		KeyCode::Escape => "Esc",
		KeyCode::Backspace => "Bksp",
		KeyCode::NumpadEnter => "Num Enter",
		_ => "",
	};
	if !short.is_empty() {
		return short.to_string();
	}
	let name = format!("{key:?}");
	let name = name.strip_prefix("Key").or(name.strip_prefix("Digit")).unwrap_or(&name);
	name.strip_prefix("Arrow").unwrap_or(name).to_string()
}

/// Every control, grouped, with the player's current key binds filled in.
pub(crate) fn help(key: impl Fn(Bind) -> KeyCode) -> Vec<HelpLine> {
	use HelpLine::{Control, Heading};
	let bound = |b: Bind| match b.arrow() {
		Some(arrow) => format!("{} / {}", key_name(key(b)), key_name(arrow)),
		None => key_name(key(b)),
	};
	let mut lines = vec![
		Heading("Exploring"),
		Control(format!("{} {}", bound(Bind::Forward), bound(Bind::Back)), "walk forward / back"),
		Control(format!("{} {}", bound(Bind::TurnLeft), bound(Bind::TurnRight)), "step sideways"),
		Control(key_name(key(Bind::Sprint)), "hold to sprint"),
		Control("Mouse".into(), "look around"),
		Control(format!("Click / {}", key_name(key(Bind::Attack))), "swing: hits what's in front, opens chests"),
		Control(key_name(key(Bind::Map)), "map: mini / full / off"),
		Control(key_name(key(Bind::Inventory)), "inventory"),
		Control(key_name(key(Bind::Pause)), "pause"),
		Control(key_name(TOGGLE_ASCII), "ASCII / textured view"),
		Heading("Combat (an enemy is near)"),
		Control(format!("{} {}", bound(Bind::Forward), bound(Bind::Back)), "step one tile; into an enemy attacks"),
		Control(format!("{} {}", bound(Bind::TurnLeft), bound(Bind::TurnRight)), "turn"),
		Heading("Menus"),
		Control("W S / Up Down".into(), "move"),
		Control("A D / Left Right".into(), "change a value"),
		Control("Enter / Space".into(), "select"),
		Control(key_name(MENU_BACK), "back"),
	];
	if crate::game::dev_tools() {
		lines.extend([
			Heading("Level editor (debug binary)"),
			Control(key_name(TOGGLE_FOG), "fog and darkness on / off"),
			Control(key_name(editor::TOGGLE), "open / close"),
			Control("WASD Space Ctrl".into(), "fly (Shift: faster)"),
			Control("Q E / R (Shift: 90°)".into(), "model / turn 15°"),
			Control("Click / Right click".into(), "place / remove"),
			Control(key_name(editor::BROWSER), "level browser"),
			Control(format!("{} {}", key_name(editor::SAVE), key_name(editor::LOAD)), "save / reload level"),
		]);
	}
	lines
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn all_lists_binds_in_declaration_order() {
		for (i, bind) in Bind::ALL.iter().enumerate() {
			assert_eq!(*bind as usize, i, "{bind:?} is out of place in Bind::ALL");
		}
	}
}
