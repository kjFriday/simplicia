// The game: a first-person dungeon crawl. Levels are generated from the
// dungeon kit, explored freely, and fought through turn by turn. The engine
// (src/engine) does the loading and drawing; this module decides what's in
// the world and what the player can do.

/// Every placeable asset, sorted by where it goes and what rooms it suits.
mod catalog;
/// Chests, and the loot that pops out of them.
mod chests;
/// Generating cells from the catalog at startup.
mod cellgen;
/// Cells: the furnished pieces rooms and halls are made of.
mod cells;
/// Combat: enemies, damage, and the turn order once one is close.
mod combat;
/// Every key and mouse button, and the help text listing them.
mod controls;
/// The level editor (F1), in the debug binary.
mod editor;
/// Items on the floor, the hero's bag, and using them.
mod items;
/// Laying out a level and building it from the dungeon kit.
mod level_gen;
/// The main, pause, options, key binds, and inventory menus.
mod menu;
/// The map panel and the HP/log readout.
mod minimap;
/// Generator settings and saved level presets.
mod presets;
/// Glowing gateways over the staircases.
mod portals;
/// Floor heights: raised and sunken rooms, stairs, ledges, and pits.
mod terrain;
/// Particles, the swinging weapon, and other things the view shows happening.
mod effects;
/// The player: position, view, and movement.
mod player;
/// Running the game in a Bevy window: input, the 3D scene, and drawing.
mod window;
/// A level and moving around it.
mod world;

#[cfg(test)]
mod tests;

use bevy::ecs::resource::Resource;

use crate::engine::font::GlyphSheet;
use crate::engine::icons::IconSheet;
use crate::engine::hud::Framebuffer;
use player::Action;
use world::{Arrival, World};

pub use window::GamePlugin;

/// True in the `debug` binary, which has the level editor (F1) and the Level
/// Generator, and makes fresh levels from the generator's settings. The
/// `base` binary is the game: it plays the saved level presets.
pub(crate) fn dev_tools() -> bool {
	env!("CARGO_BIN_NAME") == "debug"
}

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
	/// Main menu, playing, or paused
	screen: menu::Screen,
	/// What the options menus set: text size, text filter, key binds
	settings: menu::Settings,
	framebuffer: Framebuffer,
	glyphs: GlyphSheet,
	/// The Raven icons enemies and items are drawn with
	icons: IconSheet,
	/// Level presets saved from the Generator screen; the game build plays
	/// these in order, one per depth
	presets: Vec<presets::Preset>,
	/// When set, the first level of the next game uses exactly this seed
	/// (the Generator screen's Play It)
	first_seed: Option<u64>,
	/// Every cell levels are furnished from: the hand-made ones from cells/,
	/// then the ones generated from the asset catalog at startup
	cells: Vec<cells::Cell>,
	/// Fog: darkness, and the map showing only what's been seen. The debug
	/// binary starts with it off and toggles it with F3.
	fog: bool,
	/// Shown on the main menu: how the last run ended
	notice: Option<String>,
}

impl Game {
	pub fn new() -> Result<Self, String> {
		//needs base ground plane from hex on round world grid.
		let mut game = Self {
			levels: Vec::new(),
			depth: 0,
			run: 0,
			screen: menu::Screen::MainMenu { selected: 0 },
			settings: menu::Settings::default(),
			framebuffer: Framebuffer::new(0, 0),
			glyphs: GlyphSheet::load()?,
			icons: IconSheet::load()?,
			presets: presets::Preset::load_all(),
			first_seed: None,
			cells: Vec::new(),
			fog: !dev_tools(),
			notice: None,
		};
		game.make_cells();
		game.levels = vec![game.make_level(0)?];
		Ok(game)
	}

	/// Throws away every level and starts over on a fresh top level.
	fn new_game(&mut self) -> Result<(), String> {
		self.presets = presets::Preset::load_all();
		self.levels = vec![self.make_level(0)?];
		self.depth = 0;
		self.run += 1;
		self.notice = None;
		Ok(())
	}

	/// Builds the level for `depth` out of the saved cells.
	/// - The debug binary uses the Generator screen's settings with a fresh random
	///   seed each time, except Play It's first level, which uses its seed.
	/// - The base binary (the game) plays the saved presets in order (wrapping around);
	///   with none saved it falls back to the default settings and a random seed.
	/// - Settings no level can be made from (too few rooms fit, say) fall back
	///   to the default settings and a random seed too, rather than ending the game.
	fn make_level(&mut self, depth: usize) -> Result<World, String> {
		use rand::SeedableRng;
		let (settings, seed) = if let Some(seed) = self.first_seed.take().filter(|_| depth == 0) {
			(self.settings.generator, seed)
		} else if crate::game::dev_tools() {
			(self.settings.generator, rand::random())
		} else if !self.presets.is_empty() {
			let p = self.presets[depth % self.presets.len()];
			(p.settings, p.seed)
		} else {
			(presets::GenSettings::default(), rand::random())
		};
		let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
		World::generate_with(&mut rng, depth, &settings, &self.cells).or_else(|e| {
			eprintln!("level {}: {e}; using the default settings instead", depth + 1);
			let mut rng = rand::rngs::StdRng::seed_from_u64(rand::random());
			World::generate_with(&mut rng, depth, &presets::GenSettings::default(), &self.cells)
		})
	}

