// Reading the keys and the mouse. `handle_input` runs first every frame and
// asks `Game` to act: in a menu the keys go to the screen (ui/screen.rs); in
// the inventory they pick and use items; while playing they pause, open the
// inventory or map, attack, or take a turn. `mouse_look` turns the view.
// Which key does what is set in ui/controls.rs and the options menu.
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::game::editor::Editor;
use crate::game::play::player::{Action, SPRINT_MULTIPLIER};
use crate::game::ui::controls::{self, Bind};
use crate::game::ui::screen::{Request, Screen};
use crate::game::Game;

/// Radians the view turns per pixel of mouse movement.
const MOUSE_SENSITIVITY: f32 = 0.003;

/// In a menu: W/S or the arrows move, Enter picks, A/D change values, Esc
/// goes back. Playing: one key press, one turn (holding a key does nothing
/// more), and the pause key pauses. The cursor is only locked while playing.
pub(super) fn handle_input(
	keys: Res<ButtonInput<KeyCode>>,
	buttons: Res<ButtonInput<MouseButton>>,
	time: Res<Time>,
	mut game: ResMut<Game>,
	mut exit: MessageWriter<AppExit>,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
	editor: Option<Res<Editor>>,
) {
	// F2 switches between ASCII and textured views, in menus, play or the editor
	// F3 (debug binary): fog on or off; the light and the map follow it
	if crate::game::dev_tools() && keys.just_pressed(controls::TOGGLE_FOG) {
		game.fog = !game.fog;
	}
	if keys.just_pressed(controls::TOGGLE_ASCII) {
		game.settings.text_filter = !game.settings.text_filter;
	}
	// The editor has the other keys while it's open
	if editor.is_some_and(|e| e.active) {
		return;
	}
	let game = &mut *game;
	let before = game.screen;
	if let Screen::Inventory { selected } = game.screen {
		inventory_keys(&keys, game, selected);
	} else if game.screen != Screen::Playing {
		if !menu_keys(&keys, game, &mut exit) {
			return;
		}
	} else if !playing_keys(&keys, &buttons, game, &mut exit, time.delta_secs()) {
		return;
	}

	// Lock the cursor for mouselook while playing; free it in menus
	if (before == Screen::Playing) != (game.screen == Screen::Playing) {
		set_cursor_locked(&mut cursors, game.screen == Screen::Playing);
	}
}

/// Locks and hides the cursor (for mouselook), or frees and shows it.
pub(super) fn set_cursor_locked(cursors: &mut Query<&mut CursorOptions, With<PrimaryWindow>>, locked: bool) {
	if let Ok(mut cursor) = cursors.single_mut() {
		cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
		cursor.visible = !locked;
	}
}

/// Turns the view by how far the mouse moved, while playing (not in a menu
/// or the editor).
pub(super) fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut game: ResMut<Game>, editor: Option<Res<Editor>>) {
	let editing = editor.is_some_and(|e| e.active);
	if motion.delta != Vec2::ZERO && game.screen == Screen::Playing && !editing {
		// Screen y grows downward, so moving the mouse up looks up
		game.look(motion.delta.x * MOUSE_SENSITIVITY, -motion.delta.y * MOUSE_SENSITIVITY);
	}
}

/// The inventory, with item `selected` picked: up and down move the pick,
/// select uses the item, and back (or the inventory key) closes it.
fn inventory_keys(keys: &ButtonInput<KeyCode>, game: &mut Game, selected: usize) {
	let world = &mut game.levels[game.depth];
	let count = world.hero.bag.len().max(1) as i32;
	let step = |s: usize, d: i32| (s as i32 + d).rem_euclid(count) as usize;
	if keys.any_just_pressed(controls::MENU_UP) {
		game.screen = Screen::Inventory { selected: step(selected, -1) };
	} else if keys.any_just_pressed(controls::MENU_DOWN) {
		game.screen = Screen::Inventory { selected: step(selected, 1) };
	} else if keys.any_just_pressed(controls::MENU_SELECT) {
		world.use_item(selected);
		let count = world.hero.bag.len().max(1);
		game.screen = Screen::Inventory { selected: selected.min(count - 1) };
	} else if keys.any_just_pressed([controls::MENU_BACK, game.settings.key(Bind::Inventory)]) {
		game.screen = Screen::Playing;
	}
	// A potion drunk in combat can be the last thing the player does
	game.check_death();
}

