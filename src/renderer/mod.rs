mod crawler;
mod font;
mod mesh;
mod minimap;
pub mod models;
mod window;

use crawler::{Action, Arrival, World};
use font::GlyphSheet;

pub use window::RendererPlugin;

use bevy::ecs::resource::Resource;
use glam::{Mat4, Vec2, Vec3, Vec4};

/// Height of a character cell divided by its width. The Dungeon Mode font is 8x8.
const CELL_ASPECT: f32 = 1.0;

const ASCII_RAMP: &[char] = &['.', ':', '-', '=', '+', '*', '#', '%', '@'];

#[derive(Clone, Copy)]
struct Vertex {
	pos: Vec3,
	/// RGB, 0.0 to 1.0. Multiplies the texture when there is one.
	color: Vec3,
	/// Texture coordinates; meaningless without a texture
	uv: Vec2,
	/// Index into the mesh's `textures`, or `NO_TEXTURE`
	tex: u32,
}

const NO_TEXTURE: u32 = u32::MAX;

/// An RGBA8 image a mesh is painted with.
struct Texture {
	width: u32,
	height: u32,
	rgba: Vec<u8>,
}

impl Texture {
	/// Nearest pixel at `uv`, wrapping around like glTF's default sampler.
	fn sample(&self, uv: Vec2) -> Vec3 {
		let x = (uv.x.rem_euclid(1.0) * self.width as f32) as u32 % self.width;
		let y = (uv.y.rem_euclid(1.0) * self.height as f32) as u32 % self.height;
		let i = (y * self.width + x) as usize * 4;
		Vec3::new(self.rgba[i] as f32, self.rgba[i + 1] as f32, self.rgba[i + 2] as f32) / 255.0
	}
}

/// One character cell: the character and its RGB color.
#[derive(Clone, Copy, PartialEq)]
struct Cell {
	ch: char,
	rgb: [u8; 3],
	/// HUD text like the minimap: drawn on solid black. Other cells are drawn
	/// over the 3D view with a see-through background.
	ui: bool,
	/// How lit the surface is, 0 to 255. Over the 3D view, lit surfaces show
	/// the 3D render and dark ones show the text; see `GlyphSheet::draw`.
	light: u8,
}

/// Nothing drawn: full light, so the 3D view (or sky) shows through.
const EMPTY_CELL: Cell = Cell { ch: ' ', rgb: [0, 0, 0], ui: false, light: 255 };

struct Mesh {
	vertices: Vec<Vertex>,
	indices: Vec<[usize; 3]>,
	textures: Vec<Texture>,
}



struct Framebuffer {
	width: usize,
	height: usize,
	color_buffer: Vec<Cell>,
	z_buffer: Vec<f32>,
}

impl Framebuffer {
	fn new(width: usize, height: usize) -> Self {
		Self {
			width,
			height,
			color_buffer: vec![EMPTY_CELL; width * height],
			z_buffer: vec![f32::INFINITY; width * height],
		}
	}

	fn clear(&mut self) {
		self.color_buffer.fill(EMPTY_CELL);
		self.z_buffer.fill(f32::INFINITY);
	}

	fn set_pixel(&mut self, x: usize, y: usize, depth: f32, cell: Cell) {
		if x < self.width && y < self.height {
			let idx = y * self.width + x;
			if depth < self.z_buffer[idx] {
				self.z_buffer[idx] = depth;
				self.color_buffer[idx] = cell;
			}
		}
	}
}

/// Holds everything that must survive between frames: the levels, the
/// character grid the 3D view is drawn into, and the font that turns it into
/// pixels. Lives in the Bevy world as a resource; `RendererPlugin` drives it.
#[derive(Resource)]
pub struct Renderer {
	/// Every level generated so far, by depth; going back up finds them as you left them
	levels: Vec<World>,
	/// Index into `levels` of the level the player is on
	depth: usize,
	light_dir: Vec3,
	framebuffer: Framebuffer,
	glyphs: GlyphSheet,
}

impl Renderer {
	pub fn new() -> Result<Self, String> {
		//needs base ground plane from hex on round world grid.
		let first_level = World::generate(&mut rand::rng(), 0)?;
		Ok(Self {
			levels: vec![first_level],
			depth: 0,
			//give sun angle
			light_dir: Vec3::new(0.5, -1.0, -0.5).normalize(),
			framebuffer: Framebuffer::new(0, 0),
			glyphs: GlyphSheet::load()?,
		})
	}

	/// Turns the view by mouse movement, in radians. Doesn't take a turn.
	fn look(&mut self, yaw: f32, pitch: f32) {
		self.levels[self.depth].player.look(yaw, pitch);
	}

