// Debug-build level editor. F1 swaps the player's view for a free-flying
// camera; pick any bundled model and place copies of it exactly where you
// aim. Layouts are saved as prefabs: plain text files under prefabs/ that the
// game can bake into a room later. B opens the level browser to pick which
// file to edit. Every key is listed in controls.rs (and on the Help screen).
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::controls::{self, editor as key};
use super::menu::Screen;
use crate::engine::hud::{put_str, Framebuffer};
use crate::engine::scene::{model_parts, ModelParts};
use super::cells::{CellKind, CELLS_DIR};
use super::Game;
use crate::engine::mesh::Mesh as ModelMesh;
use crate::engine::models;

/// Where prefabs are saved, and the file the editor starts on.
const PREFAB_DIR: &str = "prefabs";
const FIRST_PREFAB: &str = "prefabs/room.prefab";
/// How far Z / X move the placement height, in meters.
const HEIGHT_STEP: f32 = 0.25;
/// How far R turns the model, in degrees; with Shift, a quarter turn.
const TURN_STEP: f32 = 15.0;
/// Flying speed, meters per second; Shift multiplies it.
const SPEED: f32 = 8.0;
const FAST: f32 = 4.0;
const MOUSE_SENSITIVITY: f32 = 0.003;
/// How far ahead the placement point sits when the view doesn't hit its plane.
const REACH: f32 = 8.0;

/// One model placed in the prefab.
struct Placed {
	model: String,
	pos: Vec3,
	/// Turn about Y, in degrees
	yaw: f32,
	entities: Vec<Entity>,
}

#[derive(Resource)]
pub(super) struct Editor {
	pub(super) active: bool,
	pos: Vec3,
	yaw: f32,
	pitch: f32,
	/// Every model that can be placed, by its path under models/
	palette: Vec<&'static str>,
	current: usize,
	/// Turn of the next model placed, in degrees
	turn: f32,
	/// Height of the plane models are placed on
	height: f32,
	placed: Vec<Placed>,
	/// The preview of the model about to be placed
	ghost: Vec<Entity>,
	ghost_model: Option<usize>,
	/// Models already turned into Bevy meshes; None when one failed to load
	/// The second value is how far the model reaches below its origin; it's
	/// lifted that much so it stands on the floor
	cache: HashMap<&'static str, Option<(ModelParts, f32)>>,
	/// Last thing that happened, shown in the HUD
	status: String,
	/// The prefab file being edited, under PREFAB_DIR
	file: String,
	/// The level browser, when it's open
	browser: Option<Browser>,
}

impl Default for Editor {
	fn default() -> Self {
		let mut palette: Vec<&'static str> = models::MODEL_FILES
			.iter()
			.map(|(name, _)| *name)
			.filter(|n| n.ends_with(".glb") || n.ends_with(".obj"))
			.collect();
		palette.sort_unstable();
		Self {
			active: false,
			pos: Vec3::ZERO,
			yaw: 0.0,
			pitch: 0.0,
			palette,
			current: 0,
			turn: 0.0,
			height: 0.0,
			placed: Vec::new(),
			ghost: Vec::new(),
			ghost_model: None,
			cache: HashMap::new(),
			status: "B browse levels  F5 save  F9 reload".into(),
			file: FIRST_PREFAB.into(),
			browser: None,
		}
	}
}

impl Editor {
	fn forward(&self) -> Vec3 {
		let (sy, cy) = self.yaw.sin_cos();
		let (sp, cp) = self.pitch.sin_cos();
		Vec3::new(sy * cp, sp, -cy * cp)
	}

	/// Where the next model goes: exactly where the view meets the placement
	/// plane, or straight ahead when it doesn't.
	fn target(&self) -> Vec3 {
		let dir = self.forward();
		let t = (self.height - self.pos.y) / dir.y;
		let hit = if dir.y.abs() > 1e-4 && t > 0.0 && t < 60.0 { self.pos + dir * t } else { self.pos + dir * REACH };
		Vec3::new(hit.x, self.height, hit.z)
	}

	/// The file being edited; `file` is its path from the game's folder.
	fn path(&self) -> std::path::PathBuf {
		Path::new(&self.file).to_path_buf()
	}
}