/// Any other menu: the screen handles the keys, and this does what it asks
/// for (start a game, save a preset, quit). False when the app is exiting,
/// so nothing more should happen this frame.
fn menu_keys(keys: &ButtonInput<KeyCode>, game: &mut Game, exit: &mut MessageWriter<AppExit>) -> bool {
	match game.screen.handle(&mut game.settings, keys) {
		Some(Request::NewGame) => {
			if let Err(e) = game.new_game() {
				eprintln!("error: {e}");
				exit.write(AppExit::error());
				return false;
			}
			game.screen = Screen::Playing;
		}
		Some(Request::SavePreset) => {
			let message = game.save_preset();
			eprintln!("{message}");
		}
		Some(Request::PlayGenerated) => {
			if let Err(e) = game.play_generated() {
				eprintln!("error: {e}");
				exit.write(AppExit::error());
				return false;
			}
			game.screen = Screen::Playing;
		}
		Some(Request::Quit) => {
			exit.write(AppExit::Success);
			return false;
		}
		None => {}
	}
	true
}

/// Playing: the pause, inventory and map keys; an attack (the left mouse
/// button or the attack key); or else a turn or a walk (`play_turn`). False
/// when the app is exiting, so nothing more should happen this frame.
fn playing_keys(
	keys: &ButtonInput<KeyCode>,
	buttons: &ButtonInput<MouseButton>,
	game: &mut Game,
	exit: &mut MessageWriter<AppExit>,
	dt: f32,
) -> bool {
	if keys.just_pressed(game.settings.key(Bind::Pause)) {
		game.screen = Screen::Paused { selected: 0 };
	} else if keys.just_pressed(game.settings.key(Bind::Inventory)) {
		game.screen = Screen::Inventory { selected: 0 };
	} else if keys.just_pressed(game.settings.key(Bind::Map)) {
		game.settings.map = game.settings.map.next();
	} else if buttons.just_pressed(MouseButton::Left) || keys.just_pressed(game.settings.key(Bind::Attack)) {
		if let Err(e) = game.attack() {
			eprintln!("error: {e}");
			exit.write(AppExit::error());
			return false;
		}
	} else {
		play_turn(keys, game, exit, dt);
	}
	true
}

/// Outside combat: walk freely while keys are held, with Left/Right
/// stepping sideways. In combat: one key press, one turn.
fn play_turn(keys: &ButtonInput<KeyCode>, game: &mut Game, exit: &mut MessageWriter<AppExit>, dt: f32) {
	// The bound key, plus the arrows, which always work
	let key = |bind| game.settings.key(bind);
	if !game.levels[game.depth].combat {
		let held = |bind: Bind| {
			if keys.any_pressed([Some(key(bind)), bind.arrow()].into_iter().flatten()) { 1.0 } else { 0.0 }
		};
		let forward = held(Bind::Forward) - held(Bind::Back);
		let strafe = held(Bind::TurnRight) - held(Bind::TurnLeft);
		if forward != 0.0 || strafe != 0.0 {
			let sprinting = keys.pressed(key(Bind::Sprint));
			let dt = if sprinting { dt * SPRINT_MULTIPLIER } else { dt };
			if let Err(e) = game.walk(forward, strafe, dt) {
				eprintln!("error: {e}");
				exit.write(AppExit::error());
			}
		}
		return;
	}
	let bindings = [
		(Bind::Forward, Action::Forward),
		(Bind::Back, Action::Back),
		(Bind::TurnLeft, Action::TurnLeft),
		(Bind::TurnRight, Action::TurnRight),
	];
	let pressed: Vec<Action> = bindings
		.into_iter()
		.filter(|&(bind, _)| keys.any_just_pressed([Some(key(bind)), bind.arrow()].into_iter().flatten()))
		.map(|(_, action)| action)
		.collect();
	for action in pressed {
		if let Err(e) = game.act(action) {
			eprintln!("error: {e}");
			exit.write(AppExit::error());
			return;
		}
	}
}
