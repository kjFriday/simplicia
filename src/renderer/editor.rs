// Debug-build level editor. F1 swaps the player's view for a free-flying
// camera; pick any bundled model, place copies of it on a snapping grid, and
// save the layout as a prefab: a plain text file the game can bake into a
// room later.
//
// Keys:   F1 / Esc  leave          WASD  fly, Space / Ctrl up / down, Shift fast
//         mouse     look           Q / E previous / next model (Shift: by 10)
//         R         rotate 90°     Z / X lower / raise the placement height
//         G         change snap    Left click / Enter  place
//         Right click / Backspace  remove the nearest placed model
//         Delete    remove all     F5 save   F9 load   (prefabs/room.prefab)
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::menu::Screen;
use super::minimap::put_str;
use super::window::{model_parts, ModelParts};
use super::{models, Renderer};

/// Where F5 saves and F9 loads.
const PREFAB_PATH: &str = "prefabs/room.prefab";
/// Grid steps G cycles through, in meters. The dungeon tiles are 4 m.
const SNAPS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
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
	/// Quarter turns about Y
	turns: u8,
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
	turns: u8,
	snap: usize,
	/// Height of the plane models are placed on
	height: f32,
	placed: Vec<Placed>,
	/// The preview of the model about to be placed
	ghost: Vec<Entity>,
	ghost_model: Option<usize>,
	/// Models already turned into Bevy meshes; None when one failed to load
	cache: HashMap<&'static str, Option<ModelParts>>,
	/// Last thing that happened, shown in the HUD
	status: String,
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
			turns: 0,
			snap: 4,
			height: 0.0,
			placed: Vec::new(),
			ghost: Vec::new(),
			ghost_model: None,
			cache: HashMap::new(),
			status: "F5 save  F9 load".into(),
		}
	}
}

impl Editor {
	fn forward(&self) -> Vec3 {
		let (sy, cy) = self.yaw.sin_cos();
		let (sp, cp) = self.pitch.sin_cos();
		Vec3::new(sy * cp, sp, -cy * cp)
	}

	/// Where the next model goes: where the view meets the placement plane,
	/// or straight ahead when it doesn't; snapped to the grid.
	fn target(&self) -> Vec3 {
		let dir = self.forward();
		let t = (self.height - self.pos.y) / dir.y;
		let hit = if dir.y.abs() > 1e-4 && t > 0.0 && t < 60.0 { self.pos + dir * t } else { self.pos + dir * REACH };
		let snap = SNAPS[self.snap];
		Vec3::new((hit.x / snap).round() * snap, self.height, (hit.z / snap).round() * snap)
	}
}

fn transform(pos: Vec3, turns: u8) -> Transform {
	Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(turns as f32 * std::f32::consts::FRAC_PI_2))
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
			super::Mesh::load(name)
				.ok()
				.map(|m| model_parts(&m, &mut self.meshes, &mut self.materials, &mut self.images))
		});
		let Some(parts) = parts else { return Vec::new() };
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
	renderer: Res<Renderer>,
	mut editor: ResMut<Editor>,
	mut spawner: Spawner,
	mut cursors: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
	let editor = &mut *editor;
	let leave = editor.active && keys.just_pressed(KeyCode::Escape);
	if keys.just_pressed(KeyCode::F1) || leave {
		editor.active = !editor.active;
		if editor.active {
			// Start where the player is standing, looking where they look
			let view = renderer.levels[renderer.depth].player.view_matrix().inverse();
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
			let locked = editor.active || renderer.screen == Screen::Playing;
			cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
			cursor.visible = !locked;
		}
	}
	if !editor.active {
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
		(KeyCode::KeyW, fwd),
		(KeyCode::KeyS, -fwd),
		(KeyCode::KeyD, right),
		(KeyCode::KeyA, -right),
		(KeyCode::Space, Vec3::Y),
		(KeyCode::ControlLeft, -Vec3::Y),
	] {
		if keys.pressed(key) {
			dir += d;
		}
	}
	let shift = keys.pressed(KeyCode::ShiftLeft);
	let speed = if shift { SPEED * FAST } else { SPEED };
	editor.pos += dir.normalize_or_zero() * speed * time.delta_secs();

	// Choose and set up the model
	let len = editor.palette.len().max(1) as i32;
	let step = if shift { 10 } else { 1 };
	if keys.just_pressed(KeyCode::KeyE) {
		editor.current = (editor.current as i32 + step).rem_euclid(len) as usize;
	}
	if keys.just_pressed(KeyCode::KeyQ) {
		editor.current = (editor.current as i32 - step).rem_euclid(len) as usize;
	}
	if keys.just_pressed(KeyCode::KeyR) {
		editor.turns = (editor.turns + 1) % 4;
	}
	if keys.just_pressed(KeyCode::KeyG) {
		editor.snap = (editor.snap + 1) % SNAPS.len();
	}
	if keys.just_pressed(KeyCode::KeyX) {
		editor.height += SNAPS[editor.snap];
	}
	if keys.just_pressed(KeyCode::KeyZ) {
		editor.height -= SNAPS[editor.snap];
	}

	// The preview follows the target; rebuild it when the model changes
	let target = editor.target();
	let at = transform(target, editor.turns);
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
	for &e in &editor.ghost {
		spawner.commands.entity(e).insert(at);
	}

	// Place and remove
	if buttons.just_pressed(MouseButton::Left) || keys.just_pressed(KeyCode::Enter) {
		if let Some(&model) = editor.palette.get(editor.current) {
			let entities = spawner.spawn(editor, model, at);
			if !entities.is_empty() {
				editor.placed.push(Placed { model: model.into(), pos: target, turns: editor.turns, entities });
				editor.status = format!("placed {} models", editor.placed.len());
			}
		}
	}
	if buttons.just_pressed(MouseButton::Right) || keys.just_pressed(KeyCode::Backspace) {
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
	if keys.just_pressed(KeyCode::Delete) {
		for mut p in editor.placed.drain(..) {
			spawner.despawn(&mut p.entities);
		}
		editor.status = "cleared".into();
	}

	// Save and load
	if keys.just_pressed(KeyCode::F5) {
		editor.status = match save(&editor.placed) {
			Ok(()) => format!("saved {} models to {PREFAB_PATH}", editor.placed.len()),
			Err(e) => format!("save failed: {e}"),
		};
	}
	if keys.just_pressed(KeyCode::F9) {
		match load() {
			Ok(entries) => {
				for mut p in editor.placed.drain(..) {
					spawner.despawn(&mut p.entities);
				}
				let mut missing = 0;
				for (model, pos, turns) in entries {
					let entities = spawner.spawn(editor, &model, transform(pos, turns));
					if entities.is_empty() {
						missing += 1;
					}
					editor.placed.push(Placed { model, pos, turns, entities });
				}
				editor.status = format!("loaded {} models ({missing} missing)", editor.placed.len());
			}
			Err(e) => editor.status = format!("load failed: {e}"),
		}
	}
}

