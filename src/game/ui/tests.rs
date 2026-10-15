// Tests for the menus: moving through them, changing settings, rebinding
// keys, and what the main and pause menus cover when drawn.
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;

use super::controls::Bind;
use super::menus::{draw, TITLE};
use super::screen::{Request, Screen};
use super::settings::{Settings, TEXT_SCALE_MAX};
use crate::engine::hud::Framebuffer;

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

	// The menus the pause menu opens are boxes over the game too
	let mut fb = Framebuffer::new(80, 30);
	draw(Screen::TextOptions { selected: 0, paused: true }, &settings, &mut fb);
	assert!(!fb.color_buffer[0].ui);
}
