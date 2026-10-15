// Combat: the monsters, their stats, and the turn order once one is close.
use super::items::SHIELD_BLOCK;
use super::items::SWORD_BONUS;
use super::world::{Event, World};

/// The kinds of monster.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum EnemyKind {
	ThornVine,
	ShadowImp,
	GreenSpider,
	GreyRat,
	ShadowBlob,
}

/// What a kind of monster is like.
pub(super) struct EnemyStats {
	pub(super) name: &'static str,
	/// Raven icons it can be drawn with; one is picked per monster
	pub(super) icons: &'static [u32],
	/// Its letter on the map
	pub(super) glyph: char,
	pub(super) hp: i32,
	pub(super) damage: i32,
	/// Tiles it steps per turn; 0 never moves
	pub(super) steps: u32,
	/// Acts only every this many turns (2 is slow)
	pub(super) every: u32,
	/// How often it turns up at the top level, and how much more each level down
	pub(super) weight: u32,
	pub(super) weight_per_depth: u32,
}

impl EnemyKind {
	pub(super) const ALL: [EnemyKind; 5] =
		[EnemyKind::ThornVine, EnemyKind::ShadowImp, EnemyKind::GreenSpider, EnemyKind::GreyRat, EnemyKind::ShadowBlob];

	pub(super) fn stats(self) -> EnemyStats {
		let base = EnemyStats {
			name: "",
			icons: &[],
			glyph: '?',
			hp: 3,
			damage: 1,
			steps: 1,
			every: 1,
			weight: 1,
			weight_per_depth: 0,
		};
		match self {
			EnemyKind::ThornVine => EnemyStats {
				name: "Thorn Vine",
				icons: &[1067],
				glyph: 'v',
				hp: 5,
				steps: 0,
				weight: 2,
				weight_per_depth: 1,
				..base
			},
			EnemyKind::ShadowImp => EnemyStats {
				name: "Shadow Imp",
				icons: &[1381, 1382, 1383, 1384],
				glyph: 'i',
				weight: 3,
				weight_per_depth: 1,
				..base
			},
			EnemyKind::GreenSpider => {
				EnemyStats { name: "Green Spider", icons: &[1066, 1070], glyph: 's', hp: 2, steps: 2, weight: 2, ..base }
			}
			EnemyKind::GreyRat => EnemyStats { name: "Grey Rat", icons: &[861], glyph: 'r', hp: 1, weight: 5, ..base },
			EnemyKind::ShadowBlob => EnemyStats {
				name: "Shadow Blob",
				icons: &[1391],
				glyph: 'b',
				hp: 6,
				damage: 2,
				every: 2,
				weight: 0,
				weight_per_depth: 2,
				..base
			},
		}
	}

	/// A random kind, with tougher ones likelier deeper down.
	pub(super) fn pick(rng: &mut impl rand::Rng, depth: usize) -> Self {
		use rand::RngExt;
		let weight = |k: EnemyKind| {
			let s = k.stats();
			s.weight + s.weight_per_depth * depth as u32
		};
		let total: u32 = Self::ALL.iter().map(|&k| weight(k)).sum();
		let mut roll = rng.random_range(0..total);
		for k in Self::ALL {
			if roll < weight(k) {
				return k;
			}
			roll -= weight(k);
		}
		EnemyKind::GreyRat
	}
}

/// A monster on the map. `id` stays the same for its whole life, so the 3D
/// view can match its sprite to it.
pub(super) struct Enemy {
	pub(super) id: u32,
	pub(super) kind: EnemyKind,
	/// Which of its kind's icons it's drawn with
	pub(super) icon: u32,
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) hp: i32,
	/// Turns it has waited, for kinds that act only every few turns
	pub(super) waited: u32,
}

impl Enemy {
	/// A fresh monster of `kind` on tile (x, y), drawn with a random one of its icons.
	pub(super) fn new(rng: &mut impl rand::Rng, id: u32, kind: EnemyKind, x: i32, y: i32) -> Self {
		use rand::RngExt;
		let s = kind.stats();
		let icon = s.icons[rng.random_range(0..s.icons.len())];
		Self { id, kind, icon, x, y, hp: s.hp, waited: 0 }
	}
}

/// Enemies this close (in tiles, counting diagonals) start combat.
pub(super) const COMBAT_RANGE: i32 = 4;
pub(super) const PLAYER_HP: i32 = 10;
pub(super) const PLAYER_DAMAGE: i32 = 2;
/// Lines of the log kept.
pub(super) const LOG_LINES: usize = 4;

/// Combat: enemies taking their turns, and the log.
impl World {
	pub(super) fn say(&mut self, line: impl Into<String>) {
		self.log.push(line.into());
		if self.log.len() > LOG_LINES {
			self.log.remove(0);
		}
	}

