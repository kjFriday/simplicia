// Level generation: lays out a dungeon map, then builds it from the modular
// dungeon kit (models/Tiles), lights it, and furnishes and populates it.
use glam::{Mat4, Vec3};
use rand::{Rng, RngExt};

use super::combat::{Enemy, EnemyKind};
use super::items::{GroundItem, Hero, ItemKind, ITEM_CHANCE};
use super::player::Player;
use super::world::*;
use crate::dungeons::mapgen::{generate_dungeon_with_rooms, Room};
use crate::engine::mesh::Mesh;
use crate::engine::scene::SceneObject;

/// Map size and room settings handed to the room & corridor generator.
pub(super) const MAP_WIDTH: usize = 48;
pub(super) const MAP_HEIGHT: usize = 32;
pub(super) const MAX_DEPTH: usize = 4;
pub(super) const MAX_ROOMS: usize = 12;
pub(super) const ROOM_MIN: i32 = 4;
pub(super) const ROOM_MAX: i32 = 8;

// The level is built from the modular dungeon kit in models/Tiles: 4 x 4 m
// pieces centered on their origin with the floor at y = 0. Wall panels stand
// on a tile's north (-Z) edge and are turned to face the other sides.

/// A look for a level: which kit pieces build it, and how they're tinted
/// (the kit's shared texture isn't bundled, so color comes from tints).
pub(super) struct Theme {
	/// Floor and ceiling for one walkable tile
	pub(super) floor: &'static str,
	/// Wall panels; the first is the most common
	pub(super) walls: &'static [&'static str],
	/// An arch where a corridor opens into a room
	pub(super) passage: &'static str,
	pub(super) stone: Vec3,
}

pub(super) const DUNGEON: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Dungeon_Wall_Var1.glb", "Tiles/Dungeon_Wall_Var2.glb", "Tiles/Dungeon_Wall_Var3.glb"],
	passage: "Tiles/Dungeon_Passage.glb",
	stone: Vec3::new(0.62, 0.6, 0.58),
};

pub(super) const BASEMENT: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Basement_Wall_Var1.glb", "Tiles/Basement_Wall_Var2.glb", "Tiles/Basement_Wall_Var3.glb"],
	passage: "Tiles/Basement_Passage.glb",
	stone: Vec3::new(0.55, 0.5, 0.44),
};

pub(super) const PILLAR: &str = "Tiles/Pillar.glb";
pub(super) const TORCH: &str = "Lights/Torch_Wall.glb";
/// The way down sinks a full flight below the floor; the way up stands on it.
pub(super) const STAIRS_DOWN: &str = "Tiles/Stairs_High.glb";
pub(super) const STAIRS_UP: &str = "Tiles/Stairs_Low.glb";
/// Sunk this far, the top step of the way down is level with the floor.
pub(super) const STAIRS_DOWN_DEPTH: f32 = 5.0;

/// Floors, walls and arches are stretched this much across the floor, so
/// neighboring pieces overlap and close the seams, and this much upward.
pub(super) const STRETCH: f32 = 1.04;
pub(super) const STRETCH_UP: f32 = 1.1;

/// Chance that a wall panel facing a room or corridor gets a torch.
pub(super) const TORCH_CHANCE: f64 = 0.08;
/// Never more torches than this, so the lights stay cheap.
pub(super) const MAX_TORCHES: usize = 40;

pub(super) const WOOD: Vec3 = Vec3::new(0.55, 0.38, 0.22);
pub(super) const IRON: Vec3 = Vec3::new(0.45, 0.45, 0.5);
pub(super) const BONE: Vec3 = Vec3::new(0.85, 0.82, 0.72);
pub(super) const CLOTH: Vec3 = Vec3::new(0.6, 0.15, 0.12);
pub(super) const GLASS: Vec3 = Vec3::new(0.35, 0.6, 0.7);
pub(super) const PAPER: Vec3 = Vec3::new(0.8, 0.75, 0.6);