/// The browser: every room prefab and cell saved so far, then a "new" entry
/// for a room and for each kind of cell. Each room or cell is its own file.
struct Browser {
	/// Paths like prefabs/room.prefab or cells/room_edge_1.prefab
	files: Vec<String>,
	selected: usize,
}

impl Browser {
	/// Lists prefabs/ and cells/, highlighting `current` if it's there.
	fn open(current: &str) -> Self {
		let mut files = Vec::new();
		for dir in [PREFAB_DIR, CELLS_DIR] {
			let mut names: Vec<String> = std::fs::read_dir(dir)
				.into_iter()
				.flatten()
				.flatten()
				.filter_map(|e| e.file_name().into_string().ok())
				.filter(|n| n.ends_with(".prefab"))
				.map(|n| format!("{dir}/{n}"))
				.collect();
			names.sort();
			files.extend(names);
		}
		let selected = files.iter().position(|f| f == current).unwrap_or(0);
		Self { files, selected }
	}

	/// Entries after the files: a new room, then a new cell of each kind.
	fn new_entries(&self) -> Vec<String> {
		let free = |prefix: &str| {
			(1..).map(|n| format!("{prefix}_{n}.prefab")).find(|n| !self.files.contains(n)).unwrap()
		};
		let mut entries = vec![free(&format!("{PREFAB_DIR}/room"))];
		entries.extend(CellKind::ALL.iter().map(|k| free(&format!("{CELLS_DIR}/{}", k.prefix()))));
		entries
	}

	fn len(&self) -> usize {
		self.files.len() + self.new_entries().len()
	}

	/// The highlighted file, and whether it's a new one.
	fn chosen(&self) -> (String, bool) {
		match self.files.get(self.selected) {
			Some(f) => (f.clone(), false),
			None => (self.new_entries()[self.selected - self.files.len()].clone(), true),
		}
	}
}

fn transform(pos: Vec3, yaw_degrees: f32) -> Transform {
	Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw_degrees.to_radians()))
}

/// Everything a system needs to put a model into the world.
#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Spawner<'w, 's> {
	commands: Commands<'w, 's>,
	meshes: ResMut<'w, Assets<Mesh>>,
	materials: ResMut<'w, Assets<StandardMaterial>>,
	images: ResMut<'w, Assets<Image>>,
}

impl Spawner<'_, '_> {
	/// Spawns `model`, loading it the first time. Empty if it won't load.
	fn spawn(&mut self, editor: &mut Editor, model: &str, at: Transform) -> Vec<Entity> {
		let Some(&name) = editor.palette.iter().find(|&&n| n == model) else { return Vec::new() };
		let parts = editor.cache.entry(name).or_insert_with(|| {
			ModelMesh::load(name).ok().map(|m| {
				let lowest = m.vertices.iter().map(|v| v.pos.y).fold(f32::INFINITY, f32::min);
				let sink = if lowest.is_finite() { (-lowest).max(0.0) } else { 0.0 };
				(model_parts(&m, &mut self.meshes, &mut self.materials, &mut self.images), sink)
			})
		});
		let Some((parts, sink)) = parts else { return Vec::new() };
		let mut at = at;
		at.translation.y += *sink;
		parts
			.iter()
			.map(|(mesh, material)| self.commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), at)).id())
			.collect()
	}

	fn despawn(&mut self, entities: &mut Vec<Entity>) {
		for e in entities.drain(..) {
			self.commands.entity(e).despawn();
		}
	}
}

