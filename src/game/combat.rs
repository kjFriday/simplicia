// Combat: slimes, damage, and the turn order once an enemy is close.
use super::items::SHIELD_BLOCK;
use super::world::World;

/// A monster on the map. `id` stays the same for its whole life, so the 3D
/// view can match its model to it.
pub(super) struct Enemy {
	pub(super) id: u32,
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) hp: i32,
}

/// Enemies this close (in tiles, counting diagonals) start combat.
pub(super) const COMBAT_RANGE: i32 = 4;
pub(super) const PLAYER_HP: i32 = 10;
pub(super) const ENEMY_HP: i32 = 3;
pub(super) const PLAYER_DAMAGE: i32 = 2;
pub(super) const ENEMY_DAMAGE: i32 = 1;
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
			self.player.snap_to_tile();
			self.say("An enemy approaches!");
		} else if !combat && self.combat {
			self.say("The way is clear.");
		}
		self.combat = combat;
	}

	/// The enemies' turn in combat: each one close by attacks if it's next to
	/// the player, or else steps toward them.
	pub(super) fn enemies_act(&mut self) {
		let (px, py) = (self.player.x, self.player.y);
		for i in 0..self.enemies.len() {
			if !self.near(&self.enemies[i]) {
				continue;
			}
			let (ex, ey) = (self.enemies[i].x, self.enemies[i].y);
			let (dx, dy) = (px - ex, py - ey);
			if dx.abs() + dy.abs() == 1 {
				if self.hero.shield && rand::random_bool(SHIELD_BLOCK) {
					self.say("Your shield turns the slime aside.");
				} else {
					self.hero.hp -= ENEMY_DAMAGE;
					self.say(format!("The slime hits you ({} HP).", self.hero.hp.max(0)));
				}
				continue;
			}
			// Step along the longer way first, then the other, if it's free
			let steps = if dx.abs() >= dy.abs() {
				[(dx.signum(), 0), (0, dy.signum())]
			} else {
				[(0, dy.signum()), (dx.signum(), 0)]
			};
			for (sx, sy) in steps {
				let (nx, ny) = (ex + sx, ey + sy);
				if (sx, sy) != (0, 0) && !self.map.is_wall(nx, ny) && self.enemy_at(nx, ny).is_none() && (nx, ny) != (px, py) {
					self.enemies[i].x = nx;
					self.enemies[i].y = ny;
					break;
				}
			}
		}
	}
}