/// One line per model: `path x y z quarter-turns`. Lines starting with # are comments.
fn save(placed: &[Placed]) -> std::io::Result<()> {
	let mut text = String::from("# prefab: model x y z quarter_turns (meters, Y up; tiles are 4 m)\n");
	for p in placed {
		let _ = writeln!(text, "{} {} {} {} {}", p.model.replace(' ', "%20"), p.pos.x, p.pos.y, p.pos.z, p.turns);
	}
	if let Some(dir) = Path::new(PREFAB_PATH).parent() {
		std::fs::create_dir_all(dir)?;
	}
	std::fs::write(PREFAB_PATH, text)
}

fn load() -> Result<Vec<(String, Vec3, u8)>, String> {
	let text = std::fs::read_to_string(PREFAB_PATH).map_err(|e| e.to_string())?;
	parse(&text)
}

fn parse(text: &str) -> Result<Vec<(String, Vec3, u8)>, String> {
	let mut out = Vec::new();
	for (n, line) in text.lines().enumerate() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let f: Vec<&str> = line.split_whitespace().collect();
		let bad = || format!("line {}: expected `model x y z turns`", n + 1);
		if f.len() != 5 {
			return Err(bad());
		}
		let num = |s: &str| s.parse::<f32>().map_err(|_| bad());
		let turns = f[4].parse::<u8>().map_err(|_| bad())? % 4;
		out.push((f[0].replace("%20", " "), Vec3::new(num(f[1])?, num(f[2])?, num(f[3])?), turns));
	}
	Ok(out)
}

/// Puts the 3D camera at the editor's eye while the editor is open.
pub(super) fn camera(editor: &Editor) -> Option<Transform> {
	editor.active.then(|| Transform::from_translation(editor.pos).looking_to(editor.forward(), Vec3::Y))
}

/// The editor's readout along the bottom of the screen.
pub(super) fn draw_hud(editor: &Editor, buffer: &mut super::Framebuffer) {
	if !editor.active {
		return;
	}
	const TEXT: [u8; 3] = [220, 220, 220];
	const VALUE: [u8; 3] = [255, 220, 90];
	let model = editor.palette.get(editor.current).copied().unwrap_or("(no models)");
	let t = editor.target();
	let lines = [
		(format!("EDITOR  model {}/{}: {model}", editor.current + 1, editor.palette.len()), VALUE),
		(
			format!(
				"at {:.2} {:.2} {:.2}  turn {}°  snap {} m  placed {}",
				t.x,
				t.y,
				t.z,
				editor.turns as u32 * 90,
				SNAPS[editor.snap],
				editor.placed.len()
			),
			TEXT,
		),
		(editor.status.clone(), TEXT),
		("Q/E model  R turn  Z/X height  G snap  LMB place  RMB remove  F5 save  F9 load  F1 exit".into(), TEXT),
	];
	let top = buffer.height as i32 - lines.len() as i32;
	for (i, (text, rgb)) in lines.iter().enumerate() {
		put_str(buffer, 0, top + i as i32, text, *rgb);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prefab_text_round_trips() {
		let text = "# comment\nTiles/Dungeon_Hall_Center.glb 0 0 4 1\nnature/plants/Green%20Bush.glb -2 0.5 0 3\n";
		let parsed = parse(text).unwrap();
		assert_eq!(parsed[0], ("Tiles/Dungeon_Hall_Center.glb".to_string(), Vec3::new(0.0, 0.0, 4.0), 1));
		assert_eq!(parsed[1].0, "nature/plants/Green Bush.glb");
		assert!(parse("just one field").is_err());
	}

	#[test]
	fn target_snaps_to_the_grid_on_the_placement_plane() {
		let mut editor = Editor { pos: Vec3::new(0.3, 4.0, 0.0), pitch: -std::f32::consts::FRAC_PI_4, ..default() };
		editor.snap = 4; // 4 m
		let t = editor.target();
		assert_eq!(t, Vec3::new(0.0, 0.0, -4.0));
		assert!(editor.palette.iter().any(|n| n.starts_with("Tiles/")));
	}
}
