// view/: the Bevy side of the game. It shows the game in a Bevy window.
// Bevy draws the level's models in 3D into an offscreen image; the text
// filter shader turns that image into characters on a quad filling the
// window; the HUD (the minimap) is drawn as text on a sprite above it.
//
// `GamePlugin` below lists every system, in the order Bevy runs them each
// frame: read the keys and ask `Game` to act (input.rs), bring the 3D scene
// in step with the level (scene.rs, sprites.rs, portals.rs), play the effects
// (effects.rs, weapon.rs), then draw the frame (frame.rs). setup.rs runs once
// at startup and makes the cameras and images the rest draw into.
//
// The rules themselves live in play/; this folder only reads `Game` and
// shows it, and passes on the keys.

/// Startup: the cameras, the player's lantern, the font sheet and the HUD image.
mod setup;
/// Reading the keys and the mouse: menus, the inventory, turns and walking.
mod input;
/// The level's 3D models, the torches, the fog, and the camera at the player's eye.
mod scene;
/// Camera-facing sprites for monsters and items, and the chests' models.
mod sprites;
/// The glowing columns of light on the staircases.
mod portals;
/// Particles, monsters' movement, and the shake of the view when hurt.
mod effects;
/// The weapon in the player's hand, swinging and bobbing.
mod weapon;
/// Drawing each frame: the HUD text and the 3D view, sized to the window.
mod frame;

use bevy::prelude::*;

use crate::engine::text_filter::TextFilterPlugin;
use crate::game::editor::{self, Editor};
use scene::SpriteCache;

/// Runs the game in the primary window. Insert a `Game` resource
/// (from `Game::new`) before running.
pub struct GamePlugin;

impl Plugin for GamePlugin {
	fn build(&self, app: &mut App) {
		app.add_plugins(TextFilterPlugin)
			.init_resource::<SpriteCache>()
			.add_systems(Startup, (setup::setup, effects::setup).chain())
			.add_systems(
				Update,
				(
					input::handle_input,
					editor::update.run_if(resource_exists::<Editor>),
					input::mouse_look,
					scene::sync_scene,
					effects::play_events,
					sprites::sync_enemies,
					sprites::sync_items,
					sprites::sync_chests,
					portals::animate,
					scene::apply_fog,
					scene::sync_camera,
					weapon::animate,
					effects::update_particles,
					effects::ambient_particles,
					frame::draw_frame,
				)
					.chain(),
			);
		// The level editor (F1) is only in the debug binary
		if crate::game::dev_tools() {
			app.init_resource::<Editor>();
		}
	}
}

/// Marks the camera that draws the 3D models under the text.
#[derive(Component)]
pub(crate) struct WorldCamera;
