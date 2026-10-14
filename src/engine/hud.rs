// The HUD: a grid of colored characters drawn over the 3D view with the
// bitmap font. Cells marked `ui` sit on solid black; the rest are see-through.

/// One character cell: the character and its RGB color.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Cell {
	pub(crate) ch: char,
	pub(crate) rgb: [u8; 3],
	/// HUD text like the minimap, drawn on solid black. Cells without it are
	/// see-through, showing the 3D view.
	pub(crate) ui: bool,
}

/// A see-through cell with nothing in it.
pub(crate) const EMPTY_CELL: Cell = Cell { ch: ' ', rgb: [0, 0, 0], ui: false };

/// The HUD's character grid, `width` x `height` cells, row by row.
pub(crate) struct Framebuffer {
	pub(crate) width: usize,
	pub(crate) height: usize,
	pub(crate) color_buffer: Vec<Cell>,
}

impl Framebuffer {
	pub(crate) fn new(width: usize, height: usize) -> Self {
		Self { width, height, color_buffer: vec![EMPTY_CELL; width * height] }
	}

	pub(crate) fn clear(&mut self) {
		self.color_buffer.fill(EMPTY_CELL);
	}
}

/// Writes one HUD cell; anything off the grid is ignored.
pub(crate) fn put(buffer: &mut Framebuffer, x: i32, y: i32, ch: char, rgb: [u8; 3]) {
	if x >= 0 && y >= 0 && (x as usize) < buffer.width && (y as usize) < buffer.height {
		buffer.color_buffer[y as usize * buffer.width + x as usize] = Cell { ch, rgb, ui: true };
	}
}

pub(crate) fn put_str(buffer: &mut Framebuffer, x: i32, y: i32, text: &str, rgb: [u8; 3]) {
	for (i, ch) in text.chars().enumerate() {
		put(buffer, x + i as i32, y, ch, rgb);
	}
}
