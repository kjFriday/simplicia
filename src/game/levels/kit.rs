// The dungeon kit: which models build a level (floors, walls, arches,
// stairs, halls), how they're tinted, and the numbers that size and shape a
// level. Everything else in levels/ reads these: `KitMeshes` loads the
// pieces through builder.rs, shape.rs rolls the room shapes, tiles.rs places
// the pieces, and populate.rs adds the chests.
//
// The kit's pieces are 4 x 4 m, centered on their origin with the floor at
// y = 0. Wall panels stand on a tile's north (-Z) edge and are turned to face
// the other sides.
use glam::Vec3;

use super::builder::Builder;
use crate::engine::mesh::Mesh;

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

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

const PILLAR: &str = "Tiles/Pillar.glb";
const TORCH: &str = "Lights/Torch_Wall.glb";
/// The way down sinks a full flight below the floor; the way up stands on it.
const STAIRS_DOWN: &str = "Tiles/Stairs_High.glb";
const STAIRS_UP: &str = "Tiles/Stairs_Low.glb";
/// Sunk this far, the top step of the way down is level with the floor.
pub(super) const STAIRS_DOWN_DEPTH: f32 = 5.0;
/// One flight of steps, 2.5 m high, for the stair tile into a raised or sunken room
const RAMP_STEPS: &str = "Tiles/Stairs_Low.glb";

/// Tall halls: a 10 m floor-to-ceiling tile, the same with a wall on its
/// north side, and with walls north and east.
const HALL_FLOOR: &str = "Tiles/Dungeon_Hall_Center.glb";
const HALL_WALL: &str = "Tiles/Dungeon_Hall_Wall.glb";
const HALL_CORNER: &str = "Tiles/Dungeon_Hall_Corner.glb";
const BALCONY: &str = "Tiles/Balcony.glb";
const RAILING: &str = "Tiles/Balcony_Railing.glb";
const CHANDELIER: &str = "Lights/Chandelier_Ceiling.glb";
/// A 4 m cube, squashed into ledges and pit walls
const BLOCK: &str = "terrain/canyon/SM_AP_Cube_4m_0.glb";

const WOOD: Vec3 = Vec3::new(0.55, 0.38, 0.22);
/// Chests: closed and open, the chance one turns up in a room (the last room,
/// by the way down, always has one).
pub(super) const CHEST: (&str, Vec3) = ("Furniture/Chest_Closed.glb", WOOD);
pub(super) const CHEST_OPEN: &str = "Furniture/Chest_Open.glb";
pub(super) const CHEST_CHANCE: f64 = 0.25;

// ---------------------------------------------------------------------------
// Sizes and chances
// ---------------------------------------------------------------------------

/// Floors, walls and arches are stretched this much across the floor, so
/// neighboring pieces overlap and close the seams, and this much upward.
pub(super) const STRETCH: f32 = 1.04;
pub(super) const STRETCH_UP: f32 = 1.1;
/// The kit's ordinary wall height. (Its ordinary floor-to-ceiling height is
/// 4.36 m before stretching, which is where the 4.36 in tiles.rs comes from.)
pub(super) const WALL_HEIGHT: f32 = 5.0;
/// Tall halls: where the galleries run, and the ceiling.
pub(super) const GALLERY: f32 = 5.0;
pub(super) const HALL_CEILING: f32 = 9.4;

/// How far raised and sunken rooms sit from their corridors, and how high
/// ledges stand (pits sink the same).
pub(super) const ROOM_RISE: f32 = 2.5;
pub(super) const LEDGE: f32 = 1.2;
/// Chances, per room, of each shape (the rest stay level).
pub(super) const TALL_CHANCE: f64 = 0.2;
pub(super) const RAISED_CHANCE: f64 = 0.15;
pub(super) const SUNKEN_CHANCE: f64 = 0.15;
pub(super) const LEDGE_CHANCE: f64 = 0.35;
/// Share of a room's tiles that get a generated cell's furnishings, picked
/// per room between these; and the chance for a hall tile.
pub(super) const ROOM_DENSITY: (f64, f64) = (0.35, 0.85);
pub(super) const HALL_CELL_CHANCE: f64 = 0.4;

/// Chance that a wall panel facing a room or corridor gets a torch.
pub(super) const TORCH_CHANCE: f64 = 0.08;
/// Never more lights than this, so they stay cheap.
pub(super) const MAX_TORCHES: usize = 40;

// ---------------------------------------------------------------------------
// The kit, loaded
// ---------------------------------------------------------------------------

/// The kit's models, loaded once per level: indices into the builder's meshes.
pub(super) struct KitMeshes {
	pub(super) floor: usize,
	pub(super) walls: Vec<usize>,
	pub(super) passage: usize,
	pub(super) pillar: usize,
	pub(super) torch: usize,
	pub(super) stairs_down: usize,
	pub(super) stairs_up: usize,
	pub(super) ramp_steps: usize,
	/// Tall halls: the open floor, with a wall north, and with walls north and east
	pub(super) hall: [usize; 3],
	pub(super) balcony: usize,
	pub(super) railing: usize,
	pub(super) chandelier: usize,
	/// The squashed cube for ledges and pit faces
	pub(super) block: usize,
}

impl KitMeshes {
	/// Loads the kit's models for `theme`. The order they load in sets their
	/// mesh indices, so it stays the same.
	pub(super) fn load(b: &mut Builder, theme: &Theme) -> Result<Self, String> {
		let floor = b.model(theme.floor, theme.stone)?;
		let walls = theme.walls.iter().map(|&w| b.model(w, theme.stone)).collect::<Result<Vec<_>, _>>()?;
		let passage = b.model(theme.passage, theme.stone)?;
		let pillar = b.model(PILLAR, theme.stone)?;
		let torch = b.model(TORCH, WOOD)?;
		let stairs_down = b.model(STAIRS_DOWN, theme.stone)?;
		let stairs_up = b.model(STAIRS_UP, theme.stone)?;
		let ramp_steps = b.model(RAMP_STEPS, theme.stone)?;
		let hall_floor = b.model(HALL_FLOOR, theme.stone)?;
		let hall_wall = b.model(HALL_WALL, theme.stone)?;
		let hall_corner = b.model(HALL_CORNER, theme.stone)?;
		let balcony = b.model(BALCONY, WOOD)?;
		let railing = b.model(RAILING, WOOD)?;
		let chandelier = b.model(CHANDELIER, Vec3::new(0.45, 0.45, 0.5))?;
		let block = b.model_normalized(BLOCK, theme.stone)?;
		Ok(Self {
			floor,
			walls,
			passage,
			pillar,
			torch,
			stairs_down,
			stairs_up,
			ramp_steps,
			hall: [hall_floor, hall_wall, hall_corner],
			balcony,
			railing,
			chandelier,
			block,
		})
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
