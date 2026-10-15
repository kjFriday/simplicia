// The map: a top-right panel showing the part of the level you've uncovered,
// or the whole level filling the screen; plus the HP and log readout. Drawn
// over the 3D view after it has been rendered.
use super::combat::PLAYER_HP;
use super::world::World;
use crate::engine::hud::{put, put_str, Framebuffer};

/// Map tiles shown across and down, at most. Odd, so the player sits in the middle.
const VIEW_TILES_W: i32 = 31;
const VIEW_TILES_H: i32 = 21;
/// Columns per map tile. The font's cells are square, so one is enough;
/// use 2 with a font whose cells are twice as tall as wide.
const CELLS_PER_TILE: i32 = 1;

const BORDER: [u8; 3] = [110, 110, 120];
const WALL: [u8; 3] = [150, 135, 110];
const FLOOR: [u8; 3] = [80, 80, 80];
const STAIRS_DOWN: [u8; 3] = [255, 200, 60];
const STAIRS_UP: [u8; 3] = [90, 200, 255];
const PLAYER: [u8; 3] = [90, 255, 120];
const TEXT: [u8; 3] = [220, 220, 220];
const ENEMY: [u8; 3] = [255, 80, 70];
const HURT: [u8; 3] = [255, 120, 90];
const LOG: [u8; 3] = [170, 170, 170];

/// Player arrow by facing: north, east, south, west.
const PLAYER_ARROWS: [char; 4] = ['▲', '►', '▼', '◄'];


/// Draws the map. The minimap is a panel in the top-right corner that shrinks
/// on small screens and is skipped if it wouldn't leave room to see the
/// dungeon; full shows as much of the level as fits, centered.
/// With `reveal_all` (fog off) the whole level shows, enemies and all.
pub(super) fn draw(world: &World, buffer: &mut Framebuffer, full: bool, reveal_all: bool) {
	let (max_w, max_h, want_w, want_h) = if full {
		((buffer.width as i32 - 2) / CELLS_PER_TILE, buffer.height as i32 - 2, i32::MAX, i32::MAX)
	} else {
		// Never take more than half the screen in either direction
		((buffer.width as i32 / 2 - 2) / CELLS_PER_TILE, buffer.height as i32 / 2 - 2, VIEW_TILES_W, VIEW_TILES_H)
	};
	let tiles_w = want_w.min(max_w).min(world.map.x as i32);
	let tiles_h = want_h.min(max_h).min(world.map.y as i32);
	if tiles_w < 5 || tiles_h < 3 {
		return;
	}

	// Window onto the map, centered on the player but kept inside the map
	let (px, py) = (world.player.x, world.player.y);
	let left = (px - tiles_w / 2).clamp(0, world.map.x as i32 - tiles_w);
	let top = (py - tiles_h / 2).clamp(0, world.map.y as i32 - tiles_h);

	// Panel position: flush with the top-right corner, border included
	let panel_w = tiles_w * CELLS_PER_TILE + 2;
	let panel_h = tiles_h + 2;
	let (x0, y0) = if full {
		((buffer.width as i32 - panel_w) / 2, (buffer.height as i32 - panel_h) / 2)
	} else {
		(buffer.width as i32 - panel_w, 0)
	};

	// Border, with the depth written into the top edge
	for x in 1..panel_w - 1 {
		put(buffer, x0 + x, y0, '─', BORDER);
		put(buffer, x0 + x, y0 + panel_h - 1, '─', BORDER);
	}
	for y in 1..panel_h - 1 {
		put(buffer, x0, y0 + y, '│', BORDER);
		put(buffer, x0 + panel_w - 1, y0 + y, '│', BORDER);
	}
	put(buffer, x0, y0, '┌', BORDER);
	put(buffer, x0 + panel_w - 1, y0, '┐', BORDER);
	put(buffer, x0, y0 + panel_h - 1, '└', BORDER);
	put(buffer, x0 + panel_w - 1, y0 + panel_h - 1, '┘', BORDER);
	put_str(buffer, x0 + 2, y0, &format!(" Depth {} ", world.depth + 1), TEXT);

	// Tiles. Unseen ones are left blank, which also blanks the 3D view behind them
	for ty in 0..tiles_h {
		for tx in 0..tiles_w {
			let (mx, my) = (left + tx, top + ty);
			let (glyph, rgb) = if (mx, my) == (px, py) {
				(PLAYER_ARROWS[world.player.facing()], PLAYER)
			} else if !world.has_seen(mx, my, reveal_all) {
				(' ', FLOOR)
			} else if let Some(e) = world.enemies.iter().find(|e| (e.x, e.y) == (mx, my) && world.can_see(mx, my, reveal_all)) {
				// Monsters first, so one standing on loot still shows
				(e.kind.stats().glyph, ENEMY)
			} else if let Some(c) = world.chests.iter().find(|c| (c.x, c.y) == (mx, my)) {
				(if c.open { 'u' } else { 'C' }, STAIRS_DOWN)
			} else if let Some(it) = world.items.iter().find(|it| (it.x, it.y) == (mx, my)) {
				(it.kind.glyph(), STAIRS_DOWN)
			} else if (mx, my, world.player.z) == world.stairs_down {
				('>', STAIRS_DOWN)
			} else if Some((mx, my, world.player.z)) == world.stairs_up {
				('<', STAIRS_UP)
			} else if world.map.is_wall(mx, my) {
				('#', WALL)
			} else {
				('.', FLOOR)
			};
			let cx = x0 + 1 + tx * CELLS_PER_TILE;
			let cy = y0 + 1 + ty;
			put(buffer, cx, cy, glyph, rgb);
			// With wide tiles, walls fill both columns so they read as solid
			for extra in 1..CELLS_PER_TILE {
				put(buffer, cx + extra, cy, if glyph == '#' { '#' } else { ' ' }, rgb);
			}
		}
	}
}

/// HP, combat, and the last few log lines, in the bottom-left corner.
pub(super) fn draw_status(world: &World, buffer: &mut Framebuffer) {
	let bottom = buffer.height as i32 - 1;
	let hero = &world.hero;
	let hp = format!("HP {}/{}  Gold {}", hero.hp.max(0), PLAYER_HP, hero.gold);
	let rgb = if hero.hp * 3 <= PLAYER_HP { HURT } else { TEXT };
	put_str(buffer, 0, bottom, &hp, rgb);
	if world.combat {
		put_str(buffer, hp.chars().count() as i32 + 2, bottom, "COMBAT", ENEMY);
	}
	for (i, line) in world.log.iter().rev().enumerate() {
		put_str(buffer, 0, bottom - 1 - i as i32, line, LOG);
	}
}