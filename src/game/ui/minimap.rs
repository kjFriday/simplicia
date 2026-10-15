// The map: a top-right panel showing the part of the level you've uncovered,
// or the whole level filling the screen; plus the HP and log readout. Drawn
// over the 3D view after it has been rendered.
use crate::engine::font::{glyph, picture};
use crate::engine::hud::{put, put_str, Framebuffer};
use crate::game::play::combat::{EnemyKind, PLAYER_HP};
use crate::game::play::items::ItemKind;
use crate::game::play::world::World;

/// Map tiles shown across and down, at most. Odd, so the player sits in the middle.
const VIEW_TILES_W: i32 = 31;
const VIEW_TILES_H: i32 = 21;
/// Columns per map tile. The font's cells are square, so one is enough;
/// use 2 with a font whose cells are twice as tall as wide.
const CELLS_PER_TILE: i32 = 1;

const BORDER:      [u8; 3] = [110, 110, 120];
const WALL:        [u8; 3] = [150, 135, 110];
const FLOOR:       [u8; 3] = [80, 80, 80];
const STAIRS_DOWN: [u8; 3] = [255, 200, 60];
const STAIRS_UP:   [u8; 3] = [90, 200, 255];
const PLAYER:      [u8; 3] = [90, 255, 120];
const TEXT:        [u8; 3] = [220, 220, 220];
const ENEMY:       [u8; 3] = [255, 80, 70];
const HURT:        [u8; 3] = [255, 120, 90];
const LOG:         [u8; 3] = [170, 170, 170];

/// Player arrow by facing: north, east, south, west.
const PLAYER_ARROWS: [char; 4] =
	[glyph::TRIANGLE_UP, glyph::TRIANGLE_RIGHT, glyph::TRIANGLE_DOWN, glyph::TRIANGLE_LEFT];

// Map tiles, from the font's picture sheet
const WALL_TILE: char = glyph::SHADE_MEDIUM;
const FLOOR_TILE: char = glyph::SMALL_DOT;
const STAIRS_DOWN_TILE: char = glyph::STAIRS_DOWN;
const STAIRS_UP_TILE: char = glyph::STAIRS_UP;
const CHEST_TILE: char = glyph::CHEST;
const CHEST_OPEN_TILE: char = glyph::CHEST_OPEN;

// The panel's border, from the picture sheet's box drawing
const BORDER_H: char = glyph::WALL_H;
const BORDER_V: char = glyph::WALL_V;
const BORDER_TOP_LEFT: char = picture(0xA3);
const BORDER_TOP_RIGHT: char = picture(0xA5);
const BORDER_BOTTOM_LEFT: char = picture(0xC3);
const BORDER_BOTTOM_RIGHT: char = picture(0xC5);

/// Draws the map. The minimap is a panel in the top-right corner that shrinks
/// on small screens and is skipped if it wouldn't leave room to see the
/// dungeon; full shows as much of the level as fits, centered.
/// With `reveal_all` (fog off) the whole level shows, enemies and all.
pub(crate) fn draw(world: &World, buffer: &mut Framebuffer, full: bool, reveal_all: bool) {
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
	draw_border(world, buffer, x0, y0, panel_w, panel_h);

	// Tiles. Unseen ones are left blank, which also blanks the 3D view behind them
	for ty in 0..tiles_h {
		for tx in 0..tiles_w {
			let (mx, my) = (left + tx, top + ty);
			let (glyph, rgb) = tile_glyph(world, mx, my, reveal_all);
			let cx = x0 + 1 + tx * CELLS_PER_TILE;
			let cy = y0 + 1 + ty;
			put(buffer, cx, cy, glyph, rgb);
			// With wide tiles, walls fill both columns so they read as solid
			for extra in 1..CELLS_PER_TILE {
				put(buffer, cx + extra, cy, if glyph == WALL_TILE { WALL_TILE } else { ' ' }, rgb);
			}
		}
	}
}

/// HP, combat, and the last few log lines, in the bottom-left corner.
pub(crate) fn draw_status(world: &World, buffer: &mut Framebuffer) {
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

// ---------------------------------------------------------------------------
// Helpers for `draw`
// ---------------------------------------------------------------------------

/// The panel's border, with the depth written into the top edge.
fn draw_border(world: &World, buffer: &mut Framebuffer, x0: i32, y0: i32, panel_w: i32, panel_h: i32) {
	for x in 1..panel_w - 1 {
		put(buffer, x0 + x, y0, BORDER_H, BORDER);
		put(buffer, x0 + x, y0 + panel_h - 1, BORDER_H, BORDER);
	}
	for y in 1..panel_h - 1 {
		put(buffer, x0, y0 + y, BORDER_V, BORDER);
		put(buffer, x0 + panel_w - 1, y0 + y, BORDER_V, BORDER);
	}
	put(buffer, x0, y0, BORDER_TOP_LEFT, BORDER);
	put(buffer, x0 + panel_w - 1, y0, BORDER_TOP_RIGHT, BORDER);
	put(buffer, x0, y0 + panel_h - 1, BORDER_BOTTOM_LEFT, BORDER);
	put(buffer, x0 + panel_w - 1, y0 + panel_h - 1, BORDER_BOTTOM_RIGHT, BORDER);
	put_str(buffer, x0 + 2, y0, &format!(" Depth {} ", world.depth + 1), TEXT);
}

/// What map tile (`mx`, `my`) shows as, and in what color.
fn tile_glyph(world: &World, mx: i32, my: i32, reveal_all: bool) -> (char, [u8; 3]) {
	let (px, py) = (world.player.x, world.player.y);
	if (mx, my) == (px, py) {
		(PLAYER_ARROWS[world.player.facing()], PLAYER)
	} else if !world.has_seen(mx, my, reveal_all) {
		(' ', FLOOR)
	} else if let Some(e) =
		world.enemies.iter().find(|e| (e.x, e.y) == (mx, my) && world.can_see(mx, my, reveal_all))
	{
		// Monsters first, so one standing on loot still shows
		(enemy_tile(e.kind), ENEMY)
	} else if let Some(c) = world.chests.iter().find(|c| (c.x, c.y) == (mx, my)) {
		(if c.open { CHEST_OPEN_TILE } else { CHEST_TILE }, STAIRS_DOWN)
	} else if let Some(it) = world.items.iter().find(|it| (it.x, it.y) == (mx, my)) {
		(item_tile(it.kind), STAIRS_DOWN)
	} else if (mx, my, world.player.z) == world.stairs_down {
		(STAIRS_DOWN_TILE, STAIRS_DOWN)
	} else if Some((mx, my, world.player.z)) == world.stairs_up {
		(STAIRS_UP_TILE, STAIRS_UP)
	} else if world.map.is_wall(mx, my) {
		(WALL_TILE, WALL)
	} else {
		(FLOOR_TILE, FLOOR)
	}
}

/// A monster's picture on the map.
fn enemy_tile(kind: EnemyKind) -> char {
	match kind {
		EnemyKind::ThornVine => glyph::LEAF,
		EnemyKind::ShadowImp => glyph::IMP,
		EnemyKind::GreenSpider => glyph::SPIDER,
		// The sheet has no rat; the helmet looks enough like one
		EnemyKind::GreyRat => glyph::HELMET,
		EnemyKind::ShadowBlob => glyph::GHOST,
	}
}

/// An item's picture on the map.
fn item_tile(kind: ItemKind) -> char {
	match kind {
		ItemKind::Potion => glyph::POTION,
		ItemKind::Gold => glyph::COIN,
		ItemKind::Sword => glyph::SWORD,
		ItemKind::Shield => glyph::SHIELD,
	}
}
