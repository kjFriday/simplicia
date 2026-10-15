// The player's settings: text size, ASCII mode, key binds, how the map is
// shown, and the Generator screen's sliders. The options menus in screen.rs
// change them; the rest of the game reads them (the view reads the key binds,
// the HUD reads the text size and map view).
use bevy::input::keyboard::KeyCode;

use super::controls::Bind;
use crate::game::levels::presets::GenSettings;

/// Smallest and largest text size, in screen pixels per font pixel.
pub(crate) const TEXT_SCALE_MIN: u32 = 1;
pub(crate) const TEXT_SCALE_MAX: u32 = 4;

/// How the map is shown.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum MapView {
	/// A panel in the top-right corner, around the player
	Mini,
	/// The whole level, filling the screen
	Full,
	Hidden,
}

/// Everything the options menus change.
pub(crate) struct Settings {
	/// Screen pixels per font pixel; 1 draws the 8x8 font at its real size
	pub(crate) text_scale: u32,
	/// ASCII mode: the 3D view drawn as text; off shows it textured, as it
	/// renders. F2 toggles it anywhere.
	pub(crate) text_filter: bool,
	/// One key per action, in `Bind::ALL` order
	pub(super) keys: [KeyCode; Bind::ALL.len()],
	/// How the map is shown; the Map key cycles it
	pub(crate) map: MapView,
	/// The Generator screen's settings and seed (debug builds)
	pub(crate) generator: GenSettings,
	pub(crate) generator_seed: u64,
}

impl MapView {
	pub(crate) fn next(self) -> Self {
		match self {
			MapView::Mini => MapView::Full,
			MapView::Full => MapView::Hidden,
			MapView::Hidden => MapView::Mini,
		}
	}
}

impl Default for Settings {
	fn default() -> Self {
		Self {
			text_scale: 4,
			text_filter: false,
			keys: Bind::ALL.map(Bind::default_key),
			map: MapView::Mini,
			generator: GenSettings::default(),
			generator_seed: rand::random(),
		}
	}
}

impl Settings {
	pub(crate) fn key(&self, bind: Bind) -> KeyCode {
		self.keys[bind as usize]
	}

	/// Binds `key` to `bind`. Whatever else had that key gets `bind`'s old
	/// key instead, so no two actions ever share one.
	pub(super) fn rebind(&mut self, bind: Bind, key: KeyCode) {
		let old = self.keys[bind as usize];
		if let Some(other) = self.keys.iter().position(|&k| k == key) {
			self.keys[other] = old;
		}
		self.keys[bind as usize] = key;
	}
}
