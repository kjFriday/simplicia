// Dungeon map generators: rooms & corridors, drunkard's walk, cellular automata.
// Every generator takes the random number generator as an argument, so a seeded
// one (rand::rngs::StdRng::seed_from_u64) gives the same map every time.
use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

pub struct MapGrid {
	pub width: usize,
	pub height: usize,
	/// true = Wall (#), false = Floor (.); indexed [x * height + y]
	grid: Vec<bool>,
}

impl MapGrid {
	/// A map that is solid wall everywhere.
	pub fn new(width: usize, height: usize) -> Self {
		Self { width, height, grid: vec![true; width * height] }
	}

	fn index(&self, x: i32, y: i32) -> Option<usize> {
		let in_bounds = x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height;
		in_bounds.then(|| x as usize * self.height + y as usize)
	}

	/// Out-of-bounds coordinates are ignored.
	pub fn set_floor(&mut self, x: i32, y: i32) {
		if let Some(i) = self.index(x, y) {
			self.grid[i] = false;
		}
	}

	/// Anything off the map counts as wall.
	pub fn is_wall(&self, x: i32, y: i32) -> bool {
		self.index(x, y).is_none_or(|i| self.grid[i])
	}

	/// The map as text, one line per row.
	pub fn render(&self) -> String {
		let mut out = String::with_capacity((self.width + 1) * self.height);
		for y in 0..self.height as i32 {
			for x in 0..self.width as i32 {
				out.push(if self.is_wall(x, y) { '#' } else { '.' });
			}
			out.push('\n');
		}
		out
	}
}

// -------------------------------------------------------------------
// 1. Classic Room & Corridor Generator
// -------------------------------------------------------------------
#[derive(Clone, Copy)]
pub struct Room {
	pub x1: i32,
	pub y1: i32,
	pub x2: i32,
	pub y2: i32,
}

impl Room {
	pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
		Self { x1: x, y1: y, x2: x + w, y2: y + h }
	}

	pub fn center(&self) -> (i32, i32) {
		((self.x1 + self.x2) / 2, (self.y1 + self.y2) / 2)
	}

	/// Touching edges count as intersecting, which keeps a wall between rooms.
	pub fn intersects(&self, other: &Room) -> bool {
		self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
	}
}

/// Panics if `room_max` doesn't fit inside the map with a 1-tile border.
pub fn generate_dungeon(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	max_rooms: usize,
	room_min: i32,
	room_max: i32,
) -> MapGrid {
	generate_dungeon_with_rooms(rng, width, height, max_rooms, room_min, room_max).0
}

/// Same as `generate_dungeon`, but also returns the rooms that were carved,
/// in the order they were placed (each one is joined to the one before it).
pub fn generate_dungeon_with_rooms(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	max_rooms: usize,
	room_min: i32,
	room_max: i32,
) -> (MapGrid, Vec<Room>) {
	let mut dungeon = MapGrid::new(width, height);
	let mut rooms: Vec<Room> = Vec::new();
	let (width, height) = (width as i32, height as i32);

	for _ in 0..max_rooms {
		let w = rng.random_range(room_min..=room_max);
		let h = rng.random_range(room_min..=room_max);
		let x = rng.random_range(1..=width - w - 1);
		let y = rng.random_range(1..=height - h - 1);
		let new_room = Room::new(x, y, w, h);

		if rooms.iter().any(|other| new_room.intersects(other)) {
			continue;
		}

		// Carve room
		for rx in new_room.x1..new_room.x2 {
			for ry in new_room.y1..new_room.y2 {
				dungeon.set_floor(rx, ry);
			}
		}

		// Connect with L-shaped corridor to previous room
		if let Some(prev) = rooms.last() {
			let (prev_x, prev_y) = prev.center();
			let (curr_x, curr_y) = new_room.center();

			if rng.random_bool(0.5) {
				// Horizontal then vertical
				for cx in prev_x.min(curr_x)..=prev_x.max(curr_x) {
					dungeon.set_floor(cx, prev_y);
				}
				for cy in prev_y.min(curr_y)..=prev_y.max(curr_y) {
					dungeon.set_floor(curr_x, cy);
				}
			} else {
				// Vertical then horizontal
				for cy in prev_y.min(curr_y)..=prev_y.max(curr_y) {
					dungeon.set_floor(prev_x, cy);
				}
				for cx in prev_x.min(curr_x)..=prev_x.max(curr_x) {
					dungeon.set_floor(cx, curr_y);
				}
			}
		}

		rooms.push(new_room);
	}

	(dungeon, rooms)
}

// -------------------------------------------------------------------
// 2. Drunkard's Walk (Organic Caverns)
// -------------------------------------------------------------------
pub fn generate_drunkard_walk(rng: &mut impl Rng, width: usize, height: usize, fill_ratio: f64) -> MapGrid {
	let mut dungeon = MapGrid::new(width, height);
	// The walker can't touch the border, so it can carve at most the interior.
	// Capping here stops a too-high fill_ratio from looping forever.
	let interior = width.saturating_sub(2) * height.saturating_sub(2);
	let target_floors = ((width * height) as f64 * fill_ratio) as usize;
	let target_floors = target_floors.min(interior);
	let mut carved = 0;

	let (width, height) = (width as i32, height as i32);
	let (mut x, mut y) = (width / 2, height / 2);
	dungeon.set_floor(x, y);
	carved += 1;

	const DIRECTIONS: [(i32, i32); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];

	while carved < target_floors {
		let &(dx, dy) = DIRECTIONS.choose(rng).unwrap();
		let (nx, ny) = (x + dx, y + dy);

		// Keep walker within outer boundaries
		if 1 <= nx && nx < width - 1 && 1 <= ny && ny < height - 1 {
			(x, y) = (nx, ny);
			if dungeon.is_wall(x, y) {
				dungeon.set_floor(x, y);
				carved += 1;
			}
		}
	}

	dungeon
}

