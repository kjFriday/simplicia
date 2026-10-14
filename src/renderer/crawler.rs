// First-person grid dungeon crawler: builds a 3D world out of a generated map
// and the bundled models, and moves the player one tile per key press.
use super::*;

use crate::dungeons::mapgen::{generate_dungeon_with_rooms, MapGrid, Room};

/// What the player can do in one turn. The window maps keys to these.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Action {
	Forward,
	Back,
	TurnLeft,
	TurnRight,
}
use rand::{Rng, RngExt};

/// World units per map tile. Matches the 4 m cube used for walls.
pub(super) const TILE: f32 = 4.0;
/// Eye height above the floor.
const EYE_HEIGHT: f32 = 2.0;

/// Map size and room settings handed to the room & corridor generator.
const MAP_WIDTH: usize = 48;
const MAP_HEIGHT: usize = 32;
const MAX_DEPTH: usize = 4;
const MAX_ROOMS: usize = 12;
const ROOM_MIN: i32 = 4;
const ROOM_MAX: i32 = 8;

const WALL_MODEL: &str = "terrain/canyon/SM_AP_Cube_4m_0.glb";
const FLOOR_MODEL: &str = "terrain/islands/Stone Platform.glb";
/// One tile wide and one wall high; sunk a full tile, it reads as a way down.
const STAIRS_MODEL: &str = "stairs.obj";

/// Things scattered in rooms: model, footprint in world units.
const ENEMY: (&str, f32) = ("characters/Slime Enemy.glb", 2.4);
const CHEST: (&str, f32) = ("items/Chest.glb", 2.0);
const LOOT: &[(&str, f32)] = &[("items/Sword.glb", 1.2), ("items/Shield.glb", 1.2)];
const CLUTTER: &[(&str, f32)] = &[
	("terrain/rocks/Rock.glb", 1.4),
	("terrain/rocks/Small Rock.glb", 0.9),
	("terrain/rocks/Large Rock.glb", 1.8),
	("nature/trees/Log.glb", 2.2),
];

/// Compass directions in map space; map y grows south, which is world +Z.
const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

pub(super) struct Player {
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) z: i32,
	/// Index into DIRECTIONS: 0 north, 1 east, 2 south, 3 west
	facing: usize,
	/// Mouselook, in radians, on top of `facing`. Yaw stays within ±45°: past
	/// that, `facing` turns to the nearer direction so W walks where you look.
	look_yaw: f32,
	look_pitch: f32,
}

/// Furthest you can look up or down.
const MAX_PITCH: f32 = 80.0 * std::f32::consts::PI / 180.0;

impl Player {
	fn new(x: i32, y: i32, z: i32) -> Self {
		Self { x, y, z, facing: 0, look_yaw: 0.0, look_pitch: 0.0 }
	}

	/// Turns the view by mouse movement, in radians. Positive yaw turns right,
	/// positive pitch looks up.
	pub(super) fn look(&mut self, yaw: f32, pitch: f32) {
		use std::f32::consts::FRAC_PI_2;
		self.look_yaw += yaw;
		self.look_pitch = (self.look_pitch + pitch).clamp(-MAX_PITCH, MAX_PITCH);
		while self.look_yaw > FRAC_PI_2 / 2.0 {
			self.look_yaw -= FRAC_PI_2;
			self.facing = (self.facing + 1) % 4;
		}
		while self.look_yaw < -FRAC_PI_2 / 2.0 {
			self.look_yaw += FRAC_PI_2;
			self.facing = (self.facing + 3) % 4;
		}
	}

	/// 0 north, 1 east, 2 south, 3 west
	pub(super) fn facing(&self) -> usize {
		self.facing
	}

	fn forward(&self) -> (i32, i32) {
		DIRECTIONS[self.facing]
	}

	/// Where the eye is, in world space.
	fn eye(&self) -> Vec3 {
		tile_center(self.x, self.y) + Vec3::new(0.0, EYE_HEIGHT, 0.0)
	}

