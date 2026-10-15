// Dungeon map generators: rooms & corridors, drunkard's walk, cellular automata.
// Every generator takes the random number generator as an argument, so a seeded
// one (rand::rngs::StdRng::seed_from_u64) gives the same map every time.
use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

/// A 2D map of wall and floor tiles, `x` wide and `y` tall.
pub struct MapGrid {
	pub x: usize,
	pub y: usize,
	pub z: usize,
	//welcome to the grid ARCADIANS
	/// One entry per tile, column by column; `true` is wall.
	pub grid: Vec<bool>,
}

impl MapGrid {
	/// A map that is solid wall everywhere.
	pub fn new(x: usize, y: usize, z: usize) -> Self {
		Self { x, y, z, grid: vec![true; x * y] }
	}

	/// Where tile (`x`, `y`) sits in `grid`, or `None` if it's off the map.
	fn index(&self, x: i32, y: i32) -> Option<usize> {
		let in_bounds = x >= 0 && y >= 0 && (x as usize) < self.x && (y as usize) < self.y;
		in_bounds.then(|| x as usize * self.y + y as usize)
	}

	/// Makes tile (`x`, `y`) floor. Out-of-bounds coordinates are ignored.
	pub fn set_floor(&mut self, x: i32, y: i32) {
		if let Some(i) = self.index(x, y) {
			self.grid[i] = false;
		}
	}

	/// Whether tile (`x`, `y`) is wall. Anything off the map counts as wall.
	pub fn is_wall(&self, x: i32, y: i32) -> bool {
		self.index(x, y).is_none_or(|i| self.grid[i])
	}

