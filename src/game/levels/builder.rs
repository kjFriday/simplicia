// Placing models: the `Builder` loads each model once and places copies of
// it, and keeps track of the furnishings it places (props) for the clipping
// check, stage 5, which takes out anything that ends up inside something
// else or in a wall. tiles.rs and populate.rs place through it; mod.rs hands
// its meshes and objects to the finished `World`.
use glam::{Mat4, Quat, Vec3};

use super::catalog::bounds;
use super::cells::Cell;
use super::kit::{facing_yaw, load_kit_model, MAX_TORCHES, STRETCH};
use crate::dungeons::mapgen::MapGrid;
use crate::engine::mesh::Mesh;
use crate::engine::scene::SceneObject;
use crate::game::play::world::TILE;

/// Something placed to furnish the level, kept track of for the clipping check.
struct Prop {
	/// Index into `Builder::objects`
	object: usize,
	/// Its box in the world
	min: Vec3,
	max: Vec3,
	/// The prop it stands on, which it may touch
	parent: Option<usize>,
	/// Its light, as an index into the level's lights
	light: Option<usize>,
	/// Lies flat (a carpet): others may stand on it
	flat: bool,
	/// Part of the building (a kit torch): never removed, only avoided
	fixed: bool,
}

/// Loads models once and places copies of them.
pub(super) struct Builder {
	pub(super) meshes: Vec<Mesh>,
	/// (model, tint, normalized) -> index into meshes
	loaded: Vec<((String, [u32; 3], bool), usize)>,
	pub(super) objects: Vec<SceneObject>,
	props: Vec<Prop>,
}

impl Builder {
	pub(super) fn new() -> Self {
		Self { meshes: Vec::new(), loaded: Vec::new(), objects: Vec::new(), props: Vec::new() }
	}

	pub(super) fn model(&mut self, name: &str, tint: Vec3) -> Result<usize, String> {
		self.load(name, tint, false)
	}

	/// Like `model`, but moved so its footprint is centered on its origin and
	/// its lowest point is at y = 0.
	pub(super) fn model_normalized(&mut self, name: &str, tint: Vec3) -> Result<usize, String> {
		self.load(name, tint, true)
	}

	fn load(&mut self, name: &str, tint: Vec3, normalize: bool) -> Result<usize, String> {
		let key = (name.to_string(), tint.to_array().map(f32::to_bits), normalize);
		if let Some(&(_, i)) = self.loaded.iter().find(|(k, _)| *k == key) {
			return Ok(i);
		}
		let mut mesh = load_kit_model(name, tint)?;
		if normalize {
			if let Some((lo, hi)) = bounds(&mesh) {
				let shift = Vec3::new(-(lo.x + hi.x) / 2.0, -lo.y, -(lo.z + hi.z) / 2.0);
				for v in &mut mesh.vertices {
					v.pos += shift;
				}
			}
		}
		self.meshes.push(mesh);
		self.loaded.push((key, self.meshes.len() - 1));
		Ok(self.meshes.len() - 1)
	}

	/// Places a model at `at` (world space), turned `yaw` about Y.
	pub(super) fn put(&mut self, mesh: usize, at: Vec3, yaw: f32) -> usize {
		self.put_scaled(mesh, at, yaw, Vec3::ONE)
	}

	/// Places a model scaled by `scale` (in its own axes) after turning it.
	pub(super) fn put_scaled(&mut self, mesh: usize, at: Vec3, yaw: f32, scale: Vec3) -> usize {
		let transform = Mat4::from_translation(at) * Mat4::from_rotation_y(yaw) * Mat4::from_scale(scale);
		self.objects.push(SceneObject { mesh, transform });
		self.objects.len() - 1
	}

	/// Like `put`, but stretched a little across the floor (and `up` times
	/// upward) so the piece overlaps its neighbors and no seams show.
	pub(super) fn put_stretched(&mut self, mesh: usize, at: Vec3, yaw: f32, up: f32) -> usize {
		self.put_scaled(mesh, at, yaw, Vec3::new(STRETCH, up, STRETCH))
	}

	/// How far below its origin a model reaches. Hand-placed models are
	/// lifted by this, so they stand on the floor instead of sinking into it.
	pub(super) fn sink(&self, mesh: usize) -> f32 {
		bounds(&self.meshes[mesh]).map_or(0.0, |(lo, _)| (-lo.y).max(0.0))
	}