// -------------------------------------------------------------------
// 3. Cellular Automata (Cave Systems)
// -------------------------------------------------------------------
pub fn generate_cellular_cave(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	fill_prob: f64,
	iterations: usize,
) -> MapGrid {
	let mut dungeon = MapGrid::new(width, height);
	let (w, h) = (width as i32, height as i32);

	// Initial random noise fill
	for x in 1..w - 1 {
		for y in 1..h - 1 {
			if rng.random::<f64>() > fill_prob {
				dungeon.set_floor(x, y);
			}
		}
	}

	// Smoothing passes using 8-neighbor rule B5/S5-8
	for _ in 0..iterations {
		let mut next = MapGrid::new(width, height);
		for x in 1..w - 1 {
			for y in 1..h - 1 {
				let mut wall_count = 0;
				for dx in -1..=1 {
					for dy in -1..=1 {
						if dx == 0 && dy == 0 {
							continue;
						}
						if dungeon.is_wall(x + dx, y + dy) {
							wall_count += 1;
						}
					}
				}

				// If wall count >= 5, tile stays wall; otherwise floor
				if wall_count < 5 {
					next.set_floor(x, y);
				}
			}
		}

		dungeon = next;
	}

	dungeon
}

// -------------------------------------------------------------------
// Usage Example
// -------------------------------------------------------------------
/// Prints one map from each generator.
pub fn print_demo() {
	let mut rng = rand::rng();

	println!("=== 1. CLASSIC ROOM & CORRIDOR ===");
	print!("{}", generate_dungeon(&mut rng, 60, 18, 8, 4, 8).render());

	println!("\n=== 2. DRUNKARD'S WALK CAVERN ===");
	print!("{}", generate_drunkard_walk(&mut rng, 60, 18, 0.35).render());

	println!("\n=== 3. CELLULAR AUTOMATA CAVE ===");
	print!("{}", generate_cellular_cave(&mut rng, 60, 18, 0.45, 4).render());
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::SeedableRng;
	use rand::rngs::StdRng;

	fn floor_count(map: &MapGrid) -> usize {
		(0..map.width as i32)
			.flat_map(|x| (0..map.height as i32).map(move |y| (x, y)))
			.filter(|&(x, y)| !map.is_wall(x, y))
			.count()
	}

	fn border_is_solid(map: &MapGrid) -> bool {
		let (w, h) = (map.width as i32, map.height as i32);
		(0..w).all(|x| map.is_wall(x, 0) && map.is_wall(x, h - 1))
			&& (0..h).all(|y| map.is_wall(0, y) && map.is_wall(w - 1, y))
	}

	#[test]
	fn off_map_is_wall_and_set_floor_ignores_it() {
		let mut map = MapGrid::new(3, 3);
		map.set_floor(-1, 5);
		assert!(map.is_wall(-1, 0) && map.is_wall(3, 0));
		map.set_floor(1, 1);
		assert!(!map.is_wall(1, 1));
		assert_eq!(map.render(), "###\n#.#\n###\n");
	}

	#[test]
	fn touching_rooms_intersect() {
		assert!(Room::new(0, 0, 4, 4).intersects(&Room::new(4, 0, 4, 4)));
		assert!(!Room::new(0, 0, 4, 4).intersects(&Room::new(5, 0, 4, 4)));
	}

	#[test]
	fn generators_keep_the_border_and_carve_something() {
		for seed in 0..20 {
			let mut rng = StdRng::seed_from_u64(seed);
			for map in [
				generate_dungeon(&mut rng, 60, 18, 8, 4, 8),
				generate_drunkard_walk(&mut rng, 60, 18, 0.35),
				generate_cellular_cave(&mut rng, 60, 18, 0.45, 4),
			] {
				assert!(border_is_solid(&map), "seed {seed}:\n{}", map.render());
				assert!(floor_count(&map) > 0, "seed {seed}:\n{}", map.render());
			}
		}
	}

	#[test]
	fn drunkard_walk_carves_exactly_its_target() {
		let mut rng = StdRng::seed_from_u64(1);
		let map = generate_drunkard_walk(&mut rng, 60, 18, 0.35);
		assert_eq!(floor_count(&map), (60.0 * 18.0 * 0.35) as usize);
	}

	#[test]
	fn drunkard_walk_with_impossible_ratio_still_finishes() {
		let mut rng = StdRng::seed_from_u64(1);
		let map = generate_drunkard_walk(&mut rng, 10, 6, 1.0);
		assert_eq!(floor_count(&map), 8 * 4);
	}

	#[test]
	fn same_seed_same_map() {
		let a = generate_cellular_cave(&mut StdRng::seed_from_u64(7), 60, 18, 0.45, 4);
		let b = generate_cellular_cave(&mut StdRng::seed_from_u64(7), 60, 18, 0.45, 4);
		assert_eq!(a.render(), b.render());
	}

	/// Run with `cargo test print_demo -- --ignored --nocapture` to see the maps.
	#[test]
	#[ignore]
	fn print_demo_maps() {
		print_demo();
	}
}
