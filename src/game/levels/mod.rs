// Making levels. `World::generate_with` below builds one, in stages:
//   1. Lay out rooms and corridors (dungeons::mapgen).
//   2. Shape them: some rooms are raised or sunk (with a stair tile joining
//      them to their corridors), some are tall halls with galleries, and
//      some get ledges or pits.
//   3. Build every tile from the modular dungeon kit (models/Tiles).
//   4. Furnish it with cells: hand-made ones from cells/, then the ones
//      generated at startup from the asset catalog, matched to each room's
//      theme.
//   5. Check for clipping: anything that ends up inside something else or
//      in a wall is taken out.
//   6. Add the stairs, treasure, items, and monsters.
//
// A good order to read the files in: kit.rs (the pieces and the numbers),
// builder.rs (placing models), shape.rs (stage 2), tiles.rs (stages 3 and 4),
// populate.rs (stage 6). The last four files are the furnishing side: the
// asset catalog, cells, the cells generated from the catalog at startup, and
// the saved level presets.
//
// Visibility: `pub(crate)` items are used outside levels/; `pub(super)` ones
// only inside it.

/// The dungeon kit's pieces, how they're tinted, and the numbers that size a level.
mod kit;
/// Loading and placing models, and the clipping check.
mod builder;
/// Stage 2: raised, sunken and tall rooms, stair tiles, ledges and pits.
mod shape;
/// Stages 3 and 4: building and furnishing every walkable tile.
mod tiles;
/// Stage 6: chests, items, and monsters.
mod populate;
/// Every furniture, object, and light model, sorted by where it goes.
pub(crate) mod catalog;
/// Hand-built one-tile pieces of rooms and halls, and which tiles they fit.
pub(crate) mod cells;
/// Cells generated at startup from the catalog.
pub(crate) mod cellgen;
/// The generator's settings, and saved level presets.
pub(crate) mod presets;

use rand::Rng;

use crate::dungeons::mapgen::generate_dungeon_with_options;
use crate::game::play::items::Hero;
use crate::game::play::player::Player;
use crate::game::play::world::World;
use builder::Builder;
use cells::Cell;
use kit::{KitMeshes, BASEMENT, DUNGEON};
use populate::{free_tile, place_chests, place_items_and_monsters};
use presets::GenSettings;
use shape::{shape_rooms, RoomIndex};
use tiles::{build_tiles, Built, Layout};

impl World {
	/// Builds a level with the default settings and the dungeon kit only.
	#[cfg(test)]
	pub(crate) fn generate(rng: &mut impl Rng, depth: usize) -> Result<Self, String> {
		Self::generate_with(rng, depth, &GenSettings::default(), &[])
	}

	/// Builds a level from `settings`, furnished with `cells` (see the stages
	/// at the top of this file). The player starts in the first room, by the
	/// up staircase when `depth` > 0; the down staircase is in the last room.
	pub(crate) fn generate_with(
		rng: &mut impl Rng,
		depth: usize,
		settings: &GenSettings,
		cells: &[Cell],
	) -> Result<Self, String> {
		// 1. Rooms and corridors
		let s = settings;
		let (map, rooms) = generate_dungeon_with_options(
			rng,
			s.width,
			s.height,
			s.splits,
			s.max_rooms,
			s.room_min,
			s.room_max,
			s.diagonal as f64 / 100.0,
		);
		if rooms.len() < 2 {
			return Err("the generator made fewer than 2 rooms, so there's nowhere for the stairs".into());
		}
		let start = rooms[0].center();
		let theme = if depth % 2 == 0 { &DUNGEON } else { &BASEMENT };
		let index = RoomIndex::new(&map, &rooms);

		// The way down is in the last room, kept off the room's center so the
		// corridor in doesn't land you on it by accident
		let last = rooms.len() - 1;
		let stairs_down = free_tile(rng, &rooms[last], &[rooms[last].center()]).unwrap_or(rooms[last].center());
		let stairs_up = (depth > 0).then_some(start);

		// 2. The rooms' shapes
		let (plans, terrain, features) = shape_rooms(rng, &map, &rooms, &index, &[start, stairs_down]);

		// 3 and 4. Every tile, built and furnished
		let mut b = Builder::new();
		let kit = KitMeshes::load(&mut b, theme)?;
		let layout = Layout {
			map: &map,
			rooms: &rooms,
			index: &index,
			plans: &plans,
			terrain: &terrain,
			features: &features,
			stairs_down,
			stairs_up,
			cells,
		};
		let Built { mut lights, mut blockers, hand_made } = build_tiles(rng, &mut b, &kit, &layout)?;

		// 6, first part. Chests, which the clipping check keeps furniture out of
		let mut taken = vec![start, stairs_down];
		taken.extend(hand_made);
		let (chests, chest_meshes) = place_chests(rng, &mut b, &rooms, &terrain, &mut taken, &mut blockers)?;

		// 5. The clipping check
		let removed = b.remove_clipping(&map, &mut lights);
		if crate::game::dev_tools() {
			eprintln!("level {}: {} rooms, {} things taken out for clipping", depth + 1, rooms.len(), removed);
		}

		// 6, the rest. Items and monsters
		let (items, enemies) = place_items_and_monsters(rng, &rooms, s, depth, &mut taken);

		// Tiles carry the level as z
		let z = depth as i32;
		let seen = vec![false; map.x * map.y];
		let mut world = Self {
			map,
			player: Player::new(start.0, start.1, z),
			meshes: b.meshes,
			objects: b.objects,
			lights,
			depth,
			stairs_down: (stairs_down.0, stairs_down.1, z),
			stairs_up: stairs_up.map(|(x, y)| (x, y, z)),
			seen,
			blockers,
			terrain,
			enemies,
			next_item_id: items.len() as u32,
			items,
			chests,
			chest_meshes,
			events: Vec::new(),
			hero: Hero::default(),
			combat: false,
			log: Vec::new(),
		};
		world.place_player((start.0, start.1, z));
		Ok(world)
	}
}
