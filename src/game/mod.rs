// The game: a first-person dungeon crawl. Levels are generated from the
// dungeon kit, explored freely, and fought through turn by turn. The engine
// (src/engine) does the loading and drawing; this module decides what's in
// the world and what the player can do.

/// Combat: enemies, damage, and the turn order once one is close.
mod combat;
/// Every key and mouse button, and the help text listing them.
mod controls;
/// The debug-build level editor (F1).
mod editor;
/// Items on the floor, the hero's bag, and using them.
mod items;
/// Laying out a level and building it from the dungeon kit.
mod level_gen;
/// The main, pause, options, key binds, and inventory menus.
mod menu;
/// The map panel and the HP/log readout.
mod minimap;
/// Glowing gateways over the staircases.
mod portals;
/// The player: position, view, and movement.
mod player;
/// Running the game in a Bevy window: input, the 3D scene, and drawing.
mod window;
/// A level and moving around it.
mod world;

#[cfg(test)]
mod tests;

use bevy::ecs::resource::Resource;
use glam::Vec3;

use crate::engine::font::GlyphSheet;
use crate::engine::hud::Framebuffer;
use player::Action;
use world::{Arrival, World};

pub use window::GamePlugin;

/// Holds everything that must survive between frames: the levels, the HUD's
/// character grid, and the font that turns it into pixels. Lives in the Bevy
/// world as a resource; `GamePlugin` drives it.
#[derive(Resource)]
pub struct Game {
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
	/// What the options menus set: text size, text filter, key binds
	settings: menu::Settings,
	framebuffer: Framebuffer,
	glyphs: GlyphSheet,
}

impl Game {
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
			settings: menu::Settings::default(),
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
		let arrival = self.levels[self.depth].act(action);
		self.arrive(arrival)
	}

	/// Walks freely for `dt` seconds (outside combat); stairs work the same as
	/// stepping onto them.
	fn walk(&mut self, forward: f32, strafe: f32, dt: f32) -> Result<(), String> {
		let arrival = self.levels[self.depth].walk(forward, strafe, dt);
		self.arrive(arrival)
	}

	/// After a move: changes level on stairs, and ends the run on death.
	fn arrive(&mut self, arrival: Option<Arrival>) -> Result<(), String> {
		if self.levels[self.depth].hero.hp <= 0 {
			self.screen = menu::Screen::MainMenu { selected: 0 };
			return Ok(());
		}
		// The hero goes along to whichever level the player ends up on
		let hero = std::mem::take(&mut self.levels[self.depth].hero);
		match arrival {
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
		self.levels[self.depth].hero = hero;
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
		// Menus opened from the main menu cover the whole screen anyway
		if matches!(self.screen, menu::Screen::Playing | menu::Screen::Paused { .. } | menu::Screen::Inventory { .. }) {
			let world = &self.levels[self.depth];
			minimap::draw_status(world, &mut self.framebuffer);
			match self.settings.map {
				menu::MapView::Mini => minimap::draw(world, &mut self.framebuffer, false),
				menu::MapView::Full => minimap::draw(world, &mut self.framebuffer, true),
				menu::MapView::Hidden => {}
			}
		}
		menu::draw(self.screen, &self.settings, &mut self.framebuffer);
		if let menu::Screen::Inventory { selected } = self.screen {
			menu::draw_inventory(&self.levels[self.depth].hero, selected, &mut self.framebuffer);
		}
	}
}
