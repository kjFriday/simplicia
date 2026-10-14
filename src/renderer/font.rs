// The "Dungeon Mode" 8x8 bitmap font (assets/dungeonmode), laid out as a
// 16 x 16 grid of glyphs in IBM code page 437 order. The sheet is built into
// the executable, like the models.
use super::{Cell, Framebuffer};

const SHEET_PNG: &[u8] = include_bytes!("../../assets/dungeonmode/dungeonmode/bitmap/dungeon-437.png");

/// Glyph size in pixels.
pub(super) const GLYPH_W: usize = 8;
pub(super) const GLYPH_H: usize = 8;

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

	/// The glyph sheet for the GPU: 16 x 16 glyphs, white, with each pixel's ink
	/// as alpha. The text filter shader draws its characters from this.
	pub(super) fn atlas_rgba(&self) -> Vec<u8> {
		let (w, h) = (16 * GLYPH_W, 16 * GLYPH_H);
		let mut rgba = vec![0u8; w * h * 4];
		for (slot, glyph) in self.coverage.iter().enumerate() {
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
	pub(super) fn draw(&self, buffer: &Framebuffer, rgba: &mut [u8]) {
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