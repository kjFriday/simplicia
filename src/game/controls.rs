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
pub(super) enum Bind {
	Forward,
	Back,
	/// Turns in combat; steps sideways when walking freely
	TurnLeft,
	TurnRight,
	Pause,
	Map,
	Inventory,
	/// Hold to walk faster (outside combat)
	Sprint,
}

impl Bind {
	pub(super) const ALL: [Bind; 8] = [
		Bind::Forward,
		Bind::Back,
		Bind::TurnLeft,
		Bind::TurnRight,
		Bind::Sprint,
		Bind::Pause,
		Bind::Map,
		Bind::Inventory,
	];

	pub(super) fn label(self) -> &'static str {
		match self {
			Bind::Forward => "Forward",
			Bind::Back => "Back",
			Bind::TurnLeft => "Left",
			Bind::TurnRight => "Right",
			Bind::Pause => "Pause",
			Bind::Map => "Map",
			Bind::Inventory => "Inventory",
			Bind::Sprint => "Sprint",
		}
	}

	pub(super) fn default_key(self) -> KeyCode {
		match self {
			Bind::Forward => KeyCode::KeyW,
			Bind::Back => KeyCode::KeyS,
			Bind::TurnLeft => KeyCode::KeyA,
			Bind::TurnRight => KeyCode::KeyD,
			Bind::Pause => KeyCode::Escape,
			Bind::Map => KeyCode::KeyM,
			Bind::Inventory => KeyCode::KeyI,
			Bind::Sprint => KeyCode::ShiftLeft,
		}
	}

	/// The arrow key that always does this too, whatever it's bound to.
	pub(super) fn arrow(self) -> Option<KeyCode> {
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
pub(super) const MENU_UP: [KeyCode; 2] = [KeyCode::KeyW, KeyCode::ArrowUp];
pub(super) const MENU_DOWN: [KeyCode; 2] = [KeyCode::KeyS, KeyCode::ArrowDown];
pub(super) const MENU_LEFT: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];
pub(super) const MENU_RIGHT: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];
pub(super) const MENU_SELECT: [KeyCode; 3] = [KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space];
pub(super) const MENU_BACK: KeyCode = KeyCode::Escape;

/// Switches between the ASCII and textured views, anywhere.
pub(super) const TOGGLE_ASCII: KeyCode = KeyCode::F2;

/// The level editor (debug builds only).
pub(super) mod editor {
	use super::{KeyCode, MouseButton};

	pub(in crate::game) const TOGGLE: KeyCode = KeyCode::F1;
	pub(in crate::game) const LEAVE: KeyCode = KeyCode::Escape;
	pub(in crate::game) const FORWARD: KeyCode = KeyCode::KeyW;
	pub(in crate::game) const BACK: KeyCode = KeyCode::KeyS;
	pub(in crate::game) const LEFT: KeyCode = KeyCode::KeyA;
	pub(in crate::game) const RIGHT: KeyCode = KeyCode::KeyD;
	pub(in crate::game) const UP: KeyCode = KeyCode::Space;
	pub(in crate::game) const DOWN: KeyCode = KeyCode::ControlLeft;
	pub(in crate::game) const FAST: KeyCode = KeyCode::ShiftLeft;
	pub(in crate::game) const NEXT_MODEL: KeyCode = KeyCode::KeyE;
	pub(in crate::game) const PREVIOUS_MODEL: KeyCode = KeyCode::KeyQ;
	pub(in crate::game) const ROTATE: KeyCode = KeyCode::KeyR;
	pub(in crate::game) const BROWSER: KeyCode = KeyCode::KeyB;
	pub(in crate::game) const RAISE: KeyCode = KeyCode::KeyX;
	pub(in crate::game) const LOWER: KeyCode = KeyCode::KeyZ;
	pub(in crate::game) const PLACE: (MouseButton, KeyCode) = (MouseButton::Left, KeyCode::Enter);
	pub(in crate::game) const REMOVE: (MouseButton, KeyCode) = (MouseButton::Right, KeyCode::Backspace);
	pub(in crate::game) const CLEAR: KeyCode = KeyCode::Delete;
	pub(in crate::game) const SAVE: KeyCode = KeyCode::F5;
	pub(in crate::game) const LOAD: KeyCode = KeyCode::F9;
}

// ---------------------------------------------------------------------------
// Names and help
// ---------------------------------------------------------------------------

/// A key's name as shown to the player: "W", "Space", "Up", "F2".
pub(super) fn key_name(key: KeyCode) -> String {
	let name = format!("{key:?}");
	let name = name.strip_prefix("Key").or(name.strip_prefix("Digit")).unwrap_or(&name);
	name.strip_prefix("Arrow").unwrap_or(name).to_string()
}

/// One line of the help screen: a heading (no keys) or keys and what they do.
pub(super) enum HelpLine {
	Heading(&'static str),
	Control(String, &'static str),
}

/// Every control, grouped, with the player's current key binds filled in.
pub(super) fn help(key: impl Fn(Bind) -> KeyCode) -> Vec<HelpLine> {
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
	if cfg!(debug_assertions) {
		lines.extend([
			Heading("Level editor (debug builds)"),
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