	/// The startup stage before any level: loads the hand-made cells, and
	/// generates the rest from every asset in the catalog.
	fn make_cells(&mut self) {
		use rand::SeedableRng;
		let catalog = catalog::Catalog::load();
		let mut cells = cells::load_all();
		let made = cells.len();
		cells.extend(cellgen::generate(&catalog, &mut rand::rngs::StdRng::seed_from_u64(rand::random())));
		if dev_tools() {
			eprintln!("{} assets in the catalog; {} hand-made cells, {} generated", catalog.assets.len(), made, cells.len() - made);
		}
		self.cells = cells;
	}

	/// Saves the Generator screen's level as a preset; says where.
	fn save_preset(&mut self) -> String {
		let preset = presets::Preset { settings: self.settings.generator, seed: self.settings.generator_seed };
		match preset.save() {
			Ok(name) => format!("saved {}/{name}", presets::LEVELS_DIR),
			Err(e) => format!("save failed: {e}"),
		}
	}

	/// Starts a game on the Generator screen's level.
	fn play_generated(&mut self) -> Result<(), String> {
		self.first_seed = Some(self.settings.generator_seed);
		self.new_game()
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

	/// Swings the weapon at whatever's in front.
	fn attack(&mut self) -> Result<(), String> {
		self.levels[self.depth].attack();
		self.arrive(None)
	}

	/// Walks freely for `dt` seconds (outside combat); stairs work the same as
	/// stepping onto them.
	fn walk(&mut self, forward: f32, strafe: f32, dt: f32) -> Result<(), String> {
		let arrival = self.levels[self.depth].walk(forward, strafe, dt);
		self.arrive(arrival)
	}

	/// Ends the run if the hero has died: back to the main menu, which says
	/// how it ended. True if it did.
	fn check_death(&mut self) -> bool {
		let hero = &self.levels[self.depth].hero;
		if hero.hp > 0 {
			return false;
		}
		self.notice = Some(format!("You died on depth {} with {} gold.", self.depth + 1, hero.gold));
		self.screen = menu::Screen::MainMenu { selected: 0 };
		true
	}

	/// After a move: ends the run on death, and changes level on stairs.
	fn arrive(&mut self, arrival: Option<Arrival>) -> Result<(), String> {
		if self.check_death() {
			return Ok(());
		}
		let (depth, going) = match arrival {
			Some(Arrival::StairsDown) => (self.depth + 1, "You descend"),
			Some(Arrival::StairsUp) if self.depth > 0 => (self.depth - 1, "You climb back up"),
			_ => return Ok(()),
		};
		if depth == self.levels.len() {
			// Made before anything moves, so a level that can't be built
			// leaves the game as it was
			let level = self.make_level(depth)?;
			self.levels.push(level);
		} else {
			// Back on a level you've been to: arrive by the staircase you took
			let level = &mut self.levels[depth];
			let at = if depth > self.depth {
				level.stairs_up.expect("levels below the top have an up staircase")
			} else {
				level.stairs_down
			};
			level.place_player(at);
		}
		// The hero and the log go along; what was happening on the level left
		// behind is over
		let left = &mut self.levels[self.depth];
		let hero = std::mem::take(&mut left.hero);
		let log = std::mem::take(&mut left.log);
		left.events.clear();
		self.depth = depth;
		let level = &mut self.levels[depth];
		level.hero = hero;
		level.log = log;
		level.say(format!("{going} to depth {}.", depth + 1));
		// Monsters waiting by the stairs start a fight straight away
		level.update_combat();
		Ok(())
	}

	/// Draws the HUD into a `cols` x `rows` character grid; the 3D view itself
	/// is rendered by Bevy and turned into text by the filter shader. While the
	/// level editor is open (`editing`) all of it stays out of the way.
	fn render(&mut self, cols: usize, rows: usize, editing: bool) {
		// Reallocate only when the window is resized
		if cols != self.framebuffer.width || rows != self.framebuffer.height {
			self.framebuffer = Framebuffer::new(cols, rows);
		}
		self.framebuffer.clear();
		if editing {
			return;
		}
		// The game shows behind the pause menu and the menus it opens; menus
		// opened from the main menu cover the whole screen anyway
		let in_game = matches!(self.screen, menu::Screen::Playing | menu::Screen::Inventory { .. }) || self.screen.paused();
		if in_game {
			let world = &self.levels[self.depth];
			let reveal_all = !self.fog;
			minimap::draw_status(world, &mut self.framebuffer);
			match self.settings.map {
				menu::MapView::Mini => minimap::draw(world, &mut self.framebuffer, false, reveal_all),
				menu::MapView::Full => minimap::draw(world, &mut self.framebuffer, true, reveal_all),
				menu::MapView::Hidden => {}
			}
		}
		menu::draw(self.screen, &self.settings, &mut self.framebuffer);
		match self.screen {
			menu::Screen::Inventory { selected } => {
				menu::draw_inventory(&self.levels[self.depth].hero, selected, &mut self.framebuffer);
			}
			menu::Screen::MainMenu { .. } => {
				if let Some(notice) = &self.notice {
					menu::draw_notice(notice, &mut self.framebuffer);
				}
			}
			_ => {}
		}
	}
}