	pub(super) fn view_matrix(&self) -> Mat4 {
		let (dx, dy) = self.forward();
		// Map east (+X) is to the right of north (-Z), so turning right is a
		// negative rotation about +Y
		let dir = glam::Quat::from_rotation_y(-self.look_yaw) * Vec3::new(dx as f32, 0.0, dy as f32);
		let dir = Vec3::new(dir.x * self.look_pitch.cos(), self.look_pitch.sin(), dir.z * self.look_pitch.cos());
		glam::camera::rh::view::look_to_mat4(self.eye(), dir, Vec3::Y)
	}

	/// One action, one turn. Walls block movement.
	/// Returns true if the player did something.
	pub(super) fn act(&mut self, action: Action, map: &MapGrid) -> bool {
		match action {
			Action::Forward => self.step(map, 1),
			Action::Back => self.step(map, -1),
			Action::TurnLeft => {
				self.facing = (self.facing + 3) % 4;
				true
			}
			Action::TurnRight => {
				self.facing = (self.facing + 1) % 4;
				true
			}
		}
	}

	fn step(&mut self, map: &MapGrid, sign: i32) -> bool {
		let (dx, dy) = self.forward();
		let (nx, ny) = (self.x + dx * sign, self.y + dy * sign);
		if map.is_wall(nx, ny) {
			return false;
		}
		(self.x, self.y) = (nx, ny);
		true
	}
}

/// The generated dungeon: the map for movement rules, and what to draw.
pub(super) struct World {
	pub(super) map: MapGrid,
	pub(super) player: Player,
	pub(super) meshes: Vec<Mesh>,
	pub(super) objects: Vec<SceneObject>,
	/// 0 is the top level; each staircase down adds one
	pub(super) depth: usize,
	/// Stepping here takes you one level deeper
	pub(super) stairs_down: (i32, i32, i32),
	/// Stepping here takes you back up; none on the top level
	pub(super) stairs_up: Option<(i32, i32, i32)>,
	/// Tiles the minimap shows, indexed [x * height + y]
	seen: Vec<bool>,
}

/// How far around the player the minimap uncovers, in tiles.
const REVEAL_RADIUS: i32 = 3;

/// Where a move left the player.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Arrival {
	Floor,
	StairsDown,
	StairsUp,
}

impl World {
	pub(super) fn has_seen(&self, x: i32, y: i32) -> bool {
		x >= 0
			&& y >= 0
			&& (x as usize) < self.map.x as usize
			&& (y as usize) < self.map.y as usize
			&& self.seen[x as usize * self.map.y as usize + y as usize]
	}

	fn reveal_around_player(&mut self) {
		let (px, py) = (self.player.x, self.player.y);
		for x in px - REVEAL_RADIUS..=px + REVEAL_RADIUS {
			for y in py - REVEAL_RADIUS..=py + REVEAL_RADIUS {
				if x >= 0 && y >= 0 && (x as usize) < self.map.x && (y as usize) < self.map.y {
					self.seen[x as usize * self.map.y + y as usize] = true;
				}
			}
		}
	}

	/// Passes an action to the player, uncovers the map around them, and says
	/// whether they just stepped onto a staircase.
	pub(super) fn act(&mut self, action: Action) -> Option<Arrival> {
		let before = (self.player.x, self.player.y);
		self.player.act(action, &self.map);
		let here = (self.player.x, self.player.y);
		if here == before {
			// Turning in place, or bumping a wall, never triggers stairs
			return None;
		}
		self.reveal_around_player();
		Some(if here == (self.stairs_down.0, self.stairs_down.1) {
			Arrival::StairsDown
		} else if Some(here) == self.stairs_up.map(|s| (s.0, s.1)) {
			Arrival::StairsUp
		} else {
			Arrival::Floor
		})
	}

