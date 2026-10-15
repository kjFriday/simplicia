// Debug-build level editor. F1 swaps the player's view for a free-flying
// camera; pick any bundled model and place copies of it exactly where you
// aim. Layouts are saved as prefabs: plain text files under prefabs/ that the
// game can bake into a room later. B opens the level browser to pick which
// file to edit. Every key is listed in ui/controls.rs (and on the Help screen).
//
// Reading guide: this file holds the editor's state (`Editor`) and what the
// view asks of it (`camera`, `draw_hud`). edit.rs is the per-frame system that
// flies and places, browser.rs is the file picker opened with B, and prefab.rs
// reads and writes the files. view/ registers `update` and calls `camera` and
// `draw_hud`; levels/cells.rs reads cells with `parse`.

/// The editor's per-frame system: flying, choosing, placing, saving, loading.
mod edit;
/// The level browser (B): picking which room or cell file to edit.
mod browser;
/// Prefab files: writing, reading, and parsing their text.
mod prefab;

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;

use crate::engine::hud::{put_str, Framebuffer};
use crate::engine::models;
use crate::engine::scene::ModelParts;
use browser::{draw_browser, Browser};

pub(crate) use edit::update;
pub(crate) use prefab::parse;

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
pub(crate) struct Editor {
	pub(crate) active: bool,
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
		let hit = if dir.y.abs() > 1e-4 && t > 0.0 && t < 60.0 {
			self.pos + dir * t
		} else {
			self.pos + dir * REACH
		};
		Vec3::new(hit.x, self.height, hit.z)
	}

	/// The file being edited; `file` is its path from the game's folder.
	fn path(&self) -> std::path::PathBuf {
		Path::new(&self.file).to_path_buf()
	}
}

fn transform(pos: Vec3, yaw_degrees: f32) -> Transform {
	Transform::from_translation(pos).with_rotation(Quat::from_rotation_y(yaw_degrees.to_radians()))
}

/// Puts the 3D camera at the editor's eye while the editor is open.
pub(crate) fn camera(editor: &Editor) -> Option<Transform> {
	editor.active.then(|| Transform::from_translation(editor.pos).looking_to(editor.forward(), Vec3::Y))
}

/// The editor's readout along the bottom of the screen.
pub(crate) fn draw_hud(editor: &Editor, buffer: &mut Framebuffer) {
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
	let place = format!(
		"{}/{} {model}  {}°  y {:.2}",
		editor.current + 1,
		editor.palette.len(),
		editor.turn,
		t.y
	);
	put_str(buffer, 0, 0, &place, VALUE);
	let status = format!("{}  ({} placed)  {}", editor.file, editor.placed.len(), editor.status);
	put_str(buffer, 0, 1, &status, TEXT);
	let hint = "Q/E model  R turn  Z/X height  click place  right-click remove  B files  F5 save  F1 exit";
	put_str(buffer, 0, buffer.height as i32 - 1, hint, HINT);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn target_is_exactly_where_the_view_meets_the_placement_plane() {
		let editor = Editor { pos: Vec3::new(0.3, 4.0, 0.0), pitch: -std::f32::consts::FRAC_PI_4, ..default() };
		let t = editor.target();
		assert!(t.distance(Vec3::new(0.3, 0.0, -4.0)) < 1e-4, "{t}");
		assert!(editor.palette.iter().any(|n| n.starts_with("Tiles/")));
	}
}
