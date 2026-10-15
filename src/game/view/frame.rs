// Drawing each frame, last of all the systems: works out how many character
// cells fit the window, has `Game` draw the HUD into that grid, turns the grid
// into pixels in the HUD image, and sizes the 3D view's image, the text
// filter, and the HUD sprite to the window. The images and entities it sizes
// were made by setup.rs and are kept in its `Display`.
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::window::PrimaryWindow;

use crate::engine::font::{GLYPH_H, GLYPH_W};
use crate::engine::text_filter::TextFilter;
use crate::game::editor::{self, Editor};
use crate::game::Game;
use super::setup::{scene_target, Display};
use super::WorldCamera;

/// The 3D view renders at 1 / this of the screen's resolution, in each
/// direction. Lower is sharper in textured mode but slower.
const SCENE_DIVISOR: u32 = 4;

/// Draws the HUD into its image (only when it changed) and fits the 3D view,
/// the filter quad and the HUD sprite to the window.
pub(super) fn draw_frame(
	windows: Query<&Window, With<PrimaryWindow>>,
	mut screen: ResMut<Display>,
	mut game: ResMut<Game>,
	mut images: ResMut<Assets<Image>>,
	mut filters: ResMut<Assets<TextFilter>>,
	mut sprites: Query<&mut Sprite>,
	mut transforms: Query<&mut Transform, Without<WorldCamera>>,
	editor: Option<Res<Editor>>,
) {
	let Ok(window) = windows.single() else { return };

	// As many character cells as fit the window at the font's real pixel size
	let scale = game.settings.text_scale;
	let cell_w = GLYPH_W as u32 * scale;
	let cell_h = GLYPH_H as u32 * scale;
	let cols = (window.physical_width() / cell_w) as usize;
	let rows = (window.physical_height() / cell_h) as usize;
	if cols == 0 || rows == 0 {
		return;
	}
	game.render(cols, rows, editor.as_ref().is_some_and(|e| e.active));
	if let Some(editor) = &editor {
		editor::draw_hud(editor, &mut game.framebuffer);
	}

	let size = Extent3d {
		width: (cols * GLYPH_W) as u32,
		height: (rows * GLYPH_H) as u32,
		depth_or_array_layers: 1,
	};
	// Redrawn and re-uploaded only when the text changed: the image is several MB
	if screen.hud != game.framebuffer.color_buffer {
		let Some(mut image) = images.get_mut(&screen.image) else { return };
		if image.texture_descriptor.size != size {
			image.resize(size);
		}
		let Some(pixels) = image.data.as_mut() else { return };
		game.glyphs.draw(&game.framebuffer, pixels);
		screen.hud.clone_from(&game.framebuffer.color_buffer);
	}

	let text_filter = game.settings.text_filter;
	fit_scene(&mut images, &mut filters, &screen, (cols, rows), (cell_w, cell_h), text_filter);

	// Show both at the text scale's screen pixels per font pixel. Sizes are in
	// logical pixels, so divide out the display's scale factor.
	let logical = Vec2::new((cols as u32 * cell_w) as f32, (rows as u32 * cell_h) as f32) / window.scale_factor();
	if let Ok(mut sprite) = sprites.get_mut(screen.sprite) {
		sprite.custom_size = Some(logical);
	}
	if let Ok(mut transform) = transforms.get_mut(screen.filter_quad) {
		transform.scale = logical.extend(1.0);
	}
}

/// Resizes the 3D view's image to suit a `cols` x `rows` grid of cells, each
/// `cell` (width, height) screen pixels, and tells the text filter the grid
/// size and whether it's on (`text_filter`).
fn fit_scene(
	images: &mut Assets<Image>,
	filters: &mut Assets<TextFilter>,
	screen: &Display,
	(cols, rows): (usize, usize),
	(cell_w, cell_h): (u32, u32),
	text_filter: bool,
) {
	// The 3D view renders at a fixed fraction of the screen, whatever the text
	// size; the filter averages however many of its pixels fall in a cell
	let scene_size = Extent3d {
		width: (cols as u32 * cell_w / SCENE_DIVISOR).max(1),
		height: (rows as u32 * cell_h / SCENE_DIVISOR).max(1),
		depth_or_array_layers: 1,
	};
	if images.get(&screen.scene).is_some_and(|s| s.texture_descriptor.size != scene_size) {
		images.insert(&screen.scene, scene_target(scene_size.width, scene_size.height)).ok();
	}
	let grid = Vec4::new(cols as f32, rows as f32, if text_filter { 1.0 } else { 0.0 }, 0.0);
	if filters.get(&screen.filter).is_some_and(|f| f.grid != grid) {
		if let Some(mut filter) = filters.get_mut(&screen.filter) {
			filter.grid = grid;
		}
	}
}
