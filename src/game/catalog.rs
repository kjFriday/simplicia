// The asset catalog: every furniture, object, and light model in the
// game, sorted by where it naturally goes, which kinds of room it suits, and
// whether other things can stand on it. It's built at startup by scanning
// the bundled models; names are matched against RULES, and anything not
// listed is guessed from its size.
//
// Positions in the catalog are "normalized": every model is measured, and
// placing one at a point puts the middle of its footprint there with its
// lowest point on the floor, whatever its own origin is.
use glam::Vec3;

use crate::engine::mesh::Mesh;
use crate::engine::models::MODEL_FILES;

/// Folders scanned for placeable things.
const FOLDERS: [&str; 3] = ["Furniture/", "Objects/", "Lights/"];

/// What a room is for. Each one draws from its own set of assets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RoomTheme {
	Storeroom,
	Dining,
	Study,
	Alchemy,
	Armory,
	Crypt,
}

impl RoomTheme {
	pub(super) const ALL: [RoomTheme; 6] = [
		RoomTheme::Storeroom,
		RoomTheme::Dining,
		RoomTheme::Study,
		RoomTheme::Alchemy,
		RoomTheme::Armory,
		RoomTheme::Crypt,
	];
}

/// Where an asset lives.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Mount {
	/// Stands on the floor. `wall`: belongs with its back to a wall.
	/// `flat`: lies flat (carpets), so others can stand on it.
	Floor { wall: bool, flat: bool },
	/// Hangs on a wall, at its own height
	Wall,
	/// Hangs from the ceiling
	Ceiling,
	/// Small: sits on top of a table, shelf, barrel, ...
	Surface,
}

/// One catalog rule: a model-name prefix and what it means.
struct Rule {
	prefix: &'static str,
	mount: Mount,
	themes: &'static [RoomTheme],
	tint: Vec3,
	/// Other things can be set on its top
	holds: bool,
	light: bool,
}

const WOOD: Vec3 = Vec3::new(0.55, 0.38, 0.22);
const IRON: Vec3 = Vec3::new(0.45, 0.45, 0.5);
const BONE: Vec3 = Vec3::new(0.85, 0.82, 0.72);
const CLOTH: Vec3 = Vec3::new(0.6, 0.15, 0.12);
const MOSS: Vec3 = Vec3::new(0.2, 0.45, 0.2);
const GLASS: Vec3 = Vec3::new(0.35, 0.6, 0.7);
const PAPER: Vec3 = Vec3::new(0.8, 0.75, 0.6);
const CLAY: Vec3 = Vec3::new(0.7, 0.45, 0.3);
const GOLD: Vec3 = Vec3::new(0.9, 0.75, 0.3);
const WAX: Vec3 = Vec3::new(0.95, 0.9, 0.75);

use Mount::*;
use RoomTheme::*;

const FLOOR: Mount = Floor { wall: false, flat: false };
const BY_WALL: Mount = Floor { wall: true, flat: false };
const FLAT: Mount = Floor { wall: false, flat: true };
const ANY: &[RoomTheme] = &[Storeroom, Dining, Study, Alchemy, Armory, Crypt];

