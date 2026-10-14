// The minimap: a top-right panel showing the part of the level you've
// uncovered, drawn over the 3D view after it has been rendered.
use super::*;

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

/// Player arrow by facing: north, east, south, west.
const PLAYER_ARROWS: [char; 4] = ['▲', '►', '▼', '◄'];

/// Writes one HUD cell; anything off the grid is ignored.
pub(super) fn put(buffer: &mut Framebuffer, x: i32, y: i32, ch: char, rgb: [u8; 3]) {
	if x >= 0 && y >= 0 && (x as usize) < buffer.width && (y as usize) < buffer.height {
		buffer.color_buffer[y as usize * buffer.width + x as usize] = Cell { ch, rgb, ui: true };
	}
}

pub(super) fn put_str(buffer: &mut Framebuffer, x: i32, y: i32, text: &str, rgb: [u8; 3]) {
	for (i, ch) in text.chars().enumerate() {
		put(buffer, x + i as i32, y, ch, rgb);
	}
}

/// Draws the panel into the top-right corner. Shrinks on small terminals and
/// is skipped entirely if it wouldn't leave room to see the dungeon.
pub(super) fn draw(world: &World, buffer: &mut Framebuffer) {
	// Never take more than half the screen in either direction
	let max_w = (buffer.width as i32 / 2 - 2) / CELLS_PER_TILE;
	let max_h = buffer.height as i32 / 2 - 2;
	let tiles_w = VIEW_TILES_W.min(max_w).min(world.map.x as i32);
	let tiles_h = VIEW_TILES_H.min(max_h).min(world.map.y as i32);
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
	let x0 = buffer.width as i32 - panel_w;
	let y0 = 0;

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
			} else if !world.has_seen(mx, my) {
				(' ', FLOOR)
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