/// F1 toggles the editor; while it's open, flies the camera and edits.
pub(super) fn update(
	keys: Res<ButtonInput<KeyCode>>,
	buttons: Res<ButtonInput<MouseButton>>,
	motion: Res<AccumulatedMouseMotion>,
	time: Res<Time>,
	game: Res<Game>,
	mut editor: ResMut<Editor>,
	mut spawner: Spawner,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
	let editor = &mut *editor;
	let leave = editor.active && keys.just_pressed(key::LEAVE);
	if keys.just_pressed(key::TOGGLE) || leave {
		editor.active = !editor.active;
		if editor.active {
			// Start where the player is standing, looking where they look
			let view = game.levels[game.depth].player.view_matrix().inverse();
			let eye = Mat4::from_cols_array(&view.to_cols_array());
			editor.pos = eye.w_axis.truncate();
			let fwd = -eye.z_axis.truncate();
			editor.yaw = fwd.x.atan2(-fwd.z);
			editor.pitch = fwd.y.clamp(-1.0, 1.0).asin();
		} else {
			let mut ghost = std::mem::take(&mut editor.ghost);
			spawner.despawn(&mut ghost);
			editor.ghost_model = None;
		}
		if let Ok(mut cursor) = cursors.single_mut() {
			let locked = editor.active || game.screen == Screen::Playing;
			cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
			cursor.visible = !locked;
		}
	}
	if !editor.active {
		return;
	}

	// The level browser has the keys while it's open
	if editor.browser.is_some() {
		browse(editor, &mut spawner, &keys);
		return;
	}
	if keys.just_pressed(key::BROWSER) {
		editor.browser = Some(Browser::open(&editor.file));
		return;
	}

	// Look and fly
	editor.yaw += motion.delta.x * MOUSE_SENSITIVITY;
	editor.pitch = (editor.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(-1.5, 1.5);
	let fwd = editor.forward();
	let flat = Vec3::new(fwd.x, 0.0, fwd.z).normalize_or_zero();
	let right = flat.cross(Vec3::Y);
	let mut dir = Vec3::ZERO;
	for (key, d) in [
		(key::FORWARD, fwd),
		(key::BACK, -fwd),
		(key::RIGHT, right),
		(key::LEFT, -right),
		(key::UP, Vec3::Y),
		(key::DOWN, -Vec3::Y),
	] {
		if keys.pressed(key) {
			dir += d;
		}
	}
	let shift = keys.pressed(key::FAST);
	let speed = if shift { SPEED * FAST } else { SPEED };
	editor.pos += dir.normalize_or_zero() * speed * time.delta_secs();

	// Choose and set up the model
	let len = editor.palette.len().max(1) as i32;
	let step = if shift { 10 } else { 1 };
	if keys.just_pressed(key::NEXT_MODEL) {
		editor.current = (editor.current as i32 + step).rem_euclid(len) as usize;
	}
	if keys.just_pressed(key::PREVIOUS_MODEL) {
		editor.current = (editor.current as i32 - step).rem_euclid(len) as usize;
	}
	if keys.just_pressed(key::ROTATE) {
		editor.turn = (editor.turn + if shift { 90.0 } else { TURN_STEP }).rem_euclid(360.0);
	}
	if keys.just_pressed(key::RAISE) {
		editor.height += HEIGHT_STEP;
	}
	if keys.just_pressed(key::LOWER) {
		editor.height -= HEIGHT_STEP;
	}

	// The preview follows the target; rebuild it when the model changes
	let target = editor.target();
	let at = transform(target, editor.turn);
	if editor.ghost_model != Some(editor.current) {
		let mut ghost = std::mem::take(&mut editor.ghost);
		spawner.despawn(&mut ghost);
		let model = editor.palette.get(editor.current).copied().unwrap_or_default();
		editor.ghost = spawner.spawn(editor, model, at);
		editor.ghost_model = Some(editor.current);
		if editor.ghost.is_empty() && !model.is_empty() {
			editor.status = format!("{model} won't load");
		}
	}
	// Lifted like placed models, so the preview shows where it will stand
	let sink = editor
		.palette
		.get(editor.current)
		.and_then(|m| editor.cache.get(m))
		.and_then(|c| c.as_ref())
		.map_or(0.0, |(_, s)| *s);
	let mut lifted = at;
	lifted.translation.y += sink;
	for &e in &editor.ghost {
		spawner.commands.entity(e).insert(lifted);
	}

	// Place and remove
	if buttons.just_pressed(key::PLACE.0) || keys.just_pressed(key::PLACE.1) {
		if let Some(&model) = editor.palette.get(editor.current) {
			let entities = spawner.spawn(editor, model, at);
			if !entities.is_empty() {
				editor.placed.push(Placed { model: model.into(), pos: target, yaw: editor.turn, entities });
				editor.status = format!("placed {} models", editor.placed.len());
			}
		}
	}
	if buttons.just_pressed(key::REMOVE.0) || keys.just_pressed(key::REMOVE.1) {
		let nearest = editor
			.placed
			.iter()
			.enumerate()
			.min_by(|a, b| a.1.pos.distance(target).total_cmp(&b.1.pos.distance(target)))
			.map(|(i, _)| i);
		if let Some(i) = nearest {
			let mut removed = editor.placed.remove(i);
			spawner.despawn(&mut removed.entities);
			editor.status = format!("removed {}", removed.model);
		}
	}
	if keys.just_pressed(key::CLEAR) {
		for mut p in editor.placed.drain(..) {
			spawner.despawn(&mut p.entities);
		}
		editor.status = "cleared".into();
	}

	// Save and load
	if keys.just_pressed(key::SAVE) {
		editor.status = match save(&editor.path(), &editor.placed) {
			Ok(()) => format!("saved {} models to {}", editor.placed.len(), editor.file),
			Err(e) => format!("save failed: {e}"),
		};
	}
	if keys.just_pressed(key::LOAD) {
		open(editor, &mut spawner);
	}
}

/// Clears the editor and loads its current file; a file that doesn't exist
/// yet is a new, empty level.
fn open(editor: &mut Editor, spawner: &mut Spawner) {
	for mut p in editor.placed.drain(..) {
		spawner.despawn(&mut p.entities);
	}
	if !editor.path().exists() {
		editor.status = format!("new level: {}", editor.file);
		return;
	}
	match load(&editor.path()) {
		Ok(entries) => {
			let mut missing = 0;
			for (model, pos, yaw) in entries {
				let entities = spawner.spawn(editor, &model, transform(pos, yaw));
				if entities.is_empty() {
					missing += 1;
				}
				editor.placed.push(Placed { model, pos, yaw, entities });
			}
			editor.status = format!("opened {}: {} models ({missing} missing)", editor.file, editor.placed.len());
		}
		Err(e) => editor.status = format!("load failed: {e}"),
	}
}

/// The level browser's turn at the keys while it's open: W/S pick a file,
/// Enter opens it (or starts a new level), Esc or B closes the browser.
fn browse(editor: &mut Editor, spawner: &mut Spawner, keys: &ButtonInput<KeyCode>) {
	let Some(browser) = &mut editor.browser else { return };
	let count = browser.len();
	if keys.any_just_pressed(controls::MENU_UP) {
		browser.selected = (browser.selected + count - 1) % count;
	}
	if keys.any_just_pressed(controls::MENU_DOWN) {
		browser.selected = (browser.selected + 1) % count;
	}
	if keys.any_just_pressed([controls::MENU_BACK, key::BROWSER]) {
		editor.browser = None;
	} else if keys.any_just_pressed(controls::MENU_SELECT) {
		editor.file = browser.chosen().0;
		editor.browser = None;
		open(editor, spawner);
	}
}

/// One line per model: `path x y z yaw-degrees`. Lines starting with # are comments.
fn save(path: &Path, placed: &[Placed]) -> std::io::Result<()> {
	let mut text = String::from("# prefab: model x y z yaw_degrees (meters, Y up; tiles are 4 m)\n");
	for p in placed {
		let _ = writeln!(text, "{} {} {} {} {}", p.model.replace(' ', "%20"), p.pos.x, p.pos.y, p.pos.z, p.yaw);
	}
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	std::fs::write(path, text)
}

fn load(path: &Path) -> Result<Vec<(String, Vec3, f32)>, String> {
	let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
	parse(&text)
}

pub(super) fn parse(text: &str) -> Result<Vec<(String, Vec3, f32)>, String> {
	let mut out = Vec::new();
	for (n, line) in text.lines().enumerate() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let f: Vec<&str> = line.split_whitespace().collect();
		let bad = || format!("line {}: expected `model x y z yaw`", n + 1);
		if f.len() != 5 {
			return Err(bad());
		}
		let num = |s: &str| s.parse::<f32>().map_err(|_| bad());
		out.push((f[0].replace("%20", " "), Vec3::new(num(f[1])?, num(f[2])?, num(f[3])?), num(f[4])?));
	}
	Ok(out)
}

