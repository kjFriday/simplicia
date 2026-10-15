// The rules of play: a level (`World`) and everything in it. This is plain
// Rust with no Bevy in it, so it can be tested on its own.
//
// A good order to read it in:
//   1. world.rs: `World`, a level's map and contents, and moving around it.
//   2. player.rs: the player, stepping tile by tile or walking freely.
//   3. terrain.rs: floor heights, and which steps are small enough to take.
//   4. combat.rs: the monsters and their turns.
//   5. items.rs: picking things up and using them.
//   6. chests.rs: opening chests and the loot that pops out.
//
// Several files add their own `impl World` block, so `World`'s methods are
// spread across them by topic. levels/ builds a `World`; ui/ and view/ read
// it to draw it.

/// A level: the map, the player in it, and everything placed on it.
pub(crate) mod world;
/// The player: where they stand, where they look, and how they move.
pub(crate) mod player;
/// Floor heights, ramps, and how big a step can be.
pub(crate) mod terrain;
/// Monsters, their stats, and the turn order in a fight.
pub(crate) mod combat;
/// Items on the floor, the hero's bag, and using what's in it.
pub(crate) mod items;
/// Chests and the loot that pops out of them.
pub(crate) mod chests;
/// Tests for generation, movement, combat, and items.
#[cfg(test)]
mod tests;