/// What a room is for, and the furniture that says so: model, tint.
pub(super) const ROOM_KINDS: &[&[(&str, Vec3)]] = &[
	// Storeroom
	&[
		("Furniture/Barrel_Closed.glb", WOOD),
		("Furniture/Barrel_Open.glb", WOOD),
		("Furniture/Barrel_Big.glb", WOOD),
		("Furniture/Bag.glb", CLOTH),
		("Furniture/Bucket.glb", WOOD),
		("Objects/FirewoodStack.glb", WOOD),
	],
	// Dining hall
	&[
		("Furniture/Table_Big.glb", WOOD),
		("Furniture/Bench.glb", WOOD),
		("Furniture/Chair.glb", WOOD),
		("Furniture/Stool_round.glb", WOOD),
		("Furniture/Amphora.glb", Vec3::new(0.7, 0.45, 0.3)),
	],
	// Study
	&[
		("Furniture/Desk.glb", WOOD),
		("Furniture/Table_Small.glb", WOOD),
		("Furniture/Chair.glb", WOOD),
		("Objects/Scroll_Stack.glb", PAPER),
		("Objects/Book_Big_Red.glb", CLOTH),
		("Furniture/Carpet_Red.glb", CLOTH),
	],
	// Alchemy lab
	&[
		("Furniture/Cauldron_Full.glb", IRON),
		("Furniture/Table_Small.glb", WOOD),
		("Objects/CrystalBall_Stand.glb", GLASS),
		("Objects/Jar_Full.glb", GLASS),
		("Objects/Beaker_Full_Var1.glb", GLASS),
	],
	// Armory
	&[
		("Furniture/WeaponRack_Big.glb", WOOD),
		("Furniture/WeaponRack_Small.glb", WOOD),
		("Objects/Shield.glb", IRON),
		("Objects/Spear.glb", IRON),
	],
	// Crypt
	&[("Objects/Skull.glb", BONE), ("Objects/Bone.glb", BONE), ("Furniture/Amphora_Stand.glb", BONE)],
];

/// Treasure in the last room, by the way down.
pub(super) const CHEST: (&str, Vec3) = ("Furniture/Chest_Closed.glb", WOOD);

/// Loads a kit model at its real size and tints it.
pub(super) fn load_kit_model(name: &str, tint: Vec3) -> Result<Mesh, String> {
	let mut mesh = Mesh::load(name)?;
	for v in &mut mesh.vertices {
		v.color *= tint;
	}
	Ok(mesh)
}

/// Turn about Y that points a north-facing kit piece toward DIRECTIONS[dir].
pub(super) fn facing_yaw(dir: usize) -> f32 {
	// North (-Z) needs none; each step clockwise (east, south, west) is a
	// quarter turn to the right, which is negative about +Y
	-(dir as f32) * std::f32::consts::FRAC_PI_2
}

/// Random floor tile inside a room, avoiding `taken` tiles. None if the room is full.
pub(super) fn free_tile(rng: &mut impl Rng, room: &Room, taken: &[(i32, i32)]) -> Option<(i32, i32)> {
	for _ in 0..32 {
		let tile = (rng.random_range(room.x1..room.x2), rng.random_range(room.y1..room.y2));
		if !taken.contains(&tile) {
			return Some(tile);
		}
	}
	None
}