/// Longer prefixes first where one prefix starts another (Barrel_Big before Barrel_).
const RULES: &[Rule] = &[
	// Furniture
	Rule { prefix: "Amphora_Stand", mount: FLOOR, themes: &[Crypt, Storeroom], tint: CLAY, holds: true, light: false },
	Rule { prefix: "Amphora", mount: FLOOR, themes: &[Storeroom, Crypt, Dining], tint: CLAY, holds: false, light: false },
	Rule { prefix: "Bag", mount: BY_WALL, themes: &[Storeroom], tint: CLOTH, holds: false, light: false },
	Rule { prefix: "Barrel_Big", mount: BY_WALL, themes: &[Storeroom], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Barrel_Closed", mount: FLOOR, themes: &[Storeroom, Dining], tint: WOOD, holds: true, light: false },
	Rule { prefix: "Barrel_Open", mount: FLOOR, themes: &[Storeroom], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Bench", mount: FLOOR, themes: &[Dining], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Bucket", mount: FLOOR, themes: &[Storeroom, Alchemy], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Carpet_Green", mount: FLAT, themes: &[Study, Alchemy], tint: MOSS, holds: false, light: false },
	Rule { prefix: "Carpet_Red", mount: FLAT, themes: &[Study, Dining], tint: CLOTH, holds: false, light: false },
	Rule { prefix: "Cauldron_Empty", mount: FLOOR, themes: &[Alchemy], tint: IRON, holds: false, light: false },
	Rule { prefix: "Cauldron_Full", mount: FLOOR, themes: &[Alchemy], tint: IRON, holds: false, light: false },
	Rule { prefix: "Chair", mount: FLOOR, themes: &[Dining, Study], tint: WOOD, holds: false, light: false },
	Rule { prefix: "ChestSmall", mount: FLOOR, themes: &[Storeroom, Crypt], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Chest_", mount: BY_WALL, themes: &[Storeroom, Crypt, Armory], tint: WOOD, holds: false, light: false },
	Rule { prefix: "CoatHook", mount: Wall, themes: &[Storeroom, Armory], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Desk", mount: BY_WALL, themes: &[Study], tint: WOOD, holds: true, light: false },
	Rule { prefix: "Fireplace", mount: BY_WALL, themes: &[Dining, Study], tint: Vec3::new(0.5, 0.48, 0.46), holds: false, light: true },
	Rule { prefix: "Shelf_Wall", mount: Wall, themes: &[Study, Alchemy, Storeroom], tint: WOOD, holds: true, light: false },
	Rule { prefix: "Shelf", mount: BY_WALL, themes: &[Study, Storeroom, Alchemy], tint: WOOD, holds: true, light: false },
	Rule { prefix: "Stool", mount: FLOOR, themes: &[Dining, Study, Alchemy], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Table_Big", mount: FLOOR, themes: &[Dining], tint: WOOD, holds: true, light: false },
	Rule { prefix: "Table_Small", mount: FLOOR, themes: &[Study, Alchemy, Dining], tint: WOOD, holds: true, light: false },
	Rule { prefix: "WeaponRack", mount: BY_WALL, themes: &[Armory], tint: WOOD, holds: false, light: false },
	// Objects
	Rule { prefix: "Beaker", mount: Surface, themes: &[Alchemy], tint: GLASS, holds: false, light: false },
	Rule { prefix: "Bone", mount: FLOOR, themes: &[Crypt], tint: BONE, holds: false, light: false },
	Rule { prefix: "Book", mount: Surface, themes: &[Study], tint: CLOTH, holds: false, light: false },
	Rule { prefix: "Cauldron_Small", mount: Surface, themes: &[Alchemy], tint: IRON, holds: false, light: false },
	Rule { prefix: "Chandelier_Small", mount: Surface, themes: &[Study, Dining], tint: WAX, holds: false, light: true },
	Rule { prefix: "CrystalBall", mount: Surface, themes: &[Alchemy], tint: GLASS, holds: false, light: false },
	Rule { prefix: "FirewoodStack", mount: BY_WALL, themes: &[Storeroom, Dining], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Firewood", mount: FLOOR, themes: &[Storeroom], tint: WOOD, holds: false, light: false },
	Rule { prefix: "Flask", mount: Surface, themes: &[Alchemy], tint: GLASS, holds: false, light: false },
	Rule { prefix: "Fork", mount: Surface, themes: &[Dining], tint: IRON, holds: false, light: false },
	Rule { prefix: "Inkwell", mount: Surface, themes: &[Study], tint: GLASS, holds: false, light: false },
	Rule { prefix: "Jar", mount: Surface, themes: &[Alchemy, Storeroom], tint: GLASS, holds: false, light: false },
	Rule { prefix: "Key", mount: Surface, themes: ANY, tint: GOLD, holds: false, light: false },
	Rule { prefix: "Knife", mount: Surface, themes: &[Dining], tint: IRON, holds: false, light: false },
	Rule { prefix: "Plate", mount: Surface, themes: &[Dining], tint: CLAY, holds: false, light: false },
	Rule { prefix: "Potion", mount: Surface, themes: &[Alchemy], tint: GLASS, holds: false, light: false },
	Rule { prefix: "Quill", mount: Surface, themes: &[Study], tint: PAPER, holds: false, light: false },
	Rule { prefix: "Scroll", mount: Surface, themes: &[Study], tint: PAPER, holds: false, light: false },
	Rule { prefix: "Shield", mount: Wall, themes: &[Armory], tint: IRON, holds: false, light: false },
	Rule { prefix: "Skull", mount: Surface, themes: &[Crypt], tint: BONE, holds: false, light: false },
	Rule { prefix: "Spear", mount: Surface, themes: &[Armory], tint: IRON, holds: false, light: false },
	Rule { prefix: "Sword", mount: Surface, themes: &[Armory], tint: IRON, holds: false, light: false },
	// Lights
	Rule { prefix: "Candle_Wall", mount: Wall, themes: ANY, tint: WAX, holds: false, light: true },
	Rule { prefix: "Chandelier_Ceiling", mount: Ceiling, themes: &[Dining, Study, Crypt], tint: IRON, holds: false, light: true },
	Rule { prefix: "Chandelier_", mount: Surface, themes: &[Dining, Study], tint: WAX, holds: false, light: true },
	Rule { prefix: "Torch_Wall", mount: Wall, themes: ANY, tint: WOOD, holds: false, light: true },
];

/// Models that aren't placed: handheld, scenery scraps, or building pieces.
const SKIP: [&str; 6] = ["Torch.", "Steps_Small", "Ivy_", "Plant_Root", "Tree_Branch", "Steps"];

/// A placeable asset and everything the generator needs to know about it.
#[derive(Clone, Debug)]
pub(super) struct Asset {
	/// Path under models/
	pub(super) model: &'static str,
	pub(super) mount: Mount,
	pub(super) themes: &'static [RoomTheme],
	pub(super) tint: Vec3,
	/// Width (x), height, and depth (z) when unturned
	pub(super) size: Vec3,
	/// Added to a point to get where the model's origin goes, so the
	/// footprint's middle is on the point and its lowest point on the floor
	pub(super) offset: Vec3,
	/// Things can be set on top of it
	pub(super) holds: bool,
	pub(super) light: bool,
	/// Wall items: true when the model sticks out along +X from its back,
	/// false along -X (+Z/-Z when it's thinner along Z, see `wall_axis_x`)
	pub(super) out_positive: bool,
	/// Wall items: true when it's thin along X (its back faces ±X)
	pub(super) wall_axis_x: bool,
}

pub(super) struct Catalog {
	pub(super) assets: Vec<Asset>,
}

impl Catalog {
	/// Measures and sorts every model under FOLDERS.
	pub(super) fn load() -> Self {
		let mut assets = Vec::new();
		for &(name, _) in MODEL_FILES {
			if !name.ends_with(".glb") || !FOLDERS.iter().any(|f| name.starts_with(f)) {
				continue;
			}
			let stem = name.rsplit('/').next().unwrap_or(name);
			if SKIP.iter().any(|s| stem.starts_with(s)) {
				continue;
			}
			let Ok(mesh) = Mesh::load(name) else { continue };
			let Some((min, max)) = bounds(&mesh) else { continue };
			let size = max - min;
			let rule = RULES.iter().find(|r| stem.starts_with(r.prefix));
			let (mount, themes, tint, holds, light) = match rule {
				Some(r) => (r.mount, r.themes, r.tint, r.holds, r.light),
				// Unlisted: small things go on surfaces, the rest on the floor
				None if size.max_element() < 0.6 => (Surface, ANY, Vec3::splat(0.7), false, false),
				None => (FLOOR, ANY, Vec3::splat(0.7), false, false),
			};
			let center = (min + max) / 2.0;
			// Wall pieces are modeled with their origin on their back. The axis
			// the origin sits furthest off-center on is the one they stick out
			// along; if it's near the middle on both, the thinner axis
			let off_x = center.x.abs() / (size.x / 2.0).max(1e-3);
			let off_z = center.z.abs() / (size.z / 2.0).max(1e-3);
			let wall_axis_x = if off_x.max(off_z) > 0.5 { off_x >= off_z } else { size.x <= size.z };
			// The back is the side nearer the model's own origin
			let out_positive = if wall_axis_x { min.x.abs() < max.x.abs() } else { min.z.abs() < max.z.abs() };
			assets.push(Asset {
				model: name,
				mount,
				themes,
				tint,
				size,
				offset: Vec3::new(-center.x, -min.y, -center.z),
				holds,
				light,
				out_positive,
				wall_axis_x,
			});
		}
		Self { assets }
	}

	/// Assets for `theme` that mount like `want`.
	pub(super) fn pick<'a>(&'a self, theme: Option<RoomTheme>, want: impl Fn(&Asset) -> bool + 'a) -> Vec<&'a Asset> {
		self.assets.iter().filter(|a| theme.is_none_or(|t| a.themes.contains(&t)) && want(a)).collect()
	}
}

/// The lowest and highest corner of a mesh, or None if it has no vertices.
pub(super) fn bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
	let mut it = mesh.vertices.iter().map(|v| v.pos);
	let first = it.next()?;
	Some(it.fold((first, first), |(lo, hi), p| (lo.min(p), hi.max(p))))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_catalog_sorts_the_bundled_assets() {
		let c = Catalog::load();
		assert!(c.assets.len() > 60, "only {} assets", c.assets.len());
		let find = |n: &str| c.assets.iter().find(|a| a.model.contains(n)).unwrap();
		assert_eq!(find("Table_Big").mount, FLOOR);
		assert!(find("Table_Big").holds);
		assert_eq!(find("Torch_Wall").mount, Wall);
		assert!(find("Torch_Wall").out_positive, "the wall torch sticks out along +X");
		assert!(!find("Candle_Wall").out_positive, "the wall candle sticks out along -X");
		assert_eq!(find("Chandelier_Ceiling").mount, Ceiling);
		assert_eq!(find("Potion_Red").mount, Surface);
		// Off-center models are re-centered: the amphora's origin is far from its middle
		let amphora = find("Furniture/Amphora.glb");
		assert!(amphora.offset.x > 0.5 && amphora.offset.z < -1.5);
	}
}