	/// One action, one turn. Stepping onto stairs changes level: down generates
	/// the next level the first time, up returns to the one above.
	fn act(&mut self, action: Action) -> Result<(), String> {
		match self.levels[self.depth].act(action) {
			Some(Arrival::StairsDown) => {
				self.depth += 1;
				if self.depth == self.levels.len() {
					let level = World::generate(&mut rand::rng(), self.depth)?;
					self.levels.push(level);
				} else {
					// Back on a level you've been to: arrive on its up staircase
					let level = &mut self.levels[self.depth];
					let up = level.stairs_up.expect("levels below the top have an up staircase");
					level.place_player(up);
				}
			}
			Some(Arrival::StairsUp) => {
				self.depth -= 1;
				let level = &mut self.levels[self.depth];
				let down = level.stairs_down;
				level.place_player(down);
			}
			Some(Arrival::Floor) | None => {}
		}
		Ok(())
	}

	/// Draws one frame into a `cols` x `rows` character grid.
	fn render(&mut self, cols: usize, rows: usize) {
		// Reallocate only when the window is resized
		if cols != self.framebuffer.width || rows != self.framebuffer.height {
			self.framebuffer = Framebuffer::new(cols, rows);
		}
		if cols == 0 || rows == 0 {
			return;
		}
		let (width, height) = (cols, rows);
		let framebuffer = &mut self.framebuffer;

		// Matrix transformations
		let aspect_ratio = (width as f32) / (height as f32 * CELL_ASPECT); // Correct for non-square characters
		let proj = glam::camera::rh::proj::opengl::perspective(
			60.0f32.to_radians(),
			aspect_ratio,
			NEAR_PLANE,
			FAR_PLANE,
		);
		let world = &self.levels[self.depth];
		let view_proj = proj * world.player.view_matrix();

		framebuffer.clear();
		for object in &world.objects {
			let mvp = view_proj * object.transform;
			render_mesh(&world.meshes[object.mesh], mvp, object.transform, self.light_dir, framebuffer);
		}
		minimap::draw(world, framebuffer);
	}
}

/// One model placed in the world.
struct SceneObject {
	/// Index into `World::meshes`
	mesh: usize,
	/// Model space to world space
	transform: Mat4,
}

const NEAR_PLANE: f32 = 0.5;
const FAR_PLANE: f32 = 1000.0;

