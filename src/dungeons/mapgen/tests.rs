// Tests for the map generators in mapgen.rs.
use super::*;
use rand::SeedableRng;
use rand::rngs::StdRng;

fn floor_count(map: &MapGrid) -> usize {
	(0..map.x as i32)
		.flat_map(|x| (0..map.y as i32).map(move |y| (x, y)))
		.filter(|&(x, y)| !map.is_wall(x, y))
		.count()
}

fn border_is_solid(map: &MapGrid) -> bool {
	let (w, h) = (map.x as i32, map.y as i32);
	(0..w).all(|x| map.is_wall(x, 0) && map.is_wall(x, h - 1))
		&& (0..h).all(|y| map.is_wall(0, y) && map.is_wall(w - 1, y))
}

#[test]
fn off_map_is_wall_and_set_floor_ignores_it() {
	let mut map = MapGrid::new(3, 3, 1);
	map.set_floor(-1, 5);
	assert!(map.is_wall(-1, 0) && map.is_wall(3, 0));
	map.set_floor(1, 1);
	assert!(!map.is_wall(1, 1));
	assert_eq!(map.render(), "###\n#.#\n###\n");
}

#[test]
fn diagonal_corridors_are_walkable_without_diagonal_steps() {
	let mut map = MapGrid::new(12, 8, 1);
	carve_diagonal(&mut map, (1, 1), (9, 6));
	assert!(!map.is_wall(1, 1) && !map.is_wall(9, 6));
	// Exactly the tiles of a 4-connected path: 8 + 5 steps after the start
	assert_eq!(map.grid.iter().filter(|w| !**w).count(), 14);
	// Every floor tile but the ends has an open orthogonal neighbor on each side of the walk
	for x in 0..12 {
		for y in 0..8 {
			if !map.is_wall(x, y) && (x, y) != (1, 1) && (x, y) != (9, 6) {
				let n = [(0, 1), (0, -1), (1, 0), (-1, 0)]
					.iter()
					.filter(|(dx, dy)| !map.is_wall(x + dx, y + dy))
					.count();
				assert_eq!(n, 2, "({x}, {y}) is a dead end or a branch");
			}
		}
	}
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
			generate_dungeon(&mut rng, 60, 18, 1, 8, 4, 8),
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
