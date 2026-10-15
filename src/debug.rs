use bevy::prelude::*;

pub mod dungeons;
pub mod engine;
pub mod game;
pub mod history;
pub mod overworld;
pub mod quests;

pub fn main() -> std::io::Result<()> {
	// Build the first level before opening the window, so a bad model fails fast
	let state = game::Game::new().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

	App::new()
		.add_plugins(
			DefaultPlugins
				.set(WindowPlugin {
					primary_window: Some(Window {
						title: "rpg_game".into(),
						resolution: (1280u32, 720u32).into(),
						..default()
					}),
					..default()
				})
				// Pixel-art friendly: no blurring when textures are scaled
				.set(ImagePlugin::default_nearest()),
				
		)
		.insert_resource(state)
		.add_plugins(game::GamePlugin)
		.run();

	Ok(())
}
