// The engine: everything that isn't specific to this game. Loading models
// and fonts, the HUD's character grid, handing meshes to Bevy, and the text
// filter that draws the 3D view as characters. The game (src/game) builds on
// these and decides what goes in the world.

/// The bitmap font: glyph coverage, and drawing the HUD grid into pixels.
pub mod font;
/// The HUD's character grid and the functions that write text into it.
pub mod hud;
/// Raven icons, and drawing them as camera-facing sprites.
pub mod icons;
/// Triangle meshes and textures, and loading them from .obj and .glb files.
pub mod mesh;
/// Every model file under models/, built into the executable.
pub mod models;
/// Models placed in the world, and turning them into Bevy meshes.
pub mod scene;
/// The shader material that draws the 3D view as characters.
pub mod text_filter;

struct Engine;

impl Engine {

}
