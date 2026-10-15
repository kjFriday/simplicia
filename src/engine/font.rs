// The "Dungeon Mode" 8x8 bitmap font (assets/dungeonmode), in two sheets of
// 16 x 16 glyphs: one in IBM code page 437 order (text, box drawing), and
// the font's own sheet of pictures (walls, monsters, items), which `glyph`
// names. Both sheets are built into the executable, like the models.
use super::hud::{Cell, Framebuffer};

const SHEET_PNG: &[u8] = include_bytes!("../../assets/dungeonmode/dungeonmode/bitmap/dungeon-437.png");
const PICTURE_PNG: &[u8] = include_bytes!("../../assets/dungeonmode/dungeonmode/bitmap/dungeon-mode.png");

/// Where the picture sheet's glyphs sit in Unicode: the Private Use Area, so
/// they never clash with real characters. Picture glyph `i` is PICTURE_BASE + i.
const PICTURE_BASE: u32 = 0xE000;
/// Slots 0-255 are the code page 437 sheet; the picture sheet follows.
const PICTURE_SLOT: usize = 256;

/// Glyph size in pixels.
pub(crate) const GLYPH_W: usize = 8;
pub(crate) const GLYPH_H: usize = 8;

/// Unicode for each code page 437 slot, 0 to 255. Slot 0 is a blank.
const CP437: [char; 256] = [
	'\0', '☺', '☻', '♥', '♦', '♣', '♠', '•', '◘', '○', '◙', '♂', '♀', '♪', '♫', '☼',
	'►', '◄', '↕', '‼', '¶', '§', '▬', '↨', '↑', '↓', '→', '←', '∟', '↔', '▲', '▼',
	' ', '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/',
	'0', '1', '2', '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?',
	'@', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O',
	'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '[', '\\', ']', '^', '_',
	'`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o',
	'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '{', '|', '}', '~', '⌂',
	'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
	'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
	'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
	'░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
	'└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
	'╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
	'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
	'≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Glyph slot for a character; anything the font lacks shows as '?'.
pub(crate) fn slot(ch: char) -> usize {
	match ch {
		' '..='~' => ch as usize,
		// Look-alikes the rest of the code uses
		'▶' => 16,
		'◀' => 17,
		_ => match (ch as u32).checked_sub(PICTURE_BASE) {
			Some(i @ 0..256) => PICTURE_SLOT + i as usize,
			_ => CP437.iter().position(|&c| c == ch).unwrap_or('?' as usize),
		},
	}
}

/// Glyph `index` (0x00 to 0xFF, row then column) of the picture sheet, as a
/// character the HUD can draw.
pub(crate) const fn picture(index: u8) -> char {
	match char::from_u32(PICTURE_BASE + index as u32) {
		Some(ch) => ch,
		None => unreachable!(),
	}
}

/// The picture sheet's glyphs by name. Big pictures are several cells, listed
/// row by row. The sheet's text and box drawing (rows 2-7, and most of rows
/// A-F) repeat the code page 437 sheet, so they aren't named here; `picture`
/// reaches them.
#[allow(dead_code)]
pub(crate) mod glyph {
	use super::picture as p;

	// Rows 0-1: symbols
	pub(crate) const SMILEY: char = p(0x01);
	pub(crate) const DOT: char = p(0x02);
	pub(crate) const HEART: char = p(0x03);
	pub(crate) const DIAMOND: char = p(0x04);
	pub(crate) const CLUB: char = p(0x05);
	pub(crate) const SPADE: char = p(0x06);
	pub(crate) const SMALL_DOT: char = p(0x07);
	pub(crate) const MALE: char = p(0x08);
	pub(crate) const FEMALE: char = p(0x09);
	pub(crate) const DOTTED_BOX: char = p(0x0A);
	pub(crate) const ARROW_UP_LEFT: char = p(0x0B);
	pub(crate) const ARROW_RIGHT: char = p(0x0C);
	pub(crate) const ARROW_LEFT: char = p(0x0D);
	pub(crate) const ARROW_UP: char = p(0x0E);
	pub(crate) const ARROW_DOWN: char = p(0x0F);
	pub(crate) const FULL_BLOCK: char = p(0x10);
	pub(crate) const CROSS: char = p(0x11);
	pub(crate) const RING: char = p(0x12);
	pub(crate) const HEART_EMPTY: char = p(0x13);
	pub(crate) const DIAMOND_EMPTY: char = p(0x14);
	pub(crate) const MOUND: char = p(0x15);
	pub(crate) const LEAF: char = p(0x16);
	pub(crate) const ELLIPSIS: char = p(0x17);
	pub(crate) const NOTE: char = p(0x18);
	pub(crate) const NOTES: char = p(0x19);
	pub(crate) const DOTTED_CORNERS: char = p(0x1A);
	pub(crate) const POINTING_HAND: char = p(0x1B);
	pub(crate) const TRIANGLE_RIGHT: char = p(0x1C);
	pub(crate) const TRIANGLE_LEFT: char = p(0x1D);
	pub(crate) const TRIANGLE_UP: char = p(0x1E);
	pub(crate) const TRIANGLE_DOWN: char = p(0x1F);
	pub(crate) const TEN: char = p(0x3A);

	// Rows 8-9: walls, big pictures, buildings, gear
	pub(crate) const WALL_V: char = p(0x80);
	pub(crate) const WALL_H: char = p(0x81);
	pub(crate) const WALL_V_DOUBLE: char = p(0x90);
	pub(crate) const WALL_H_DOUBLE: char = p(0x91);
	/// Hatched block, 2 x 2: rubble or a grate
	pub(crate) const RUBBLE: [[char; 2]; 2] = [[p(0x82), p(0x83)], [p(0x92), p(0x93)]];
	/// Eye or diamond, 2 x 2: a rug or a pit
	pub(crate) const RUG: [[char; 2]; 2] = [[p(0x84), p(0x85)], [p(0x94), p(0x95)]];
	/// Big triangle, 2 x 2: a tent or spike
	pub(crate) const TENT: [[char; 2]; 2] = [[p(0x86), p(0x87)], [p(0x96), p(0x97)]];
	/// Mountain slopes, 2 x 2, and the long slope below them
	pub(crate) const MOUNTAIN: [[char; 2]; 2] = [[p(0x88), p(0x89)], [p(0x98), p(0x99)]];
	pub(crate) const SLOPE: [char; 2] = [p(0x9A), p(0x9B)];
	pub(crate) const HOUSE: char = p(0x8A);
	pub(crate) const BATTLEMENT: char = p(0x8B);
	pub(crate) const CRATE: char = p(0x8C);
	pub(crate) const POT: char = p(0x8D);
	pub(crate) const STAIRS_UP: char = p(0x8E);
	pub(crate) const STAIRS_DOWN: char = p(0x8F);
	pub(crate) const SWORD: char = p(0x9C);
	pub(crate) const SHIELD: char = p(0x9D);
	pub(crate) const ANKH: char = p(0x9E);
	pub(crate) const URN: char = p(0x9F);

	// Rows A-F, columns 0-2: blocks, shading, hatching, stripes
	pub(crate) const HALF_LOWER: char = p(0xA0);
	pub(crate) const HALF_RIGHT: char = p(0xA1);
	pub(crate) const BIG_X: char = p(0xA2);
	pub(crate) const SHADE_LIGHT: char = p(0xB0);
	pub(crate) const SHADE_MEDIUM: char = p(0xB1);
	pub(crate) const SHADE_DARK: char = p(0xB2);
	pub(crate) const STRIPES_H: char = p(0xF0);
	pub(crate) const STRIPES_V: char = p(0xF1);
	pub(crate) const LINES_H: char = p(0xF2);

	// Rows A-F, columns C-F: items and monsters
	pub(crate) const CHEST: char = p(0xAC);
	pub(crate) const CHEST_OPEN: char = p(0xAD);
	pub(crate) const COIN: char = p(0xAE);
	pub(crate) const KEY: char = p(0xAF);
	pub(crate) const STAR: char = p(0xBC);
	pub(crate) const ARCH: char = p(0xBD);
	pub(crate) const EYE: char = p(0xBE);
	pub(crate) const BONE: char = p(0xBF);
	pub(crate) const BAG: char = p(0xCC);
	pub(crate) const WATER: char = p(0xCD);
	pub(crate) const BAR: char = p(0xCE);
	pub(crate) const STEP: char = p(0xCF);
	pub(crate) const IMP: char = p(0xDC);
	pub(crate) const SKULL: char = p(0xDD);
	pub(crate) const SPIDER: char = p(0xDE);
	pub(crate) const GOBLIN: char = p(0xDF);
	pub(crate) const WIZARD: char = p(0xEC);
	pub(crate) const GHOST: char = p(0xED);
	pub(crate) const HELMET: char = p(0xEE);
	pub(crate) const POTION: char = p(0xEF);
	pub(crate) const GRASS: char = p(0xF9);
	pub(crate) const MUSHROOM: char = p(0xFA);
	pub(crate) const SPLAT: char = p(0xFB);
	/// Tiny marks, for small details like the minimap's
	pub(crate) const MARKS: [char; 4] = [p(0xFC), p(0xFD), p(0xFE), p(0xFF)];
}

/// Decodes a 16 x 16 glyph sheet into each glyph's ink, 0 to 255 per pixel.
fn decode(png: &[u8], name: &str) -> Result<Vec<[[u8; GLYPH_W]; GLYPH_H]>, String> {
	let sheet = image::load_from_memory(png).map_err(|e| format!("{name}: {e}"))?.to_rgba8();
	let (w, h) = (sheet.width() as usize, sheet.height() as usize);
	if w != 16 * GLYPH_W || h != 16 * GLYPH_H {
		return Err(format!("{name} is {w}x{h}, expected {}x{}", 16 * GLYPH_W, 16 * GLYPH_H));
	}

	let mut coverage = vec![[[0u8; GLYPH_W]; GLYPH_H]; 256];
	for (slot, glyph) in coverage.iter_mut().enumerate() {
		let (gx, gy) = ((slot % 16) * GLYPH_W, (slot / 16) * GLYPH_H);
		for (row, line) in glyph.iter_mut().enumerate() {
			for (col, px) in line.iter_mut().enumerate() {
				let [r, g, b, a] = sheet.get_pixel((gx + col) as u32, (gy + row) as u32).0;
				// Glyphs may be drawn light-on-dark or as alpha; brightness times alpha covers both
				let brightness = r.max(g).max(b) as u32;
				*px = (brightness * a as u32 / 255) as u8;
			}
		}
	}
	Ok(coverage)
}

/// The decoded font: how much ink each pixel of every glyph has.
pub(crate) struct GlyphSheet {
	/// How much each pixel of each glyph is lit, 0 to 255; [slot][row][column].
	/// The code page 437 sheet, then the picture sheet.
	coverage: Vec<[[u8; GLYPH_W]; GLYPH_H]>,
}

impl GlyphSheet {
	/// Decodes the built-in font sheets and measures each glyph's ink.
	pub(crate) fn load() -> Result<Self, String> {
		let mut coverage = decode(SHEET_PNG, "font sheet")?;
		coverage.extend(decode(PICTURE_PNG, "picture sheet")?);
		Ok(Self { coverage })
	}

	/// The code page 437 sheet for the GPU: 16 x 16 glyphs, white, with each
	/// pixel's ink as alpha. The text filter shader draws its characters from
	/// this; it doesn't use the pictures.
	pub(crate) fn atlas_rgba(&self) -> Vec<u8> {
		let (w, h) = (16 * GLYPH_W, 16 * GLYPH_H);
		let mut rgba = vec![0u8; w * h * 4];
		for (slot, glyph) in self.coverage[..PICTURE_SLOT].iter().enumerate() {
			let (gx, gy) = ((slot % 16) * GLYPH_W, (slot / 16) * GLYPH_H);
			for (row, line) in glyph.iter().enumerate() {
				for (col, &ink) in line.iter().enumerate() {
					let i = ((gy + row) * w + gx + col) * 4;
					rgba[i..i + 4].copy_from_slice(&[255, 255, 255, ink]);
				}
			}
		}
		rgba
	}

	/// Draws the HUD grid into `rgba`, an image `buffer.width * GLYPH_W` pixels
	/// wide. HUD cells are drawn on solid black; the rest are fully transparent
	/// so the text-filtered 3D view shows through.
	pub(crate) fn draw(&self, buffer: &Framebuffer, rgba: &mut [u8]) {
		let row_bytes = buffer.width * GLYPH_W * 4;
		for cy in 0..buffer.height {
			for cx in 0..buffer.width {
				let Cell { ch, rgb, ui } = buffer.color_buffer[cy * buffer.width + cx];
				let glyph = &self.coverage[slot(ch)];
				for (row, line) in glyph.iter().enumerate() {
					let start = (cy * GLYPH_H + row) * row_bytes + cx * GLYPH_W * 4;
					let pixels = &mut rgba[start..start + GLYPH_W * 4];
					for (px, &lit) in pixels.chunks_exact_mut(4).zip(line) {
						if ui {
							let lit = lit as u32;
							px[0] = (rgb[0] as u32 * lit / 255) as u8;
							px[1] = (rgb[1] as u32 * lit / 255) as u8;
							px[2] = (rgb[2] as u32 * lit / 255) as u8;
							px[3] = 255;
						} else {
							px.copy_from_slice(&[0, 0, 0, 0]);
						}
					}
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sheet_loads_and_letters_have_ink_but_space_does_not() {
		let sheet = GlyphSheet::load().unwrap();
		let ink = |ch: char| sheet.coverage[slot(ch)].iter().flatten().filter(|&&p| p > 127).count();
		assert_eq!(ink(' '), 0);
		for ch in ['A', '#', '@', '█', '▲', '►', '─', '┌'] {
			assert!(ink(ch) > 0, "{ch:?} has no ink");
		}
		assert_eq!(ink('█'), GLYPH_W * GLYPH_H);
	}

	#[test]
	fn every_character_the_game_draws_is_in_the_font() {
		let used = ".:-=+*#%@ ─│┌┐└┘▲▶▼◀►◄<>Depth0123456789";
		for ch in used.chars() {
			assert!(ch == '?' || slot(ch) != '?' as usize, "{ch:?} would show as ?");
		}
	}

	#[test]
	fn pictures_have_their_own_slots_and_ink() {
		let sheet = GlyphSheet::load().unwrap();
		assert_eq!(slot(picture(0)), PICTURE_SLOT);
		assert_eq!(slot(glyph::SKULL), PICTURE_SLOT + 0xDD);
		let ink = |ch: char| sheet.coverage[slot(ch)].iter().flatten().filter(|&&p| p > 127).count();
		assert_eq!(ink(picture(0)), 0);
		for ch in [glyph::SKULL, glyph::CHEST, glyph::SWORD, glyph::GHOST, glyph::WALL_V, glyph::TENT[1][0]] {
			assert!(ink(ch) > 0, "{:#x} has no ink", ch as u32 - PICTURE_BASE);
		}
		assert_eq!(ink(glyph::FULL_BLOCK), GLYPH_W * GLYPH_H);
	}

	#[test]
	fn hud_cells_are_solid_and_the_rest_see_through() {
		let sheet = GlyphSheet::load().unwrap();
		let mut fb = Framebuffer::new(3, 1);
		fb.color_buffer[0] = Cell { ch: '█', rgb: [200, 100, 50], ui: true };
		fb.color_buffer[1] = Cell { ch: ' ', rgb: [0, 0, 0], ui: true };
		let mut rgba = vec![0u8; 3 * GLYPH_W * GLYPH_H * 4];
		sheet.draw(&fb, &mut rgba);
		let px = |cell: usize| &rgba[cell * GLYPH_W * 4..cell * GLYPH_W * 4 + 4];
		assert_eq!(px(0), &[200, 100, 50, 255]);
		// A HUD space is solid black
		assert_eq!(px(1), &[0, 0, 0, 255]);
		// Not HUD: the filtered 3D view shows through
		assert_eq!(px(2)[3], 0);
	}

	#[test]
	fn atlas_has_full_block_ink_in_its_slot() {
		let atlas = GlyphSheet::load().unwrap().atlas_rgba();
		let s = slot('█');
		let i = ((s / 16) * GLYPH_H * 16 * GLYPH_W + (s % 16) * GLYPH_W) * 4;
		assert_eq!(&atlas[i..i + 4], &[255, 255, 255, 255]);
	}
}