/// Puts the 3D camera at the editor's eye while the editor is open.
pub(super) fn camera(editor: &Editor) -> Option<Transform> {
	editor.active.then(|| Transform::from_translation(editor.pos).looking_to(editor.forward(), Vec3::Y))
}

/// The editor's readout along the bottom of the screen.
pub(super) fn draw_hud(editor: &Editor, buffer: &mut Framebuffer) {
	if !editor.active {
		return;
	}
	const TEXT: [u8; 3] = [220, 220, 220];
	if let Some(browser) = &editor.browser {
		draw_browser(browser, &editor.file, buffer);
		return;
	}
	const VALUE: [u8; 3] = [255, 220, 90];
	const HINT: [u8; 3] = [130, 130, 130];
	// Two lines along the top: what's being placed, and what's happening.
	// The keys are on the Help screen; the map stays hidden while editing.
	let model = editor.palette.get(editor.current).copied().unwrap_or("(no models)");
	let model = model.strip_suffix(".glb").unwrap_or(model);
	let t = editor.target();
	let place = format!("{}/{} {model}  {}°  y {:.2}", editor.current + 1, editor.palette.len(), editor.turn, t.y);
	put_str(buffer, 0, 0, &place, VALUE);
	let status = format!("{}  ({} placed)  {}", editor.file, editor.placed.len(), editor.status);
	put_str(buffer, 0, 1, &status, TEXT);
	let hint = "Q/E model  R turn  Z/X height  click place  right-click remove  B files  F5 save  F1 exit";
	put_str(buffer, 0, buffer.height as i32 - 1, hint, HINT);
}

