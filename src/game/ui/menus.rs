// Drawing the menus (main, pause, options, text options, key binds, help, the
// level generator) and the inventory as HUD text. Which screen is open and
// what its lines are come from screen.rs; the values shown come from
// settings.rs. The game calls `draw` every frame after the map and status.
use super::controls::{self, help, key_name, Bind, HelpLine};
use super::screen::{Item, Screen};
use super::settings::Settings;
use crate::engine::hud::{put, put_str};
use crate::engine::hud::Framebuffer;
use crate::game::levels::presets::GenSettings;
use crate::game::play::items::{Hero, ItemKind};

pub(super) const TITLE: &str = "RPG GAME";
const TEXT: [u8; 3] = [200, 200, 200];
const HIGHLIGHT: [u8; 3] = [255, 220, 90];
const BORDER: [u8; 3] = [110, 110, 120];
const HINT: [u8; 3] = [110, 110, 110];
const NOTICE: [u8; 3] = [255, 120, 90];

/// Draws the open menu, if any, over the HUD. Menus opened from the main menu
/// cover the whole screen; the pause menu and its options are a box in the
/// middle with the game behind it.
pub(crate) fn draw(screen: Screen, settings: &Settings, buffer: &mut Framebuffer) {
	match screen {
		Screen::Playing | Screen::Inventory { .. } => return,
		Screen::Help { .. } => return draw_help(settings, buffer),
		Screen::Generator { selected } => return draw_generator(settings, selected, buffer),
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

	let (box_w, box_h) = if screen.paused() {
		let widest = lines
			.iter()
			.chain([&title.to_string(), &hint.to_string()])
			.map(|l| l.chars().count())
			.max();
		(widest.unwrap_or(0) as i32 + 8, lines.len() as i32 + 8)
	} else {
		// Solid black over everything
		fill_black(buffer);
		(w, h)
	};

	// Box, centered, with its border and a blank inside
	let (x0, y0) = ((w - box_w) / 2, (h - box_h) / 2);
	draw_border(buffer, x0, y0, box_w, box_h);

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

/// A line near the bottom of the main menu, like how the last run ended.
pub(crate) fn draw_notice(text: &str, buffer: &mut Framebuffer) {
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	put_str(buffer, (w - text.chars().count() as i32) / 2, h - 3, text, NOTICE);
}

/// The inventory: a box listing what the hero carries, with equipped gear
/// marked. Enter uses or equips the highlighted item.
pub(crate) fn draw_inventory(hero: &Hero, selected: usize, buffer: &mut Framebuffer) {
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
			let waiting = matches!(
				screen,
				Screen::KeyBinds { selected, waiting: true, .. } if Bind::ALL[selected] == bind
			);
			let key = if waiting { "press a key".into() } else { key_name(settings.key(bind)) };
			format!("{:<11}{:>11}", bind.label(), key)
		}
		Item::ResetKeys => "Reset to Defaults".into(),
		Item::Generator => "Level Generator".into(),
		Item::Slider(i) => format!("{:<11}< {:>3} >", GenSettings::SLIDERS[i].label, settings.generator.get(i)),
		Item::Reroll => format!("New Seed ({})", settings.generator_seed % 100_000),
		Item::SavePreset => "Save as Level".into(),
		Item::PlayGenerated => "Play It".into(),
		Item::Back => "Back".into(),
	}
}

/// The Generator screen: the sliders down the left, and on the right the
/// map they make from the current seed, rooms and halls shaded apart.
fn draw_generator(settings: &Settings, selected: usize, buffer: &mut Framebuffer) {
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	fill_black(buffer);
	let screen = Screen::Generator { selected };
	put_str(buffer, 1, 0, "LEVEL GENERATOR", HIGHLIGHT);
	let items = screen.items();
	for (i, item) in items.iter().enumerate() {
		let text = line(*item, screen, settings);
		let (text, rgb) = if i == selected { (format!("> {text}"), HIGHLIGHT) } else { (format!("  {text}"), TEXT) };
		put_str(buffer, 1, 2 + i as i32, &text, rgb);
	}
	put_str(buffer, 1, 3 + items.len() as i32, "A/D change  Enter pick  Esc back", HINT);

	// The preview, one character per map tile, scaled down to fit
	let (map, rooms) = settings.generator.layout(settings.generator_seed);
	let left = 30;
	let (room_w, room_h) = ((w - left - 1).max(1), (h - 1).max(1));
	let step = ((map.x as i32 + room_w - 1) / room_w).max((map.y as i32 + room_h - 1) / room_h).max(1);
	let in_room = |x: i32, y: i32| rooms.iter().any(|r| x >= r.x1 && x < r.x2 && y >= r.y1 && y < r.y2);
	for y in (0..map.y as i32).step_by(step as usize) {
		for x in (0..map.x as i32).step_by(step as usize) {
			let (ch, rgb) = if map.is_wall(x, y) {
				('#', [90, 80, 70])
			} else if in_room(x, y) {
				('.', [200, 200, 200])
			} else {
				(',', [120, 160, 200])
			};
			put(buffer, left + x / step, 1 + y / step, ch, rgb);
		}
	}
	put_str(buffer, left, 0, &format!("{} rooms  ('.' room, ',' hall)", rooms.len()), HINT);
}

/// The help screen: every control, grouped, over solid black, from
/// `controls::help` so it always shows the current key binds.
fn draw_help(settings: &Settings, buffer: &mut Framebuffer) {
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	fill_black(buffer);
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

/// Covers the whole screen in blanks, hiding the game behind.
fn fill_black(buffer: &mut Framebuffer) {
	let (w, h) = (buffer.width as i32, buffer.height as i32);
	for y in 0..h {
		for x in 0..w {
			put(buffer, x, y, ' ', TEXT);
		}
	}
}

/// A `box_w` x `box_h` box with its top-left corner at (`x0`, `y0`): a line
/// border and a blank inside.
fn draw_border(buffer: &mut Framebuffer, x0: i32, y0: i32, box_w: i32, box_h: i32) {
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
}
