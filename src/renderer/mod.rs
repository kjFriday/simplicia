mod crawler;
mod font;
mod menu;
mod mesh;
mod minimap;
pub mod models;
mod window;

use crawler::{Action, Arrival, World};
use font::GlyphSheet;

pub use window::RendererPlugin;

use bevy::ecs::resource::Resource;
use glam::{Mat4, Vec2, Vec3};

#[derive(Clone, Copy)]
struct Vertex {
	pos: Vec3,
	/// RGB, 0.0 to 1.0. Multiplies the texture when there is one.
	color: Vec3,
	/// Texture coordinates; meaningless without a texture
	uv: Vec2,
	/// Index into the mesh's `textures`, or `NO_TEXTURE`
	tex: u32,
}

const NO_TEXTURE: u32 = u32::MAX;

/// An RGBA8 image a mesh is painted with.
struct Texture {
	width: u32,
	height: u32,
	rgba: Vec<u8>,
}

/// One character cell of the HUD: the character and its RGB color.
#[derive(Clone, Copy, PartialEq)]
struct Cell {
	ch: char,
	rgb: [u8; 3],
	/// HUD text like the minimap, drawn on solid black. Cells without it are
	/// see-through, showing the text-filtered 3D view.
	ui: bool,
}

const EMPTY_CELL: Cell = Cell { ch: ' ', rgb: [0, 0, 0], ui: false };

struct Mesh {
	vertices: Vec<Vertex>,
	indices: Vec<[usize; 3]>,
	textures: Vec<Texture>,
}

/// The HUD's character grid, laid over the 3D view.
struct Framebuffer {
	width: usize,
	height: usize,
	color_buffer: Vec<Cell>,
}

impl Framebuffer {
	fn new(width: usize, height: usize) -> Self {
		Self { width, height, color_buffer: vec![EMPTY_CELL; width * height] }
	}

	fn clear(&mut self) {
		self.color_buffer.fill(EMPTY_CELL);
	}
}

/// Holds everything that must survive between frames: the levels, the HUD's
/// character grid, and the font that turns it into pixels. Lives in the Bevy
/// world as a resource; `RendererPlugin` drives it.
#[derive(Resource)]
pub struct Renderer {
	/// Every level generated so far, by depth; going back up finds them as you left them
	levels: Vec<World>,
	/// Index into `levels` of the level the player is on
	depth: usize,
	/// Goes up by one every new game, so the 3D scene knows to rebuild even
	/// when the depth is the same
	run: u32,
	light_dir: Vec3,
	/// Main menu, playing, or paused
	screen: menu::Screen,
	framebuffer: Framebuffer,
	glyphs: GlyphSheet,
}

impl Renderer {
	pub fn new() -> Result<Self, String> {
		//needs base ground plane from hex on round world grid.
		let first_level = World::generate(&mut rand::rng(), 0)?;
		Ok(Self {
			levels: vec![first_level],
			depth: 0,
			run: 0,
			//give sun angle
			light_dir: Vec3::new(0.5, -1.0, -0.5).normalize(),
			screen: menu::Screen::MainMenu { selected: 0 },
			framebuffer: Framebuffer::new(0, 0),
			glyphs: GlyphSheet::load()?,
		})
	}

	/// Throws away every level and starts over on a fresh top level.
	fn new_game(&mut self) -> Result<(), String> {
		self.levels = vec![World::generate(&mut rand::rng(), 0)?];
		self.depth = 0;
		self.run += 1;
		Ok(())
	}

	/// Turns the view by mouse movement, in radians. Doesn't take a turn.
	fn look(&mut self, yaw: f32, pitch: f32) {
		self.levels[self.depth].player.look(yaw, pitch);
	}

	/// One action, one turn. Stepping onto stairs changes level: down generates
	/// the next level the first time, up returns to the one above.
	fn act(&mut self, action: Action) -> Result<(), String> {
		match self.levels[self.depth].act(action) {
			Some(Arrival::StairsDown) => {
				self.depth += 1;
				if self.depth == self.levels.len() {
					let level = World::generate(&mut rand::rng(), self.depth)?;
					self.levels.push(level);
				} else {
					// Back on a level you've been to: arrive on its up staircase
					let level = &mut self.levels[self.depth];
					let up = level.stairs_up.expect("levels below the top have an up staircase");
					level.place_player(up);
				}
			}
			Some(Arrival::StairsUp) => {
				self.depth -= 1;
				let level = &mut self.levels[self.depth];
				let down = level.stairs_down;
				level.place_player(down);
			}
			Some(Arrival::Floor) | None => {}
		}
		Ok(())
	}

	/// Draws the HUD into a `cols` x `rows` character grid. The 3D view itself
	/// is rendered by Bevy and turned into text by the filter shader.
	fn render(&mut self, cols: usize, rows: usize) {
		// Reallocate only when the window is resized
		if cols != self.framebuffer.width || rows != self.framebuffer.height {
			self.framebuffer = Framebuffer::new(cols, rows);
		}
		self.framebuffer.clear();
		if !matches!(self.screen, menu::Screen::MainMenu { .. }) {
			minimap::draw(&self.levels[self.depth], &mut self.framebuffer);
		}
		menu::draw(self.screen, &mut self.framebuffer);
	}
}

/// One model placed in the world.
struct SceneObject {
	/// Index into `World::meshes`
	mesh: usize,
	/// Model space to world space
	transform: Mat4,
}

const NEAR_PLANE: f32 = 0.5;
const FAR_PLANE: f32 = 1000.0;
