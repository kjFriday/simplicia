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

// The level is built from the modular dungeon kit in models/Tiles: 4 x 4 m
// pieces centered on their origin with the floor at y = 0. Wall panels stand
// on a tile's north (-Z) edge and are turned to face the other sides.

/// A look for a level: which kit pieces build it, and how they're tinted
/// (the kit's shared texture isn't bundled, so color comes from tints).
struct Theme {
	/// Floor and ceiling for one walkable tile
	floor: &'static str,
	/// Wall panels; the first is the most common
	walls: &'static [&'static str],
	/// An arch where a corridor opens into a room
	passage: &'static str,
	stone: Vec3,
}

const DUNGEON: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Dungeon_Wall_Var1.glb", "Tiles/Dungeon_Wall_Var2.glb", "Tiles/Dungeon_Wall_Var3.glb"],
	passage: "Tiles/Dungeon_Passage.glb",
	stone: Vec3::new(0.62, 0.6, 0.58),
};

const BASEMENT: Theme = Theme {
	floor: "Tiles/Dungeon_Custom_Center.glb",
	walls: &["Tiles/Basement_Wall_Var1.glb", "Tiles/Basement_Wall_Var2.glb", "Tiles/Basement_Wall_Var3.glb"],
	passage: "Tiles/Basement_Passage.glb",
	stone: Vec3::new(0.55, 0.5, 0.44),
};

const PILLAR: &str = "Tiles/Pillar.glb";
const TORCH: &str = "Lights/Torch_Wall.glb";
/// The way down sinks a full flight below the floor; the way up stands on it.
const STAIRS_DOWN: &str = "Tiles/Stairs_High.glb";
const STAIRS_UP: &str = "Tiles/Stairs_Low.glb";
/// Sunk this far, the top step of the way down is level with the floor.
const STAIRS_DOWN_DEPTH: f32 = 5.0;

/// Floors, walls and arches are stretched this much across the floor, so
/// neighboring pieces overlap and close the seams, and this much upward.
const STRETCH: f32 = 1.04;
const STRETCH_UP: f32 = 1.1;

/// Chance that a wall panel facing a room or corridor gets a torch.
const TORCH_CHANCE: f64 = 0.08;
/// Never more torches than this, so the lights stay cheap.
const MAX_TORCHES: usize = 40;

const WOOD: Vec3 = Vec3::new(0.55, 0.38, 0.22);
const IRON: Vec3 = Vec3::new(0.45, 0.45, 0.5);
const BONE: Vec3 = Vec3::new(0.85, 0.82, 0.72);
const CLOTH: Vec3 = Vec3::new(0.6, 0.15, 0.12);
const GLASS: Vec3 = Vec3::new(0.35, 0.6, 0.7);
const PAPER: Vec3 = Vec3::new(0.8, 0.75, 0.6);

/// What a room is for, and the furniture that says so: model, tint.
const ROOM_KINDS: &[&[(&str, Vec3)]] = &[
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
const CHEST: (&str, Vec3) = ("Furniture/Chest_Closed.glb", WOOD);
const ENEMY: (&str, f32) = ("characters/Slime Enemy.glb", 2.4);

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
	/// Where the feet are, in world space. In combat it's always the center of
	/// tile (x, y); walking freely it can be anywhere, and (x, y) is the tile
	/// it's in.
	pos: Vec3,
}

/// Furthest you can look up or down.
const MAX_PITCH: f32 = 80.0 * std::f32::consts::PI / 180.0;

/// Free walking speed, meters per second.
const WALK_SPEED: f32 = 5.0;
/// How close the player's center can get to a wall, in meters.
const PLAYER_RADIUS: f32 = 0.6;

impl Player {
	fn new(x: i32, y: i32, z: i32) -> Self {
		Self { x, y, z, facing: 0, look_yaw: 0.0, look_pitch: 0.0, pos: tile_center(x, y) }
	}

	/// Where the player looks, flat on the floor: the facing turned by mouselook.
	fn heading(&self) -> Vec3 {
		let (dx, dy) = self.forward();
		glam::Quat::from_rotation_y(-self.look_yaw) * Vec3::new(dx as f32, 0.0, dy as f32)
	}