	pub(super) fn enemy_at(&self, x: i32, y: i32) -> Option<usize> {
		self.enemies.iter().position(|e| (e.x, e.y) == (x, y))
	}

	/// Enemies close enough to fight.
	pub(super) fn near(&self, e: &Enemy) -> bool {
		(e.x - self.player.x).abs().max((e.y - self.player.y).abs()) <= COMBAT_RANGE
	}

	/// Starts or ends combat as enemies come and go. Starting it snaps the
	/// player to the middle of their tile.
	pub(super) fn update_combat(&mut self) {
		let combat = self.enemies.iter().any(|e| self.near(e));
		if combat && !self.combat {
			self.player.snap_to_tile(&self.terrain);
			self.say("An enemy approaches!");
		} else if !combat && self.combat {
			self.say("The way is clear.");
		}
		self.combat = combat;
	}

	/// Swings at whatever's on the tile in front: an enemy gets hit, a closed
	/// chest opens. Hitting something in combat takes a turn (as bumping into
	/// it does); a swing at nothing doesn't.
	pub(super) fn attack(&mut self) {
		self.events.push(Event::Swing);
		let (dx, dy) = self.player.forward();
		let (tx, ty) = (self.player.x + dx, self.player.y + dy);
		if let Some(i) = self.enemy_at(tx, ty) {
			self.strike(i);
		} else if let Some(c) = self.chest_at(tx, ty).filter(|&c| !self.chests[c].open) {
			self.open_chest(c);
		} else {
			return;
		}
		if self.combat {
			self.enemies_act();
		}
		self.update_combat();
	}

	/// The player hits enemy `i`, killing it if that's its last HP.
	pub(super) fn strike(&mut self, i: usize) {
		let e = &self.enemies[i];
		let name = e.kind.stats().name;
		let at = self.tile_center_of(e.x, e.y) + glam::Vec3::Y * 0.9;
		let id = e.id;
		self.enemies[i].hp -= PLAYER_DAMAGE + if self.hero.sword { SWORD_BONUS } else { 0 };
		if self.enemies[i].hp <= 0 {
			self.enemies.remove(i);
			self.events.push(Event::Kill { at });
			self.say(format!("You slay the {name}!"));
		} else {
			self.events.push(Event::Hit { id, at });
			self.say(format!("You hit the {name}."));
		}
	}

	/// The enemies' turn in combat: each one close by attacks if it's next to
	/// the player, or else steps toward them, as fast as its kind allows.
	pub(super) fn enemies_act(&mut self) {
		for i in 0..self.enemies.len() {
			if !self.near(&self.enemies[i]) {
				continue;
			}
			let stats = self.enemies[i].kind.stats();
			self.enemies[i].waited += 1;
			if self.enemies[i].waited < stats.every {
				continue;
			}
			self.enemies[i].waited = 0;

			if self.next_to_player(i) {
				self.enemy_attacks(&stats);
				continue;
			}
			for _ in 0..stats.steps {
				self.step_toward_player(i);
				if self.next_to_player(i) {
					break;
				}
			}
		}
	}

	fn next_to_player(&self, i: usize) -> bool {
		let e = &self.enemies[i];
		(self.player.x - e.x).abs() + (self.player.y - e.y).abs() == 1
	}

	fn enemy_attacks(&mut self, stats: &EnemyStats) {
		if self.hero.shield && rand::random_bool(SHIELD_BLOCK) {
			self.say(format!("Your shield turns the {} aside.", stats.name));
		} else {
			self.hero.hp -= stats.damage;
			self.events.push(Event::Hurt);
			self.say(format!("The {} hits you ({} HP).", stats.name, self.hero.hp.max(0)));
		}
	}

	/// One step toward the player: along the longer way first, then the
	/// other, if the tile is free (no wall, monster, or chest).
	fn step_toward_player(&mut self, i: usize) {
		let (px, py) = (self.player.x, self.player.y);
		let (ex, ey) = (self.enemies[i].x, self.enemies[i].y);
		let (dx, dy) = (px - ex, py - ey);
		let steps = if dx.abs() >= dy.abs() {
			[(dx.signum(), 0), (0, dy.signum())]
		} else {
			[(0, dy.signum()), (dx.signum(), 0)]
		};
		for (sx, sy) in steps {
			let (nx, ny) = (ex + sx, ey + sy);
			if (sx, sy) != (0, 0)
				&& !self.map.is_wall(nx, ny)
				&& self.terrain.can_step((ex, ey), (nx, ny))
				&& self.enemy_at(nx, ny).is_none()
				&& self.chest_at(nx, ny).is_none()
				&& (nx, ny) != (px, py)
			{
				self.enemies[i].x = nx;
				self.enemies[i].y = ny;
				return;
			}
		}
	}
}
