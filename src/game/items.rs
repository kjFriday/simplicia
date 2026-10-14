// Items: what lies on the floor, what the hero carries, and using it.
use glam::Vec3;

use super::combat::PLAYER_HP;
use super::level_gen::IRON;
use super::world::World;

/// Things the player can pick up.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum ItemKind {
	Potion,
	Gold,
	Sword,
	Shield,
}

impl ItemKind {
	pub(super) const ALL: [ItemKind; 4] = [ItemKind::Potion, ItemKind::Gold, ItemKind::Sword, ItemKind::Shield];

	pub(super) fn name(self) -> &'static str {
		match self {
			ItemKind::Potion => "Healing Potion",
			ItemKind::Gold => "Gold",
			ItemKind::Sword => "Sword",
			ItemKind::Shield => "Shield",
		}
	}

	/// Model, tint, and how wide it's drawn (bigger than life, so it's easy to spot).
	pub(super) fn model(self) -> (&'static str, Vec3, f32) {
		match self {
			ItemKind::Potion => ("Objects/Potion_Red.glb", Vec3::new(0.9, 0.2, 0.2), 0.6),
			ItemKind::Gold => ("Objects/Plate_Small.glb", Vec3::new(1.0, 0.8, 0.2), 0.7),
			ItemKind::Sword => ("Objects/Sword.glb", IRON, 1.0),
			ItemKind::Shield => ("Objects/Shield.glb", IRON, 1.0),
		}
	}

	/// How likely each kind is to turn up, in ALL's order.
	pub(super) const WEIGHTS: [u32; 4] = [5, 3, 1, 1];
}

/// An item lying on the floor. `id` lets the 3D view match its model to it.
pub(super) struct GroundItem {
	pub(super) id: u32,
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) kind: ItemKind,
}

/// Everything about the player that goes with them from level to level.
pub(super) struct Hero {
	pub(super) hp: i32,
	pub(super) gold: u32,
	/// Carried items that aren't gold
	pub(super) bag: Vec<ItemKind>,
	pub(super) sword: bool,
	pub(super) shield: bool,
}

impl Default for Hero {
	fn default() -> Self {
		Self { hp: PLAYER_HP, gold: 0, bag: Vec::new(), sword: false, shield: false }
	}
}

/// What a potion heals.
pub(super) const POTION_HEAL: i32 = 5;
/// Extra damage with the sword equipped.
pub(super) const SWORD_BONUS: i32 = 1;
/// Chance the shield turns a hit aside.
pub(super) const SHIELD_BLOCK: f64 = 0.5;
/// Chance a furnished room has an item lying in it.
pub(super) const ITEM_CHANCE: f64 = 0.6;

/// Picking things up and using them.
impl World {
	/// Picks up whatever is lying on the player's tile.
	pub(super) fn pick_up(&mut self) {
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

	/// Uses or equips the `index`th item in the bag. A potion is used up;
	/// gear is toggled on or off. In combat, drinking takes a turn.
	pub(super) fn use_item(&mut self, index: usize) {
		let Some(&kind) = self.hero.bag.get(index) else { return };
		match kind {
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