// Computes 2D edge function for barycentric coordinates
fn edge_function(a: Vec3, b: Vec3, c: Vec3) -> f32 {
	(c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

/// A triangle corner on its way to the screen.
#[derive(Clone, Copy)]
struct Corner {
	clip: Vec4,
	color: Vec3,
	uv: Vec2,
}

impl Corner {
	fn lerp(self, other: Corner, t: f32) -> Corner {
		Corner {
			clip: self.clip.lerp(other.clip, t),
			color: self.color.lerp(other.color, t),
			uv: self.uv.lerp(other.uv, t),
		}
	}
}

/// `screen` is x, y in cells and z = depth; `inv_w` is 1 / clip w per corner,
/// used to interpolate texture coordinates with perspective.
fn rasterize_triangle(
	screen: [Vec3; 3],
	inv_w: [f32; 3],
	corners: [Corner; 3],
	texture: Option<&Texture>,
	light_intensity: f32,
	buffer: &mut Framebuffer,
) {
	let [v0, v1, v2] = screen;
	let colors = corners.map(|c| c.color);
	// Bounding box of the triangle in screen space
	let min_x = v0.x.min(v1.x).min(v2.x).floor().max(0.0) as usize;
	let max_x = v0.x.max(v1.x).max(v2.x).ceil().min((buffer.width - 1) as f32) as usize;
	let min_y = v0.y.min(v1.y).min(v2.y).floor().max(0.0) as usize;
	let max_y = v0.y.max(v1.y).max(v2.y).ceil().min((buffer.height - 1) as f32) as usize;

	let area = edge_function(v0, v1, v2);
	if area <= 0.0 {
		return; // Back-face culling / degenerate triangle
	}

	// Select character based on lighting
	let ramp_idx = ((light_intensity.clamp(0.0, 1.0)) * (ASCII_RAMP.len() - 1) as f32).round() as usize;
	let char_to_draw = ASCII_RAMP[ramp_idx];

	for y in min_y..=max_y {
		for x in min_x..=max_x {
			let p = Vec3::new(x as f32 + 0.5, y as f32 + 0.5, 0.0);

			let w0 = edge_function(v1, v2, p);
			let w1 = edge_function(v2, v0, p);
			let w2 = edge_function(v0, v1, p);

			if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
				let w0 = w0 / area;
				let w1 = w1 / area;
				let w2 = w2 / area;

				// Interpolate depth z
				let depth = w0 * v0.z + w1 * v1.z + w2 * v2.z;

				// Blend the corner colors, paint with the texture, then darken by
				// the light like the character does
				let mut color = w0 * colors[0] + w1 * colors[1] + w2 * colors[2];
				if let Some(texture) = texture {
					let (p0, p1, p2) = (w0 * inv_w[0], w1 * inv_w[1], w2 * inv_w[2]);
					let uv = (p0 * corners[0].uv + p1 * corners[1].uv + p2 * corners[2].uv) / (p0 + p1 + p2);
					color *= texture.sample(uv);
				}
				let color = color * light_intensity;
				let rgb = (color.clamp(Vec3::ZERO, Vec3::ONE) * 255.0).round();
				let cell = Cell { ch: char_to_draw, rgb: [rgb.x as u8, rgb.y as u8, rgb.z as u8],
					ui: false,
					light: (light_intensity.clamp(0.0, 1.0) * 255.0) as u8,
				};
				buffer.set_pixel(x, y, depth, cell);
			}
		}
	}
}

fn render_mesh(
	mesh: &Mesh,
	mvp: Mat4,
	model: Mat4,
	light_dir: Vec3,
	buffer: &mut Framebuffer,
) {
	let screen_w = buffer.width as f32;
	let screen_h = buffer.height as f32;

	for tri in &mesh.indices {
		let v0_world = model.transform_point3(mesh.vertices[tri[0]].pos);
		let v1_world = model.transform_point3(mesh.vertices[tri[1]].pos);
		let v2_world = model.transform_point3(mesh.vertices[tri[2]].pos);

		// Compute face normal in world space for directional lighting
		let edge1 = v1_world - v0_world;
		let edge2 = v2_world - v0_world;
		let normal = edge1.cross(edge2).normalize_or_zero();

		let intensity = normal.dot(-light_dir).max(0.1); // 0.1 ambient light

		// Transform to Clip Space
		let corners = tri.map(|i| {
			let v = mesh.vertices[i];
			Corner { clip: mvp * Vec4::from((v.pos, 1.0)), color: v.color, uv: v.uv }
		});
		let tex = mesh.vertices[tri[0]].tex;
		let texture = (tex != NO_TEXTURE).then(|| &mesh.textures[tex as usize]);

		// Skip triangles wholly outside one side of the view; there are a lot of them
		if outside_frustum(corners.map(|c| c.clip)) {
			continue;
		}

		// Cut away the part behind the camera; dividing by w there would flip it onto the screen
		let polygon = clip_near(&corners);

		// Perspective Divide -> NDC (-1 to 1), then Screen Coordinates
		// (accounting for ~2:1 terminal font aspect ratio)
		let to_screen = |v: Vec4| -> Vec3 {
			let v = v / v.w;
			Vec3::new(
				(v.x + 1.0) * 0.5 * screen_w,
				(1.0 - v.y) * 0.5 * screen_h, // Y-axis inverted for screen coordinates
				v.z,
			)
		};

		// Clipping can turn the triangle into a quad; fan it back into triangles
		for i in 1..polygon.len().saturating_sub(1) {
			let (a, b, c) = (polygon[0], polygon[i], polygon[i + 1]);
			rasterize_triangle(
				[to_screen(a.clip), to_screen(b.clip), to_screen(c.clip)],
				[1.0 / a.clip.w, 1.0 / b.clip.w, 1.0 / c.clip.w],
				[a, b, c],
				texture,
				intensity,
				buffer,
			);
		}
	}
}

/// True when all three corners are past the same clip plane, so none of the
/// triangle can be visible.
fn outside_frustum(c: [Vec4; 3]) -> bool {
	let all = |f: &dyn Fn(Vec4) -> bool| c.iter().all(|&v| f(v));
	all(&|v| v.x < -v.w)
		|| all(&|v| v.x > v.w)
		|| all(&|v| v.y < -v.w)
		|| all(&|v| v.y > v.w)
		|| all(&|v| v.z < -v.w)
		|| all(&|v| v.z > v.w)
}

/// Clips a triangle against the near plane (z >= -w in OpenGL clip space).
/// Returns 0, 3 or 4 corners, colors and UVs interpolated along the cut edges.
fn clip_near(corners: &[Corner; 3]) -> Vec<Corner> {
	let inside = |v: Vec4| v.z + v.w;
	let mut out = Vec::with_capacity(4);
	for i in 0..3 {
		let (a, b) = (corners[i], corners[(i + 1) % 3]);
		let (da, db) = (inside(a.clip), inside(b.clip));
		if da >= 0.0 {
			out.push(a);
		}
		// The edge crosses the plane: add the crossing point
		if (da >= 0.0) != (db >= 0.0) {
			out.push(a.lerp(b, da / (da - db)));
		}
	}
	out
}