	/// The map as text, one line per row.
	pub fn render(&self) -> String {
		let mut out = String::with_capacity((self.x + 1) * self.y);
		for y in 0..self.y as i32 {
			for x in 0..self.x as i32 {
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
/// Share of corridors that run diagonally instead of as an L.
const DIAGONAL_CHANCE: f64 = 0.35;

/// A rectangular room. It covers x1..x2 by y1..y2; x2 and y2 are just outside it.
#[derive(Clone, Copy)]
pub struct Room {
	pub x1: i32,
	pub y1: i32,
	pub x2: i32,
	pub y2: i32,
}

impl Room {
	/// A `w` x `h` room with its top-left corner at (`x`, `y`).
	pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
		Self { x1: x, y1: y, x2: x + w, y2: y + h }
	}

	/// The tile in the middle of the room, rounded toward the top-left.
	pub fn center(&self) -> (i32, i32) {
		((self.x1 + self.x2) / 2, (self.y1 + self.y2) / 2)
	}

	/// Touching edges count as intersecting, which keeps a wall between rooms.
	pub fn intersects(&self, other: &Room) -> bool {
		self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
	}
}

/// A rooms-and-corridors map: up to `max_rooms` rooms of `room_min` to
/// `room_max` tiles a side, each joined to the one placed before it.
/// Panics if `room_max` doesn't fit inside the map with a 1-tile border.
pub fn generate_dungeon(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	depth: usize,
	max_rooms: usize,
	room_min: i32,
	room_max: i32,
) -> MapGrid {
	generate_dungeon_with_rooms(rng, width, height, depth, max_rooms, room_min, room_max).0
}

/// Same as `generate_dungeon`, but also returns the rooms that were carved,
/// in the order they were placed (each one is joined to the one before it).
pub fn generate_dungeon_with_rooms(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	depth: usize,
	max_rooms: usize,
	room_min: i32,
	room_max: i32,
) -> (MapGrid, Vec<Room>) {
	generate_dungeon_with_options(rng, width, height, depth, max_rooms, room_min, room_max, DIAGONAL_CHANCE)
}

/// Like `generate_dungeon_with_rooms`, with the share of diagonal corridors
/// (0 to 1) chosen by the caller.
#[allow(clippy::too_many_arguments)]
pub fn generate_dungeon_with_options(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	depth: usize,
	max_rooms: usize,
	room_min: i32,
	room_max: i32,
	diagonal_chance: f64,
) -> (MapGrid, Vec<Room>) {
	let mut dungeon = MapGrid::new(width, height, depth);
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
		carve_room(&mut dungeon, &new_room);

		// Connect to the previous room: a diagonal corridor or an L
		if let Some(prev) = rooms.last() {
			connect_rooms(rng, &mut dungeon, prev, &new_room, diagonal_chance);
		}

		rooms.push(new_room);
	}

	(dungeon, rooms)
}

/// Makes every tile inside `room` floor.
fn carve_room(map: &mut MapGrid, room: &Room) {
	for rx in room.x1..room.x2 {
		for ry in room.y1..room.y2 {
			map.set_floor(rx, ry);
		}
	}
}

/// Joins the centers of `prev` and `curr` with a corridor: diagonal with
/// chance `diagonal_chance`, otherwise an L that turns either way.
fn connect_rooms(rng: &mut impl Rng, map: &mut MapGrid, prev: &Room, curr: &Room, diagonal_chance: f64) {
	let (prev_x, prev_y) = prev.center();
	let (curr_x, curr_y) = curr.center();

	if rng.random_bool(diagonal_chance.clamp(0.0, 1.0)) {
		carve_diagonal(map, (prev_x, prev_y), (curr_x, curr_y));
	} else if rng.random_bool(0.5) {
		// Horizontal then vertical
		for cx in prev_x.min(curr_x)..=prev_x.max(curr_x) {
			map.set_floor(cx, prev_y);
		}
		for cy in prev_y.min(curr_y)..=prev_y.max(curr_y) {
			map.set_floor(curr_x, cy);
		}
	} else {
		// Vertical then horizontal
		for cy in prev_y.min(curr_y)..=prev_y.max(curr_y) {
			map.set_floor(prev_x, cy);
		}
		for cx in prev_x.min(curr_x)..=prev_x.max(curr_x) {
			map.set_floor(cx, curr_y);
		}
	}
}

/// Carves a corridor along the straight line from `a` to `b`, as a staircase
/// of single steps east/west or north/south, so every tile is reachable
/// without moving diagonally.
fn carve_diagonal(map: &mut MapGrid, a: (i32, i32), b: (i32, i32)) {
	let (dx, dy) = (b.0 - a.0, b.1 - a.1);
	let (sx, sy) = (dx.signum(), dy.signum());
	let (nx, ny) = (dx.abs(), dy.abs());
	let (mut x, mut y) = a;
	let (mut ix, mut iy) = (0, 0);
	map.set_floor(x, y);
	while ix < nx || iy < ny {
		// Step along whichever axis is further behind the line
		if (1 + 2 * ix) * ny < (1 + 2 * iy) * nx {
			x += sx;
			ix += 1;
		} else {
			y += sy;
			iy += 1;
		}
		map.set_floor(x, y);
	}
}

// -------------------------------------------------------------------
// 2. Drunkard's Walk (Organic Caverns)
// -------------------------------------------------------------------
/// A cavern carved by a random walker starting in the middle, until
/// `fill_ratio` (0 to 1) of the map is floor or the whole interior is.
pub fn generate_drunkard_walk(rng: &mut impl Rng, width: usize, height: usize, fill_ratio: f64) -> MapGrid {
	let mut dungeon = MapGrid::new(width, height, 1);
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
/// A cave grown from random noise: each tile starts as wall with chance
/// `fill_prob`, then `iterations` smoothing passes round it into caverns.
pub fn generate_cellular_cave(
	rng: &mut impl Rng,
	width: usize,
	height: usize,
	fill_prob: f64,
	iterations: usize,
) -> MapGrid {
	let mut dungeon = MapGrid::new(width, height, 1);
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
		let mut next = MapGrid::new(width, height, 1);
		for x in 1..w - 1 {
			for y in 1..h - 1 {
				let wall_count = walls_around(&dungeon, x, y);

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

/// How many of the 8 tiles around (`x`, `y`) are wall.
fn walls_around(map: &MapGrid, x: i32, y: i32) -> u32 {
	let mut wall_count = 0;
	for dx in -1..=1 {
		for dy in -1..=1 {
			if dx == 0 && dy == 0 {
				continue;
			}
			if map.is_wall(x + dx, y + dy) {
				wall_count += 1;
			}
		}
	}
	wall_count
}

// -------------------------------------------------------------------
// Usage Example
// -------------------------------------------------------------------
/// Prints one map from each generator.
pub fn print_demo() {
	let mut rng = rand::rng();

	println!("=== 1. CLASSIC ROOM & CORRIDOR ===");
	print!("{}", generate_dungeon(&mut rng, 60, 18, 1, 8, 4, 8).render());

	println!("\n=== 2. DRUNKARD'S WALK CAVERN ===");
	print!("{}", generate_drunkard_walk(&mut rng, 60, 18, 0.35).render());

	println!("\n=== 3. CELLULAR AUTOMATA CAVE ===");
	print!("{}", generate_cellular_cave(&mut rng, 60, 18, 0.45, 4).render());
}

#[cfg(test)]
mod tests;