	/// The box in the world that `mesh` fills when placed by `transform`.
	pub(super) fn box_of(&self, mesh: usize, transform: Mat4) -> (Vec3, Vec3) {
		let (lo, hi) = bounds(&self.meshes[mesh]).unwrap_or((Vec3::ZERO, Vec3::ZERO));
		let corners = (0..8).map(|i| {
			let c = Vec3::new(
				if i & 1 == 0 { lo.x } else { hi.x },
				if i & 2 == 0 { lo.y } else { hi.y },
				if i & 4 == 0 { lo.z } else { hi.z },
			);
			transform.transform_point3(c)
		});
		corners.fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(a, b), p| (a.min(p), b.max(p)))
	}

	/// Records object `object` as a prop, with its box in the world.
	pub(super) fn add_prop(
		&mut self,
		object: usize,
		parent: Option<usize>,
		light: Option<usize>,
		flat: bool,
		fixed: bool,
	) -> usize {
		let o = &self.objects[object];
		let (min, max) = self.box_of(o.mesh, o.transform);
		self.props.push(Prop { object, min, max, parent, light, flat, fixed });
		self.props.len() - 1
	}

	/// Something drawn elsewhere (a chest) that props must stay out of.
	pub(super) fn add_fixed_box(&mut self, min: Vec3, max: Vec3) {
		let object = usize::MAX;
		self.props.push(Prop { object, min, max, parent: None, light: None, flat: false, fixed: true });
	}

	/// The clipping check: takes out every prop that pokes into a wall or
	/// overlaps another prop (other than what it stands on), along with
	/// anything standing on it and its light. Earlier props win, and fixed
	/// ones (kit torches, chests) always do. Returns how many were taken out.
	pub(super) fn remove_clipping(&mut self, map: &MapGrid, lights: &mut Vec<Vec3>) -> usize {
		const SLACK: f32 = 0.03;
		let mut removed = vec![false; self.props.len()];
		for i in 0..self.props.len() {
			let p = &self.props[i];
			if p.fixed {
				continue;
			}
			let parent_gone = p.parent.is_some_and(|q| removed[q]);
			// Every corner of its footprint must be over floor, not rock
			let (lo, hi) = (p.min + Vec3::splat(SLACK), p.max - Vec3::splat(SLACK));
			let in_wall = [(lo.x, lo.z), (hi.x, lo.z), (lo.x, hi.z), (hi.x, hi.z)]
				.iter()
				.any(|&(x, z)| map.is_wall((x / TILE).floor() as i32, (z / TILE).floor() as i32));
			let related = |a: usize, b: usize| self.stands_on(a, b) || self.stands_on(b, a);
			// Fixed props count wherever they come: chests are added last
			let overlaps = !p.flat
				&& (0..self.props.len()).any(|j| {
					let q = &self.props[j];
					((j < i && !removed[j]) || q.fixed)
						&& !q.flat && !related(i, j)
						&& lo.x < q.max.x && hi.x > q.min.x
						&& lo.y < q.max.y && hi.y > q.min.y
						&& lo.z < q.max.z && hi.z > q.min.z
				});
			removed[i] = parent_gone || in_wall || overlaps;
		}

		let gone = |i: &usize| removed[*i];
		let gone_objects: Vec<usize> = (0..self.props.len()).filter(gone).map(|i| self.props[i].object).collect();
		let gone_lights: Vec<usize> = (0..self.props.len()).filter(gone).filter_map(|i| self.props[i].light).collect();
		let mut index = 0;
		self.objects.retain(|_| {
			index += 1;
			!gone_objects.contains(&(index - 1))
		});
		let mut index = 0;
		lights.retain(|_| {
			index += 1;
			!gone_lights.contains(&(index - 1))
		});
		self.props.clear();
		gone_objects.len()
	}

	/// Whether prop `k` stands on prop `target`, directly or on something
	/// that does.
	fn stands_on(&self, mut k: usize, target: usize) -> bool {
		while let Some(next) = self.props[k].parent {
			if next == target {
				return true;
			}
			k = next;
		}
		false
	}
}

/// Places a cell's models on the tile whose floor's middle is `center`,
/// turned `turns` quarter turns clockwise, and records them for the
/// clipping check. Hand-made cells' models are lifted to stand on the floor.
pub(super) fn place_cell(
	b: &mut Builder,
	lights: &mut Vec<Vec3>,
	cell: &Cell,
	center: Vec3,
	turns: usize,
) -> Result<(), String> {
	let yaw = facing_yaw(turns);
	let turn = Quat::from_rotation_y(yaw);
	let mut props = Vec::with_capacity(cell.objects.len());
	for o in &cell.objects {
		let mesh = b.model(&o.model, o.tint)?;
		let lift = if o.grounded { b.sink(mesh) } else { 0.0 };
		let at = center + turn * o.pos + Vec3::Y * lift;
		let object = b.put(mesh, at, yaw + o.yaw_degrees.to_radians());
		let light = (o.light && lights.len() < MAX_TORCHES).then(|| {
			lights.push(at + Vec3::Y * 0.8);
			lights.len() - 1
		});
		let parent = o.parent.and_then(|p| props.get(p).copied());
		let flat = o.model.contains("Carpet");
		props.push(b.add_prop(object, parent, light, flat, cell.structure));
	}
	Ok(())
}
