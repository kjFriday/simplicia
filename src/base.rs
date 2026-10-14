use bevy::prelude::*;

pub mod overworld;
pub mod quests;
pub mod history;
pub mod dungeons;
pub mod renderer;


pub fn main() -> std::io::Result<()>{
	// Build the first level before opening the window, so a bad model fails fast
	let renderer = renderer::Renderer::new()
		.map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

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
		.insert_resource(renderer)
		.add_plugins(renderer::RendererPlugin)
		.run();

	Ok(())
}

// Embeds every file under models/ into the executable, so the exe ships
// with its model data and needs no folder beside it at runtime.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf)>) {
	let Ok(entries) = fs::read_dir(dir) else { return };
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			collect(&path, root, out);
		} else if path.file_name().is_some_and(|n| n != ".gitkeep") {
			let name = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
			out.push((name, fs::canonicalize(&path).unwrap()));
		}
	}
}