	/// Puts the feet back in the middle of the current tile and squares the
	/// view to the facing, for turn-by-turn play.
	fn snap_to_tile(&mut self) {
		self.pos = tile_center(self.x, self.y);
		self.look_yaw = 0.0;
	}

	/// True when a circle of PLAYER_RADIUS at `pos` overlaps a wall tile.
	fn blocked(map: &MapGrid, pos: Vec3) -> bool {
		let r = PLAYER_RADIUS;
		[(-r, -r), (r, -r), (-r, r), (r, r)].iter().any(|(ox, oz)| {
			let tx = ((pos.x + ox) / TILE).floor() as i32;
			let ty = ((pos.z + oz) / TILE).floor() as i32;
			map.is_wall(tx, ty)
		})
	}

	/// Walks freely for `dt` seconds: `forward` and `strafe` are -1 to 1.
	/// Each axis moves on its own, so the player slides along walls.
	fn walk(&mut self, map: &MapGrid, forward: f32, strafe: f32, dt: f32) {
		let ahead = self.heading();
		let right = Vec3::new(-ahead.z, 0.0, ahead.x);
		let step = (ahead * forward + right * strafe).normalize_or_zero() * WALK_SPEED * dt;
		for axis in [Vec3::X, Vec3::Z] {
			let next = self.pos + step * axis;
			if !Self::blocked(map, next) {
				self.pos = next;
			}
		}
		self.x = (self.pos.x / TILE).floor() as i32;
		self.y = (self.pos.z / TILE).floor() as i32;
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
		self.pos + Vec3::new(0.0, EYE_HEIGHT, 0.0)
	}

	pub(super) fn view_matrix(&self) -> Mat4 {
		// Map east (+X) is to the right of north (-Z), so turning right is a
		// negative rotation about +Y
		let dir = self.heading();
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
		self.pos = tile_center(nx, ny);
		true
	}
}

/// The generated dungeon: the map for movement rules, and what to draw.
pub(super) struct World {
	pub(super) map: MapGrid,
	pub(super) player: Player,
	pub(super) meshes: Vec<Mesh>,
	pub(super) objects: Vec<SceneObject>,
	/// Torch flames: where to put a point light
	pub(super) lights: Vec<Vec3>,
	/// 0 is the top level; each staircase down adds one
	pub(super) depth: usize,
	/// Stepping here takes you one level deeper
	pub(super) stairs_down: (i32, i32, i32),
	/// Stepping here takes you back up; none on the top level
	pub(super) stairs_up: Option<(i32, i32, i32)>,
	/// Tiles the minimap shows, indexed [x * height + y]
	seen: Vec<bool>,
	/// Living enemies
	pub(super) enemies: Vec<Enemy>,
	/// Index into `meshes` of the enemy model
	pub(super) enemy_mesh: usize,
	pub(super) hp: i32,
	/// True while an enemy is close: movement is tile by tile, turn by turn
	pub(super) combat: bool,
	/// What happened lately, newest last
	pub(super) log: Vec<String>,
}

/// A monster on the map. `id` stays the same for its whole life, so the 3D
/// view can match its model to it.
pub(super) struct Enemy {
	pub(super) id: u32,
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) hp: i32,
}