	/// Puts the player on `tile`, facing open floor, and uncovers around them.
	/// Puts the player on the first open tile next to `tile` (a staircase or
	/// start), facing away from it, so they aren't standing inside the stairs.
	pub(super) fn place_player(&mut self, tile: (i32, i32, i32)) {
		let facing = open_facing(&self.map, tile);
		let (dx, dy) = DIRECTIONS[facing];
		// open_facing falls back to north when boxed in; stay put then
		let step = if self.map.is_wall(tile.0 + dx, tile.1 + dy) { (0, 0) } else { (dx, dy) };
		self.player.x = tile.0 + step.0;
		self.player.y = tile.1 + step.1;
		self.player.z = tile.2;

		self.player.facing = facing;
		self.reveal_around_player();
	}
}

/// The first direction from `tile` that isn't a wall, so the view isn't a wall.
fn open_facing(map: &MapGrid, tile: (i32, i32, i32)) -> usize {
	(0..4)
		.find(|&f| {
			let (dx, dy) = DIRECTIONS[f];
			!map.is_wall(tile.0 + dx, tile.1 + dy)
		})
		.unwrap_or(0)
}

fn tile_center(x: i32, y: i32) -> Vec3 {
	Vec3::new((x as f32 + 0.5) * TILE, 0.0, (y as f32 + 0.5) * TILE)
}