/// The level browser: the saved prefabs, the open one marked, and "new level".
fn draw_browser(browser: &Browser, open_file: &str, buffer: &mut Framebuffer) {
	const TEXT: [u8; 3] = [220, 220, 220];
	const HIGHLIGHT: [u8; 3] = [255, 220, 90];
	const HINT: [u8; 3] = [130, 130, 130];
	let mut lines: Vec<String> = browser
		.files
		.iter()
		.map(|f| if f == open_file { format!("{f}  (open)") } else { f.clone() })
		.collect();
	lines.extend(browser.new_entries().iter().map(|f| format!("+ new {f}")));
	let top = 2;
	put_str(buffer, 2, 0, "ROOMS and CELLS (cells are one 4 m tile, built facing north)", HIGHLIGHT);
	for (i, line) in lines.iter().enumerate() {
		let chosen = i == browser.selected;
		let text = if chosen { format!("> {line}") } else { format!("  {line}") };
		put_str(buffer, 2, top + i as i32, &text, if chosen { HIGHLIGHT } else { TEXT });
	}
	put_str(buffer, 2, top + lines.len() as i32 + 1, "W/S pick  Enter open  Esc/B close", HINT);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prefab_text_round_trips() {
		let text = "# comment\nTiles/Dungeon_Hall_Center.glb 0 0 4 1\nnature/plants/Green%20Bush.glb -2 0.5 0 3\n";
		let parsed = parse(text).unwrap();
		assert_eq!(parsed[0], ("Tiles/Dungeon_Hall_Center.glb".to_string(), Vec3::new(0.0, 0.0, 4.0), 1.0));
		assert_eq!(parsed[1].0, "nature/plants/Green Bush.glb");
		assert!(parse("just one field").is_err());
	}

	#[test]
	fn target_is_exactly_where_the_view_meets_the_placement_plane() {
		let editor = Editor { pos: Vec3::new(0.3, 4.0, 0.0), pitch: -std::f32::consts::FRAC_PI_4, ..default() };
		let t = editor.target();
		assert!(t.distance(Vec3::new(0.3, 0.0, -4.0)) < 1e-4, "{t}");
		assert!(editor.palette.iter().any(|n| n.starts_with("Tiles/")));
	}
}