/// Enemies this close (in tiles, counting diagonals) start combat.
const COMBAT_RANGE: i32 = 4;
pub(super) const PLAYER_HP: i32 = 10;
const ENEMY_HP: i32 = 3;
const PLAYER_DAMAGE: i32 = 2;
const ENEMY_DAMAGE: i32 = 1;
/// Lines of the log kept.
const LOG_LINES: usize = 4;

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

	/// Where the player's feet are, in world space.
	pub(super) fn player_pos(&self) -> Vec3 {
		self.player.pos
	}

	/// The middle of a tile's floor, in world space.
	pub(super) fn tile_center_of(&self, x: i32, y: i32) -> Vec3 {
		tile_center(x, y)
	}

	/// Whether the player can see what's on a tile right now, so the map
	/// shows enemies there: anything within twice the reveal radius.
	pub(super) fn can_see(&self, x: i32, y: i32) -> bool {
		(x - self.player.x).abs().max((y - self.player.y).abs()) <= REVEAL_RADIUS * 2
	}

	fn say(&mut self, line: impl Into<String>) {
		self.log.push(line.into());
		if self.log.len() > LOG_LINES {
			self.log.remove(0);
		}
	}

	fn enemy_at(&self, x: i32, y: i32) -> Option<usize> {
		self.enemies.iter().position(|e| (e.x, e.y) == (x, y))
	}

	/// Enemies close enough to fight.
	fn near(&self, e: &Enemy) -> bool {
		(e.x - self.player.x).abs().max((e.y - self.player.y).abs()) <= COMBAT_RANGE
	}

	/// Starts or ends combat as enemies come and go. Starting it snaps the
	/// player to the middle of their tile.
	pub(super) fn update_combat(&mut self) {
		let combat = self.enemies.iter().any(|e| self.near(e));
		if combat && !self.combat {
			self.player.snap_to_tile();
			self.say("An enemy approaches!");
		} else if !combat && self.combat {
			self.say("The way is clear.");
		}
		self.combat = combat;
	}

	/// Walks freely for `dt` seconds, outside combat. Says whether the player
	/// just stepped onto a staircase.
	pub(super) fn walk(&mut self, forward: f32, strafe: f32, dt: f32) -> Option<Arrival> {
		let before = (self.player.x, self.player.y);
		self.player.walk(&self.map, forward, strafe, dt);
		if (self.player.x, self.player.y) == before {
			return None;
		}
		self.reveal_around_player();
		self.update_combat();
		self.arrival()
	}

	/// What's under the player now.
	fn arrival(&self) -> Option<Arrival> {
		let here = (self.player.x, self.player.y);
		Some(if here == (self.stairs_down.0, self.stairs_down.1) {
			Arrival::StairsDown
		} else if Some(here) == self.stairs_up.map(|s| (s.0, s.1)) {
			Arrival::StairsUp
		} else {
			Arrival::Floor
		})
	}

	/// The enemies' turn in combat: each one close by attacks if it's next to
	/// the player, or else steps toward them.
	fn enemies_act(&mut self) {
		let (px, py) = (self.player.x, self.player.y);
		for i in 0..self.enemies.len() {
			if !self.near(&self.enemies[i]) {
				continue;
			}
			let (ex, ey) = (self.enemies[i].x, self.enemies[i].y);
			let (dx, dy) = (px - ex, py - ey);
			if dx.abs() + dy.abs() == 1 {
				self.hp -= ENEMY_DAMAGE;
				self.say(format!("The slime hits you ({} HP).", self.hp.max(0)));
				continue;
			}
			// Step along the longer way first, then the other, if it's free
			let steps = if dx.abs() >= dy.abs() {
				[(dx.signum(), 0), (0, dy.signum())]
			} else {
				[(0, dy.signum()), (dx.signum(), 0)]
			};
			for (sx, sy) in steps {
				let (nx, ny) = (ex + sx, ey + sy);
				if (sx, sy) != (0, 0) && !self.map.is_wall(nx, ny) && self.enemy_at(nx, ny).is_none() && (nx, ny) != (px, py) {
					self.enemies[i].x = nx;
					self.enemies[i].y = ny;
					break;
				}
			}
		}
	}

	/// Passes an action to the player, uncovers the map around them, and says
	/// whether they just stepped onto a staircase. In combat, stepping into an
	/// enemy attacks it, and every step or attack gives the enemies a turn.
	pub(super) fn act(&mut self, action: Action) -> Option<Arrival> {
		if self.combat && matches!(action, Action::Forward | Action::Back) {
			let (dx, dy) = self.player.forward();
			let sign = if action == Action::Forward { 1 } else { -1 };
			let target = (self.player.x + dx * sign, self.player.y + dy * sign);
			if let Some(i) = self.enemy_at(target.0, target.1) {
				self.enemies[i].hp -= PLAYER_DAMAGE;
				if self.enemies[i].hp <= 0 {
					self.enemies.remove(i);
					self.say("You slay the slime!");
				} else {
					self.say("You hit the slime.");
				}
				self.enemies_act();
				self.update_combat();
				return None;
			}
		}

		let before = (self.player.x, self.player.y);
		self.player.act(action, &self.map);
		let here = (self.player.x, self.player.y);
		if here == before {
			// Turning in place, or bumping a wall, never triggers stairs or
			// costs a turn
			return None;
		}
		self.reveal_around_player();
		if self.combat {
			self.enemies_act();
		}
		self.update_combat();
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
		self.player.pos = tile_center(self.player.x, self.player.y);

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

/// Loads a kit model at its real size and tints it.
fn load_kit_model(name: &str, tint: Vec3) -> Result<Mesh, String> {
	let mut mesh = Mesh::load(name)?;
	for v in &mut mesh.vertices {
		v.color *= tint;
	}
	Ok(mesh)
}

/// Turn about Y that points a north-facing kit piece toward DIRECTIONS[dir].
fn facing_yaw(dir: usize) -> f32 {
	// North (-Z) needs none; each step clockwise (east, south, west) is a
	// quarter turn to the right, which is negative about +Y
	-(dir as f32) * std::f32::consts::FRAC_PI_2
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

/// Loads models once and places copies of them.
struct Builder {
	meshes: Vec<Mesh>,
	/// (model, tint) -> index into meshes
	loaded: Vec<((&'static str, [u32; 3]), usize)>,
	objects: Vec<SceneObject>,
}

impl Builder {
	fn model(&mut self, name: &'static str, tint: Vec3) -> Result<usize, String> {
		let key = (name, tint.to_array().map(f32::to_bits));
		if let Some(&(_, i)) = self.loaded.iter().find(|(k, _)| *k == key) {
			return Ok(i);
		}
		self.meshes.push(load_kit_model(name, tint)?);
		self.loaded.push((key, self.meshes.len() - 1));
		Ok(self.meshes.len() - 1)
	}

	/// Places a model at `at` (world space), turned `yaw` about Y.
	fn put(&mut self, mesh: usize, at: Vec3, yaw: f32) {
		self.objects.push(SceneObject { mesh, transform: Mat4::from_translation(at) * Mat4::from_rotation_y(yaw) });
	}

	/// Like `put`, but stretched a little across the floor so the piece
	/// overlaps its neighbors and no seams show between tiles.
	fn put_stretched(&mut self, mesh: usize, at: Vec3, yaw: f32) {
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
							// On the panel's face, a hand above head height
							let out = Vec3::new(dx as f32, 0.0, dy as f32);
							b.put(torch, center + out * (TILE / 2.0 - 0.25) + Vec3::Y * 2.2, facing_yaw(dir) + std::f32::consts::PI);
							lights.push(center + out * (TILE / 2.0 - 0.6) + Vec3::Y * 2.9);
						}
					} else if dir < 2 && in_room(x, y) != in_room(nx, ny) {
						// A corridor opens into a room: an arch on the shared edge.
						// Only east and north edges, so each is built once.
						let edge = center + Vec3::new(dx as f32, 0.0, dy as f32) * (TILE / 2.0);
						b.put_stretched(passage, edge, facing_yaw(dir));
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
		// for now: they don't block movement. Deeper levels get more slimes.
		let enemy = {
			b.meshes.push(load_tile_model(ENEMY.0, ENEMY.1)?);
			b.meshes.len() - 1
		};
		let enemy_chance = (0.5 + depth as f64 * 0.1).min(1.0);
		let mut enemies: Vec<Enemy> = Vec::new();
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
			if rng.random_bool(enemy_chance) {
				if let Some(t) = free_tile(rng, room, &taken) {
					taken.push(t);
					enemies.push(Enemy { id: enemies.len() as u32, x: t.0, y: t.1, hp: ENEMY_HP });
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
			enemies,
			enemy_mesh: enemy,
			hp: PLAYER_HP,
			combat: false,
			log: Vec::new(),
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
