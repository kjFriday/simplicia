// Items: what lies on the floor, what the hero carries, and using it.
//
// Ground items live in `World::items` and the hero in `World::hero`
// (world.rs). levels/ scatters items when it fills a level, chests.rs drops
// more out of chests, and ui/ draws the inventory.
use super::combat::PLAYER_HP;
use super::world::World;

/// What a potion heals.
pub(crate) const POTION_HEAL: i32 = 5;
/// Extra damage with the sword equipped.
pub(crate) const SWORD_BONUS: i32 = 1;
/// Chance the shield turns a hit aside.
pub(crate) const SHIELD_BLOCK: f64 = 0.5;

/// Things the player can pick up.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum ItemKind {
	Potion,
	Gold,
	Sword,
	Shield,
}

impl ItemKind {
	pub(crate) const ALL: [ItemKind; 4] = [ItemKind::Potion, ItemKind::Gold, ItemKind::Sword, ItemKind::Shield];

	/// How likely each kind is to turn up, in ALL's order.
	pub(crate) const WEIGHTS: [u32; 4] = [5, 3, 1, 1];

	/// Its name, as the log and the inventory show it.
	pub(crate) fn name(self) -> &'static str {
		match self {
			ItemKind::Potion => "Healing Potion",
			ItemKind::Gold => "Gold",
			ItemKind::Sword => "Sword",
			ItemKind::Shield => "Shield",
		}
	}

	/// Its letter on the map.
	pub(crate) fn glyph(self) -> char {
		match self {
			ItemKind::Potion => '!',
			ItemKind::Gold => '$',
			ItemKind::Sword => ')',
			ItemKind::Shield => '[',
		}
	}

	/// The Raven icon it's drawn with, on the floor.
	pub(crate) fn icon(self) -> u32 {
		match self {
			ItemKind::Potion => 123,
			ItemKind::Gold => 130,
			ItemKind::Sword => 1729,
			ItemKind::Shield => 2113,
		}
	}

	/// A random kind, as often as WEIGHTS says.
	pub(crate) fn random(rng: &mut impl rand::Rng) -> Self {
		use rand::RngExt;
		let total: u32 = Self::WEIGHTS.iter().sum();
		let mut roll = rng.random_range(0..total);
		for (kind, weight) in Self::ALL.into_iter().zip(Self::WEIGHTS) {
			if roll < weight {
				return kind;
			}
			roll -= weight;
		}
		ItemKind::Potion
	}
}

/// An item lying on the floor. `id` lets the 3D view match its sprite to it.
pub(crate) struct GroundItem {
	pub(crate) id: u32,
	pub(crate) x: i32,
	pub(crate) y: i32,
	pub(crate) kind: ItemKind,
	/// The chest it popped out of, so the view can show it flying out
	pub(crate) from: Option<(i32, i32)>,
}

/// Everything about the player that goes with them from level to level.
pub(crate) struct Hero {
	pub(crate) hp: i32,
	pub(crate) gold: u32,
	/// Carried items that aren't gold
	pub(crate) bag: Vec<ItemKind>,
	pub(crate) sword: bool,
	pub(crate) shield: bool,
}

impl Default for Hero {
	fn default() -> Self {
		Self { hp: PLAYER_HP, gold: 0, bag: Vec::new(), sword: false, shield: false }
	}
}

/// Picking things up and using them.
impl World {
	/// Picks up whatever is lying on the player's tile.
	pub(crate) fn pick_up(&mut self) {
		let here = (self.player.x, self.player.y);
		while let Some(i) = self.items.iter().position(|it| (it.x, it.y) == here) {
			let kind = self.items.remove(i).kind;
			match kind {
				ItemKind::Gold => {
					self.hero.gold += 10;
					self.say(format!("You pick up 10 gold ({}).", self.hero.gold));
				}
				_ => {
					self.hero.bag.push(kind);
					self.say(format!("You pick up a {}.", kind.name()));
				}
			}
		}
	}

	/// Uses or equips the `index`th item in the bag. A potion is used up
	/// (kept at full HP); gear is toggled on or off. In combat, drinking
	/// takes a turn.
	pub(crate) fn use_item(&mut self, index: usize) {
		let Some(&kind) = self.hero.bag.get(index) else { return };
		match kind {
			ItemKind::Potion if self.hero.hp >= PLAYER_HP => self.say("You're already at full health."),
			ItemKind::Potion => {
				self.hero.bag.remove(index);
				self.hero.hp = (self.hero.hp + POTION_HEAL).min(PLAYER_HP);
				self.say(format!("You drink the potion ({} HP).", self.hero.hp));
				if self.combat {
					self.enemies_act();
				}
			}
			ItemKind::Sword => {
				self.hero.sword = !self.hero.sword;
				self.say(if self.hero.sword { "You draw the sword." } else { "You put the sword away." });
			}
			ItemKind::Shield => {
				self.hero.shield = !self.hero.shield;
				self.say(if self.hero.shield { "You ready the shield." } else { "You sling the shield." });
			}
			ItemKind::Gold => {}
		}
	}
}