/// Loads a model and reshapes it to stand on a tile: centered on X/Z, lowest
/// point at y = 0, and scaled so its wider side is `footprint` world units.
fn load_tile_model(name: &str, footprint: f32) -> Result<Mesh, String> {
	let mut mesh = Mesh::load(name)?;
	let (min, max) = mesh.vertices.iter().fold(
		(Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
		|(min, max), v| (min.min(v.pos), max.max(v.pos)),
	);
	let size = max - min;
	let widest = size.x.max(size.z);
	let scale = if widest > 0.0 { footprint / widest } else { 1.0 };
	let base = Vec3::new((min.x + max.x) * 0.5, min.y, (min.z + max.z) * 0.5);
	for v in &mut mesh.vertices {
		v.pos = (v.pos - base) * scale;
	}
	Ok(mesh)
}

/// Random floor tile inside a room, avoiding `taken` tiles. None if the room is full.
fn free_tile(rng: &mut impl Rng, room: &Room, taken: &[(i32, i32)]) -> Option<(i32, i32)> {
	for _ in 0..32 {
		let tile = (rng.random_range(room.x1..room.x2), rng.random_range(room.y1..room.y2));
		if !taken.contains(&tile) {
			return Some(tile);
		}
	}
	None
}

impl World {
	/// Builds a level. The player starts in the first room, on the up staircase
	/// when `depth` > 0; the down staircase is in the last room.
	pub(super) fn generate(rng: &mut impl Rng, depth: usize) -> Result<Self, String> {
		let (map, rooms) = generate_dungeon_with_rooms(rng, MAP_WIDTH, MAP_HEIGHT, MAX_DEPTH, MAX_ROOMS, ROOM_MIN, ROOM_MAX);
		if rooms.len() < 2 {
			return Err("the generator made fewer than 2 rooms, so there's nowhere for the stairs".into());
		}
		let start = rooms[0].center();

		let mut meshes = Vec::new();
		let mut objects = Vec::new();
		// Loads each model once; objects then refer to it by index
		let mut add_model = |name: &str, footprint: f32| -> Result<usize, String> {
			meshes.push(load_tile_model(name, footprint)?);
			Ok(meshes.len() - 1)
		};

		let wall = add_model(WALL_MODEL, TILE)?;
		let floor = add_model(FLOOR_MODEL, TILE)?;
		let stairs = add_model(STAIRS_MODEL, TILE)?;
		let enemy = add_model(ENEMY.0, ENEMY.1)?;
		let chest = add_model(CHEST.0, CHEST.1)?;
		let loot = LOOT.iter().map(|&(n, f)| add_model(n, f)).collect::<Result<Vec<_>, _>>()?;
		let clutter = CLUTTER.iter().map(|&(n, f)| add_model(n, f)).collect::<Result<Vec<_>, _>>()?;

		let place = |objects: &mut Vec<SceneObject>, mesh: usize, (x, y): (i32, i32), yaw: f32, lift: f32| {
			let at = tile_center(x, y) + Vec3::new(0.0, lift, 0.0);
			objects.push(SceneObject {
				mesh,
				transform: Mat4::from_translation(at) * Mat4::from_rotation_y(yaw),
			});
		};

		// The way down is in the last room, kept off the room's center so the
		// corridor in doesn't land you on it by accident
		let last = rooms.len() - 1;
		let stairs_down = free_tile(rng, &rooms[last], &[rooms[last].center()]).unwrap_or(rooms[last].center());
		let stairs_up = (depth > 0).then_some(start);

		// Floors everywhere you can walk; walls only where they border a floor,
		// since solid rock you can never see is wasted triangles
		for x in 0..MAP_WIDTH as i32 {
			for y in 0..MAP_HEIGHT as i32 {
				if (x, y) == stairs_down {
					// No floor here: the steps go down through it into a pit
					place(&mut objects, stairs, (x, y), 0.0, -TILE);
				} else if !map.is_wall(x, y) {
					// The floor slab's top sits at y = 0
					place(&mut objects, floor, (x, y), 0.0, 0.0);
				} else if DIRECTIONS.iter().any(|&(dx, dy)| !map.is_wall(x + dx, y + dy)) {
					place(&mut objects, wall, (x, y), 0.0, 0.0);
				}
			}
		}
		// Push the floor slabs down so their tops are flush with y = 0
		let floor_top = meshes[floor].vertices.iter().map(|v| v.pos.y).fold(0.0, f32::max);
		for v in &mut meshes[floor].vertices {
			v.pos.y -= floor_top;
		}

		if let Some(up) = stairs_up {
			place(&mut objects, stairs, up, 0.0, 0.0);
		}

		// Furnish every room but the first, where the player starts.
		// Props are scenery for now: they don't block movement.
		// Deeper levels get more slimes.
		let enemy_chance = (0.5 + depth as f64 * 0.1).min(1.0);
		for (i, room) in rooms.iter().enumerate().skip(1) {
			let mut taken = vec![start, stairs_down];

			if i == last {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					place(&mut objects, chest, t, rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2, 0.0);
				}
			}
			if rng.random_bool(enemy_chance) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					place(&mut objects, enemy, t, rng.random_range(0.0..std::f32::consts::TAU), 0.0);
				}
			}
			if rng.random_bool(0.4) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let item = loot[rng.random_range(0..loot.len())];
					place(&mut objects, item, t, rng.random_range(0.0..std::f32::consts::TAU), 0.0);
				}
			}
			for _ in 0..rng.random_range(1..=3) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let rock = clutter[rng.random_range(0..clutter.len())];
					place(&mut objects, rock, t, rng.random_range(0.0..std::f32::consts::TAU), 0.0);
				}
			}
		}

		// Tiles carry the level as z
		let z = depth as i32;
		let seen = vec![false; map.x * map.y];
		let mut world = Self {
			map,
			player: Player::new(start.0, start.1, z),
			meshes,
			objects,
			depth,
			stairs_down: (stairs_down.0, stairs_down.1, z),
			stairs_up: stairs_up.map(|(x, y)| (x, y, z)),
			seen,
		};
		world.place_player((start.0, start.1, z));
		Ok(world)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::SeedableRng;
	use rand::rngs::StdRng;

	#[test]
	fn worlds_generate_with_player_on_floor() {
		for seed in 0..10 {
			let world = World::generate(&mut StdRng::seed_from_u64(seed), 0).unwrap();
			assert!(!world.map.is_wall(world.player.x, world.player.y), "seed {seed}");
			let tris: usize = world.objects.iter().map(|o| world.meshes[o.mesh].indices.len()).sum();
			println!("seed {seed}: {} objects, {tris} triangles", world.objects.len());
		}
	}

	#[test]
	fn walls_block_and_turning_works() {
		let mut world = World::generate(&mut StdRng::seed_from_u64(3), 0).unwrap();
		// Walk forward until a wall stops us; we must never end up inside one
		for _ in 0..100 {
			world.player.act(Action::Forward, &world.map);
			assert!(!world.map.is_wall(world.player.x, world.player.y));
		}
		let (dx, dy) = world.player.forward();
		assert!(world.map.is_wall(world.player.x + dx, world.player.y + dy));

		let facing = world.player.facing;
		for _ in 0..4 {
			world.player.act(Action::TurnRight, &world.map);
		}
		assert_eq!(world.player.facing, facing);
	}

	#[test]
	fn mouselook_turns_facing_past_45_degrees_and_clamps_pitch() {
		let mut player = Player::new(0, 0, 0);
		player.look(50f32.to_radians(), 0.0);
		assert_eq!(player.facing(), 1);
		assert!((player.look_yaw - (-40f32).to_radians()).abs() < 1e-4);
		// -40° - 10° = -50°, past -45°, so it turns back to north at +40°
		player.look(-10f32.to_radians(), 0.0);
		assert_eq!(player.facing(), 0);
		player.look(0.0, 10.0);
		assert_eq!(player.look_pitch, MAX_PITCH);
	}

	/// Stands the player on a floor tile next to `target`, facing it.
	fn stand_next_to(world: &mut World, target: (i32, i32, i32)) {
		let target = (target.0, target.1);
		for (facing, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
			let from = (target.0 - dx, target.1 - dy);
			if !world.map.is_wall(from.0, from.1) && from != target {
				world.player.x = from.0;
				world.player.y = from.1;
				world.player.facing = facing;
				return;
			}
		}
		panic!("no floor next to {target:?}");
	}

	#[test]
	fn stairs_are_on_floor_and_stepping_on_them_is_reported() {
		for seed in 0..10 {
			let mut world = World::generate(&mut StdRng::seed_from_u64(seed), 1).unwrap();
			let down = world.stairs_down;
			let up = world.stairs_up.expect("depth 1 has a way up");
			assert!(!world.map.is_wall(down.0, down.1) && !world.map.is_wall(up.0, up.1));
			assert_ne!(down, up);

			stand_next_to(&mut world, down);
			assert_eq!(world.act(Action::Forward), Some(Arrival::StairsDown));
			stand_next_to(&mut world, up);
			assert_eq!(world.act(Action::Forward), Some(Arrival::StairsUp));
		}
	}

	#[test]
	fn top_level_has_no_way_up_and_turning_never_triggers_stairs() {
		let mut world = World::generate(&mut StdRng::seed_from_u64(5), 0).unwrap();
		assert!(world.stairs_up.is_none());
		assert_eq!(world.act(Action::TurnRight), None);
	}

	#[test]
	fn minimap_draws_in_the_top_right_and_reveals_only_nearby_tiles() {
		let world = World::generate(&mut StdRng::seed_from_u64(2), 0).unwrap();
		assert!(world.has_seen(world.player.x, world.player.y));
		assert!(!world.has_seen(world.stairs_down.0, world.stairs_down.1) || {
			let (dx, dy) = (world.stairs_down.0 - world.player.x, world.stairs_down.1 - world.player.y);
			dx.abs() <= REVEAL_RADIUS && dy.abs() <= REVEAL_RADIUS
		});

		let mut fb = Framebuffer::new(120, 40);
		super::super::minimap::draw(&world, &mut fb);
		let row0: String = fb.color_buffer[..120].iter().map(|c| c.ch).collect();
		assert!(row0.ends_with('┐') && row0.contains("Depth 1"), "{row0}");
		let arrow = ['▲', '►', '▼', '◄'][world.player.facing];
		assert!(fb.color_buffer.iter().any(|c| c.ch == arrow));
		// Nothing drawn in the left half
		assert!(fb.color_buffer.iter().enumerate().all(|(i, c)| i % 120 >= 60 || c.ch == ' '));
	}
}
