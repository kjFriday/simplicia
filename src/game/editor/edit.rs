// The editor's per-frame system. view/ registers `update`, which toggles the
// editor with F1 and, while it's open, flies the camera, chooses the model,
// places and removes copies, and saves and loads the prefab (prefab.rs). The
// level browser (browser.rs) takes the keys instead while it's open.
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use super::browser::{browse, Browser};
use super::prefab::{load, save};
use super::{transform, Editor, Placed, FAST, HEIGHT_STEP, MOUSE_SENSITIVITY, SPEED, TURN_STEP};
use crate::engine::mesh::Mesh as ModelMesh;
use crate::engine::scene::model_parts;
use crate::game::ui::controls::editor as key;
use crate::game::ui::screen::Screen;
use crate::game::Game;

/// Everything a system needs to put a model into the world.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Spawner<'w, 's> {
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
			.map(|(mesh, material)| {
				self.commands.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone()), at)).id()
			})
			.collect()
	}

	fn despawn(&mut self, entities: &mut Vec<Entity>) {
		for e in entities.drain(..) {
			self.commands.entity(e).despawn();
		}
	}
}

/// F1 toggles the editor; while it's open, flies the camera and edits.
pub(crate) fn update(
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
	toggle(editor, &mut spawner, &keys, &game, &mut cursors);
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

	// Shift flies faster and takes bigger steps through the models
	let shift = keys.pressed(key::FAST);
	fly(editor, &keys, &motion, &time, shift);
	choose_model(editor, &keys, shift);

	let target = editor.target();
	let at = transform(target, editor.turn);
	show_preview(editor, &mut spawner, at);
	place_and_remove(editor, &mut spawner, &keys, &buttons, target, at);
	save_and_load(editor, &mut spawner, &keys);
}

/// Clears the editor and loads its current file; a file that doesn't exist
/// yet is a new, empty level.
pub(super) fn open(editor: &mut Editor, spawner: &mut Spawner) {
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
			editor.status =
				format!("opened {}: {} models ({missing} missing)", editor.file, editor.placed.len());
		}
		Err(e) => editor.status = format!("load failed: {e}"),
	}
}

/// F1 (or Esc) opens or closes the editor, and locks or frees the cursor.
fn toggle(
	editor: &mut Editor,
	spawner: &mut Spawner,
	keys: &ButtonInput<KeyCode>,
	game: &Game,
	cursors: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
	// Esc leaves the editor, except in the level browser, which it closes
	let leave = editor.active && editor.browser.is_none() && keys.just_pressed(key::LEAVE);
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
			editor.browser = None;
		}
		if let Ok(mut cursor) = cursors.single_mut() {
			let locked = editor.active || game.screen == Screen::Playing;
			cursor.grab_mode = if locked { CursorGrabMode::Locked } else { CursorGrabMode::None };
			cursor.visible = !locked;
		}
	}
}

/// Look and fly: the mouse turns the view, the movement keys fly.
fn fly(
	editor: &mut Editor,
	keys: &ButtonInput<KeyCode>,
	motion: &AccumulatedMouseMotion,
	time: &Time,
	shift: bool,
) {
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
	let speed = if shift { SPEED * FAST } else { SPEED };
	editor.pos += dir.normalize_or_zero() * speed * time.delta_secs();
}

/// Choose and set up the model: which one, its turn, and the placement height.
fn choose_model(editor: &mut Editor, keys: &ButtonInput<KeyCode>, shift: bool) {
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
}

/// The preview follows the target; rebuild it when the model changes.
fn show_preview(editor: &mut Editor, spawner: &mut Spawner, at: Transform) {
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
}

/// Place and remove: a copy at the target, the nearest one, or all of them.
fn place_and_remove(
	editor: &mut Editor,
	spawner: &mut Spawner,
	keys: &ButtonInput<KeyCode>,
	buttons: &ButtonInput<MouseButton>,
	target: Vec3,
	at: Transform,
) {
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
}

/// Save and load the file being edited.
fn save_and_load(editor: &mut Editor, spawner: &mut Spawner, keys: &ButtonInput<KeyCode>) {
	if keys.just_pressed(key::SAVE) {
		editor.status = match save(&editor.path(), &editor.placed) {
			Ok(()) => format!("saved {} models to {}", editor.placed.len(), editor.file),
			Err(e) => format!("save failed: {e}"),
		};
	}
	if keys.just_pressed(key::LOAD) {
		open(editor, spawner);
	}
}
