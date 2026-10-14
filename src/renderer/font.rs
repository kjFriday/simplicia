// The "Dungeon Mode" 8x8 bitmap font (assets/dungeonmode), laid out as a
// 16 x 16 grid of glyphs in IBM code page 437 order. The sheet is built into
// the executable, like the models.
use super::{Cell, Framebuffer};
#[cfg(test)]
use super::EMPTY_CELL;

const SHEET_PNG: &[u8] = include_bytes!("../../assets/dungeonmode/dungeonmode/bitmap/dungeon-437.png");

/// Glyph size in pixels.
pub(super) const GLYPH_W: usize = 8;
pub(super) const GLYPH_H: usize = 8;

/// How much brighter the text over the 3D view is than the surface it was
/// drawn from. 1.0 would make it vanish; higher stands out more.
const TEXT_TINT: f32 = 1.25;

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
pub(super) fn slot(ch: char) -> usize {
	match ch {
		' '..='~' => ch as usize,
		// Look-alikes the rest of the code uses
		'▶' => 16,
		'◀' => 17,
		_ => CP437.iter().position(|&c| c == ch).unwrap_or('?' as usize),
	}
}

pub(super) struct GlyphSheet {
	/// How much each pixel of each glyph is lit, 0 to 255; [slot][row][column]
	coverage: Vec<[[u8; GLYPH_W]; GLYPH_H]>,
}

impl GlyphSheet {
	pub(super) fn load() -> Result<Self, String> {
		let sheet = image::load_from_memory(SHEET_PNG).map_err(|e| format!("font sheet: {e}"))?.to_rgba8();
		let (w, h) = (sheet.width() as usize, sheet.height() as usize);
		if w != 16 * GLYPH_W || h != 16 * GLYPH_H {
			return Err(format!("font sheet is {w}x{h}, expected {}x{}", 16 * GLYPH_W, 16 * GLYPH_H));
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
		Ok(Self { coverage })
	}

	/// Draws the whole framebuffer into `rgba`, an image `buffer.width * GLYPH_W`
	/// pixels wide. HUD cells are drawn solid. Every other cell is the glyph on
	/// black, cross-faded with the 3D view under it by the cell's lighting: the
	/// alpha is 255 - light, so lit surfaces show the 3D render and shadows show
	/// the text. Together the two always add up to a fully opaque picture.
	pub(super) fn draw(&self, buffer: &Framebuffer, rgba: &mut [u8]) {
		let row_bytes = buffer.width * GLYPH_W * 4;
		for cy in 0..buffer.height {
			for cx in 0..buffer.width {
				let Cell { ch, rgb, ui, light } = buffer.color_buffer[cy * buffer.width + cx];
				let (rgb, alpha) = if ui {
					(rgb.map(|c| c as f32), 255)
				} else {
					(rgb.map(|c| (c as f32 * TEXT_TINT).min(255.0)), 255 - light)
				};
				let glyph = &self.coverage[slot(ch)];
				for (row, line) in glyph.iter().enumerate() {
					let start = (cy * GLYPH_H + row) * row_bytes + cx * GLYPH_W * 4;
					let pixels = &mut rgba[start..start + GLYPH_W * 4];
					for (px, &lit) in pixels.chunks_exact_mut(4).zip(line) {
						let lit = lit as f32 / 255.0;
						px[0] = (rgb[0] * lit) as u8;
						px[1] = (rgb[1] * lit) as u8;
						px[2] = (rgb[2] * lit) as u8;
						px[3] = alpha;
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
	fn draw_hud_solid_and_3d_text_faded_by_light() {
		let sheet = GlyphSheet::load().unwrap();
		let mut fb = Framebuffer::new(4, 1);
		fb.color_buffer[0] = Cell { ch: '█', rgb: [200, 100, 50], ui: true, light: 0 };
		fb.color_buffer[1] = Cell { ch: ' ', rgb: [0, 0, 0], ui: true, light: 0 };
		fb.color_buffer[2] = Cell { ch: '█', rgb: [100, 100, 100], ui: false, light: 55 };
		fb.color_buffer[3] = EMPTY_CELL;
		let mut rgba = vec![0u8; 4 * GLYPH_W * GLYPH_H * 4];
		sheet.draw(&fb, &mut rgba);
		let px = |cell: usize| &rgba[cell * GLYPH_W * 4..cell * GLYPH_W * 4 + 4];
		assert_eq!(px(0), &[200, 100, 50, 255]);
		// A HUD space is solid black
		assert_eq!(px(1), &[0, 0, 0, 255]);
		// Over the 3D view: slightly brighter, and dim, so mostly text
		assert_eq!(px(2), &[125, 125, 125, 200]);
		// Nothing drawn: all 3D
		assert_eq!(px(3)[3], 0);
	}
}
