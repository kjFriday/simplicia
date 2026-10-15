// The weapon in hand: a knife until the hero equips a sword. Both hang from
// the 3D camera (`effects::setup` calls `spawn` at startup), and `animate`
// shows the one the hero has, swinging it when `effects::play_events` marks
// a swing in `Motion`, and bobbing it while walking.
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;

use crate::engine::mesh::Mesh as ModelMesh;
use crate::engine::scene::model_parts;
use crate::game::editor::Editor;
use crate::game::ui::screen::Screen;
use crate::game::Game;
use super::effects::{Motion, SWING_TIME};

/// The weapon held in view: the sword (`sword`) or the knife.
#[derive(Component)]
pub(super) struct Weapon {
	sword: bool,
}

/// Shows the weapon the hero has (the sword once equipped), swinging it
/// through an arc on attacks and bobbing it while walking. It's put away in
/// the main menu and the Generator, and while the level editor is open.
pub(super) fn animate(
	game: Res<Game>,
	editor: Option<Res<Editor>>,
	time: Res<Time>,
	motion: Res<Motion>,
	mut weapons: Query<(&Weapon, &mut Transform, &mut Visibility)>,
	mut last_feet: Local<Vec3>,
) {
	let world = &game.levels[game.depth];
	let editing = editor.is_some_and(|e| e.active);
	let playing = !editing && !matches!(game.screen, Screen::MainMenu { .. } | Screen::Generator { .. });
	let t = time.elapsed_secs();
	let feet = Vec3::from_array(world.player.pos.to_array());
	let moving = ((feet - *last_feet).length() / time.delta_secs().max(1e-4)).min(8.0) / 8.0;
	*last_feet = feet;
	let p = ((t - motion.swing) / SWING_TIME).clamp(0.0, 1.0);
	let swinging = p < 1.0;
	// Ease in and out across the arc
	let e = p * p * (3.0 - 2.0 * p);
	let arc = (PI * p).sin();
	for (weapon, mut transform, mut visibility) in &mut weapons {
		let shown = playing && weapon.sword == world.hero.sword;
		*visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
		let mut tr = rest(weapon.sword);
		if swinging {
			// From the right, across and down to the left
			let sweep = Quat::from_rotation_y(0.8 - 1.8 * e) * Quat::from_rotation_x(-1.2 * arc);
			tr.rotation = sweep * tr.rotation;
			tr.translation += Vec3::new(0.15 - 0.5 * e, 0.1 * arc, -0.15 * arc);
		} else {
			// A gentle bob while walking, and a slow sway at rest
			tr.translation += Vec3::new(0.0, (t * 10.0).sin() * 0.02 * moving + (t * 1.5).sin() * 0.004, 0.0);
		}
		*transform = tr;
	}
}

/// Loads the sword and the knife, tinted steel-grey, and hangs both from
/// `camera`, hidden until `animate` shows one.
pub(super) fn spawn(
	commands: &mut Commands,
	meshes: &mut Assets<Mesh>,
	materials: &mut Assets<StandardMaterial>,
	images: &mut Assets<Image>,
	camera: Entity,
) {
	// Steel-grey: the models' colors come out white without the kit's texture
	let steel = [
		(true, "Objects/Sword.glb", glam::Vec3::new(0.75, 0.75, 0.8)),
		(false, "Objects/Knife.glb", glam::Vec3::new(0.7, 0.7, 0.72)),
	];
	for (sword, model, tint) in steel {
		let Ok(mut mesh) = ModelMesh::load(model) else { continue };
		for v in &mut mesh.vertices {
			v.color *= tint;
		}
		let parts = model_parts(&mesh, meshes, materials, images);
		let root = commands.spawn((Weapon { sword }, rest(sword), Visibility::Hidden)).id();
		for (part, material) in parts {
			let child = commands.spawn((Mesh3d(part), MeshMaterial3d(material), Transform::default())).id();
			commands.entity(root).add_child(child);
		}
		commands.entity(camera).add_child(root);
	}
}

/// How each weapon rests in view: off to the lower right, tilted in.
fn rest(sword: bool) -> Transform {
	if sword {
		// The sword model hangs blade down from its hilt: flip it up, lean it forward and in
		Transform::from_xyz(0.34, -0.36, -0.6)
			.with_rotation(Quat::from_rotation_z(0.35) * Quat::from_rotation_x(-0.9) * Quat::from_rotation_z(PI))
			.with_scale(Vec3::splat(0.75))
	} else {
		// The knife's blade runs along +X: point it forward, tipped up
		Transform::from_xyz(0.3, -0.3, -0.5)
			.with_rotation(Quat::from_rotation_x(0.35) * Quat::from_rotation_y(FRAC_PI_2))
			.with_scale(Vec3::splat(1.6))
	}
}
