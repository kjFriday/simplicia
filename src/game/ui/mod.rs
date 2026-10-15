// The user interface: everything drawn as HUD text over the 3D view, and the
// settings the menus change. A good reading order:
// - controls: every key, rebindable or fixed, and the help text listing them
// - settings: what the options menus change (text size, key binds, map view)
// - screen: which screen is open, and how the menus answer the keyboard
// - menus: drawing the menus and the inventory
// - minimap: drawing the map panel and the HP/log readout
// The game (src/game/mod.rs) keeps a `Screen` and `Settings`, passes key
// presses to `Screen::handle`, and calls the draw functions every frame.

/// Every key and mouse button, and the help text listing them.
pub(crate) mod controls;
/// The settings the options menus change, key binds included.
pub(crate) mod settings;
/// Which screen is showing, and menu navigation by keyboard.
pub(crate) mod screen;
/// Drawing the main, pause, options, key binds, help, and inventory menus.
pub(crate) mod menus;
/// The map panel and the HP/log readout.
pub(crate) mod minimap;

#[cfg(test)]
mod tests;