/// Loads models once and places copies of them.
pub(super) struct Builder {
	pub(super) meshes: Vec<Mesh>,
	/// (model, tint) -> index into meshes
	pub(super) loaded: Vec<((&'static str, [u32; 3]), usize)>,
	pub(super) objects: Vec<SceneObject>,
}

impl Builder {
	pub(super) fn model(&mut self, name: &'static str, tint: Vec3) -> Result<usize, String> {
		let key = (name, tint.to_array().map(f32::to_bits));
		if let Some(&(_, i)) = self.loaded.iter().find(|(k, _)| *k == key) {
			return Ok(i);
		}
		self.meshes.push(load_kit_model(name, tint)?);
		self.loaded.push((key, self.meshes.len() - 1));
		Ok(self.meshes.len() - 1)
	}

	/// Places a model at `at` (world space), turned `yaw` about Y.
	pub(super) fn put(&mut self, mesh: usize, at: Vec3, yaw: f32) {
		self.objects.push(SceneObject { mesh, transform: Mat4::from_translation(at) * Mat4::from_rotation_y(yaw) });
	}

	/// Like `put`, but stretched a little across the floor so the piece
	/// overlaps its neighbors and no seams show between tiles.
	pub(super) fn put_stretched(&mut self, mesh: usize, at: Vec3, yaw: f32) {
		let scale = Mat4::from_scale(Vec3::new(STRETCH, STRETCH_UP, STRETCH));
		self.objects.push(SceneObject {
			mesh,
			transform: Mat4::from_translation(at) * Mat4::from_rotation_y(yaw) * scale,
		});
	}
}

impl World {
	/// Builds a level. The player starts in the first room, by the up staircase
	/// when `depth` > 0; the down staircase is in the last room.
	pub(super) fn generate(rng: &mut impl Rng, depth: usize) -> Result<Self, String> {
		let (map, rooms) = generate_dungeon_with_rooms(rng, MAP_WIDTH, MAP_HEIGHT, MAX_DEPTH, MAX_ROOMS, ROOM_MIN, ROOM_MAX);
		if rooms.len() < 2 {
			return Err("the generator made fewer than 2 rooms, so there's nowhere for the stairs".into());
		}
		let start = rooms[0].center();
		let theme = if depth % 2 == 0 { &DUNGEON } else { &BASEMENT };
		let in_room = |x: i32, y: i32| rooms.iter().any(|r| x >= r.x1 && x < r.x2 && y >= r.y1 && y < r.y2);

		let mut b = Builder { meshes: Vec::new(), loaded: Vec::new(), objects: Vec::new() };
		let floor = b.model(theme.floor, theme.stone)?;
		let walls = theme.walls.iter().map(|&w| b.model(w, theme.stone)).collect::<Result<Vec<_>, _>>()?;
		let passage = b.model(theme.passage, theme.stone)?;
		let pillar = b.model(PILLAR, theme.stone)?;
		let torch = b.model(TORCH, WOOD)?;
		let stairs_down_model = b.model(STAIRS_DOWN, theme.stone)?;
		let stairs_up_model = b.model(STAIRS_UP, theme.stone)?;

		// The way down is in the last room, kept off the room's center so the
		// corridor in doesn't land you on it by accident
		let last = rooms.len() - 1;
		let stairs_down = free_tile(rng, &rooms[last], &[rooms[last].center()]).unwrap_or(rooms[last].center());
		let stairs_up = (depth > 0).then_some(start);

		let mut lights = Vec::new();
		let mut blockers = Vec::new();
		for x in 0..MAP_WIDTH as i32 {
			for y in 0..MAP_HEIGHT as i32 {
				if map.is_wall(x, y) {
					continue;
				}
				let center = tile_center(x, y);
				if (x, y) == stairs_down {
					// No floor: a flight of steps going down into the dark
					b.put(stairs_down_model, center - Vec3::Y * STAIRS_DOWN_DEPTH, facing_yaw(open_facing(&map, (x, y, 0))));
				} else {
					b.put_stretched(floor, center, 0.0);
				}

				for (dir, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
					let (nx, ny) = (x + dx, y + dy);
					if map.is_wall(nx, ny) {
						// Mostly the plain panel, sometimes a variant
						let wall = if rng.random_bool(0.7) { walls[0] } else { walls[rng.random_range(0..walls.len())] };
						b.put_stretched(wall, center, facing_yaw(dir));
						if lights.len() < MAX_TORCHES && rng.random_bool(TORCH_CHANCE) {
							// The torch model stands at its own height with its back at
							// x = 0, sticking out along +X; turn +X to point into the tile
							// and set its back on the wall's face
							let out = Vec3::new(dx as f32, 0.0, dy as f32);
							let face = center + out * (TILE / 2.0 * STRETCH - WALL_THICKNESS);
							b.put(torch, face, (out.z).atan2(-out.x));
							lights.push(face - out * 0.5 + Vec3::Y * 2.6);
							blockers.push(Blocker::on_edge(
								center,
								out,
								(-0.2, 0.2),
								(TILE / 2.0 - WALL_THICKNESS - TORCH_DEPTH, TILE / 2.0),
							));
						}
					} else if dir < 2 && in_room(x, y) != in_room(nx, ny) {
						// A corridor opens into a room: an arch on the shared edge.
						// Only east and north edges, so each is built once.
						let out = Vec3::new(dx as f32, 0.0, dy as f32);
						b.put_stretched(passage, center + out * (TILE / 2.0), facing_yaw(dir));
						// The stone on either side of the opening, on both sides of the edge
						let half = TILE / 2.0;
						for span in [(-half, -ARCH_OPENING), (ARCH_OPENING, half)] {
							blockers.push(Blocker::on_edge(center, out, span, (half - 0.35, half + 0.35)));
						}
					}
				}

				// Pillars cover the corners where wall panels meet. Inside a
				// room's corner both sides are wall. Where a corridor turns or
				// branches, both sides are open but the diagonal is rock, and
				// two panels' ends meet at the tile's corner point; only the
				// tile diagonally across from the rock sees it that way, so
				// each such corner gets one pillar
				for (a, c) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
					let (ax, ay) = DIRECTIONS[a];
					let (cx, cy) = DIRECTIONS[c];
					let (wall_a, wall_c) = (map.is_wall(x + ax, y + ay), map.is_wall(x + cx, y + cy));
					let offset = Vec3::new((ax + cx) as f32, 0.0, (ay + cy) as f32);
					if wall_a && wall_c {
						b.put(pillar, center + offset * (TILE / 2.0 - 0.2), 0.0);
					} else if !wall_a && !wall_c && map.is_wall(x + ax + cx, y + ay + cy) {
						b.put(pillar, center + offset * (TILE / 2.0), 0.0);
					}
				}
			}
		}
		if let Some(up) = stairs_up {
			b.put(stairs_up_model, tile_center(up.0, up.1), facing_yaw(open_facing(&map, (up.0, up.1, 0))));
		}

		// Furnish every room but the first, where the player starts. Each room
		// gets a purpose and a few pieces of its furniture. Props are scenery
		// for now: they don't block movement. Deeper levels get more, and
		// tougher, monsters.
		let enemy_chance = (0.5 + depth as f64 * 0.1).min(1.0);
		let mut enemies: Vec<Enemy> = Vec::new();
		let mut items: Vec<GroundItem> = Vec::new();
		for (i, room) in rooms.iter().enumerate().skip(1) {
			let mut taken = vec![start, stairs_down];
			let kind = ROOM_KINDS[rng.random_range(0..ROOM_KINDS.len())];
			let area = ((room.x2 - room.x1) * (room.y2 - room.y1)) as usize;
			for _ in 0..(area / 6).clamp(2, 7) {
				let Some(t) = free_tile(rng, room, &taken) else { break };
				taken.push(t);
				let (name, tint) = kind[rng.random_range(0..kind.len())];
				let model = b.model(name, tint)?;
				let yaw = rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2;
				b.put(model, tile_center(t.0, t.1), yaw);
			}
			if i == last {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let chest = b.model(CHEST.0, CHEST.1)?;
					let yaw = rng.random_range(0..4) as f32 * std::f32::consts::FRAC_PI_2;
					b.put(chest, tile_center(t.0, t.1), yaw);
				}
			}
			if rng.random_bool(ITEM_CHANCE) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let total: u32 = ItemKind::WEIGHTS.iter().sum();
					let mut roll = rng.random_range(0..total);
					let kind = ItemKind::ALL
						.into_iter()
						.zip(ItemKind::WEIGHTS)
						.find(|&(_, w)| {
							let hit = roll < w;
							roll = roll.saturating_sub(w);
							hit
						})
						.map_or(ItemKind::Potion, |(k, _)| k);
					items.push(GroundItem { id: items.len() as u32, x: t.0, y: t.1, kind });
				}
			}
			if rng.random_bool(enemy_chance) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					let kind = EnemyKind::pick(rng, depth);
					enemies.push(Enemy::new(rng, enemies.len() as u32, kind, t.0, t.1));
				}
			}
		}

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
			enemies,
			items,
			hero: Hero::default(),
			combat: false,
			log: Vec::new(),
		};
		world.place_player((start.0, start.1, z));
		Ok(world)
	}
}
