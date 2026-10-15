// The player: where they stand, which way they look, and how they move,
// either tile by tile (in combat) or freely.
use glam::{Mat4, Vec3};

use super::world::{tile_center, Blocker, DIRECTIONS, EYE_HEIGHT, TILE, WALL_THICKNESS};
use super::terrain::{Terrain, STEP_HEIGHT};
use crate::dungeons::mapgen::MapGrid;

/// What the player can do in one turn. The window maps keys to these.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Action {
	Forward,
	Back,
	TurnLeft,
	TurnRight,
	/// Swing at what's in front
	Attack,
}

pub(super) struct Player {
	pub(super) x: i32,
	pub(super) y: i32,
	pub(super) z: i32,
	/// Index into DIRECTIONS: 0 north, 1 east, 2 south, 3 west
	pub(super) facing: usize,
	/// Mouselook, in radians, on top of `facing`. Yaw stays within ±45°: past
	/// that, `facing` turns to the nearer direction so W walks where you look.
	pub(super) look_yaw: f32,
	pub(super) look_pitch: f32,
	/// Where the feet are, in world space. In combat it's always the center of
	/// tile (x, y); walking freely it can be anywhere, and (x, y) is the tile
	/// it's in.
	pub(super) pos: Vec3,
}

/// Furthest you can look up or down.
pub(super) const MAX_PITCH: f32 = 80.0 * std::f32::consts::PI / 180.0;

/// Free walking speed, meters per second.
pub(super) const WALK_SPEED: f32 = 5.0;
/// Holding Sprint multiplies the walking speed by this.
pub(super) const SPRINT_MULTIPLIER: f32 = 1.5;
/// How close the player's center can get to a wall, in meters.
pub(super) const PLAYER_RADIUS: f32 = 0.6;

impl Player {
	pub(super) fn new(x: i32, y: i32, z: i32) -> Self {
		Self { x, y, z, facing: 0, look_yaw: 0.0, look_pitch: 0.0, pos: tile_center(x, y) }
	}

	/// Where the player looks, flat on the floor: the facing turned by mouselook.
	pub(super) fn heading(&self) -> Vec3 {
		let (dx, dy) = self.forward();
		glam::Quat::from_rotation_y(-self.look_yaw) * Vec3::new(dx as f32, 0.0, dy as f32)
	}

	/// Puts the feet back in the middle of the current tile and squares the
	/// view to the facing, for turn-by-turn play.
	pub(super) fn snap_to_tile(&mut self, terrain: &Terrain) {
		self.pos = tile_center(self.x, self.y) + Vec3::Y * terrain.tile(self.x, self.y);
		self.look_yaw = 0.0;
	}

	/// True when a square of PLAYER_RADIUS at `pos` overlaps a wall or one of
	/// the `blockers` (door frames, torches). Wall panels' faces stand
	/// WALL_THICKNESS inside the rock tiles' edges, so walls count as that
	/// much closer.
	pub(super) fn blocked(map: &MapGrid, blockers: &[Blocker], pos: Vec3) -> bool {
		let r = PLAYER_RADIUS;
		let w = PLAYER_RADIUS + WALL_THICKNESS;
		let in_wall = [(-w, -w), (w, -w), (-w, w), (w, w)].iter().any(|(ox, oz)| {
			let tx = ((pos.x + ox) / TILE).floor() as i32;
			let ty = ((pos.z + oz) / TILE).floor() as i32;
			map.is_wall(tx, ty)
		});
		in_wall
			|| blockers.iter().any(|b| {
				pos.x + r > b.min.x && pos.x - r < b.max.x && pos.z + r > b.min.z && pos.z - r < b.max.z
			})
	}

	/// Walks freely for `dt` seconds: `forward` and `strafe` are -1 to 1.
	/// Each axis moves on its own, so the player slides along walls. The feet
	/// follow the floor up and down; a rise or drop bigger than a step blocks.
	pub(super) fn walk(&mut self, map: &MapGrid, blockers: &[Blocker], terrain: &Terrain, forward: f32, strafe: f32, dt: f32) {
		let ahead = self.heading();
		let right = Vec3::new(-ahead.z, 0.0, ahead.x);
		let step = (ahead * forward + right * strafe).normalize_or_zero() * WALK_SPEED * dt;
		for axis in [Vec3::X, Vec3::Z] {
			let next = self.pos + step * axis;
			let climb = (terrain.at(next) - terrain.at(self.pos)).abs();
			if !Self::blocked(map, blockers, next) && climb <= STEP_HEIGHT {
				self.pos = next;
			}
		}
		self.pos.y = terrain.at(self.pos);
		self.x = (self.pos.x / TILE).floor() as i32;
		self.y = (self.pos.z / TILE).floor() as i32;
	}

	/// Turns the view by mouse movement, in radians. Positive yaw turns right,
	/// positive pitch looks up.
	pub(super) fn look(&mut self, yaw: f32, pitch: f32) {
		use std::f32::consts::FRAC_PI_2;
		self.look_yaw += yaw;
		self.look_pitch = (self.look_pitch + pitch).clamp(-MAX_PITCH, MAX_PITCH);
		while self.look_yaw > FRAC_PI_2 / 2.0 {
			self.look_yaw -= FRAC_PI_2;
			self.facing = (self.facing + 1) % 4;
		}
		while self.look_yaw < -FRAC_PI_2 / 2.0 {
			self.look_yaw += FRAC_PI_2;
			self.facing = (self.facing + 3) % 4;
		}
	}

	/// 0 north, 1 east, 2 south, 3 west
	pub(super) fn facing(&self) -> usize {
		self.facing
	}

	pub(super) fn forward(&self) -> (i32, i32) {
		DIRECTIONS[self.facing]
	}

	/// Where the eye is, in world space.
	pub(super) fn eye(&self) -> Vec3 {
		self.pos + Vec3::new(0.0, EYE_HEIGHT, 0.0)
	}

	pub(super) fn view_matrix(&self) -> Mat4 {
		// Map east (+X) is to the right of north (-Z), so turning right is a
		// negative rotation about +Y
		let dir = self.heading();
		let dir = Vec3::new(dir.x * self.look_pitch.cos(), self.look_pitch.sin(), dir.z * self.look_pitch.cos());
		glam::camera::rh::view::look_to_mat4(self.eye(), dir, Vec3::Y)
	}

	/// One action, one turn. Walls, and rises too high to step, block movement.
	/// Returns true if the player did something.
	pub(super) fn act(&mut self, action: Action, map: &MapGrid, terrain: &Terrain) -> bool {
		match action {
			Action::Forward => self.step(map, terrain, 1),
			Action::Back => self.step(map, terrain, -1),
			Action::TurnLeft => {
				self.facing = (self.facing + 3) % 4;
				true
			}
			Action::TurnRight => {
				self.facing = (self.facing + 1) % 4;
				true
			}
			// The world handles attacks
			Action::Attack => false,
		}
	}

	pub(super) fn step(&mut self, map: &MapGrid, terrain: &Terrain, sign: i32) -> bool {
		let (dx, dy) = self.forward();
		let (nx, ny) = (self.x + dx * sign, self.y + dy * sign);
		if map.is_wall(nx, ny) || !terrain.can_step((self.x, self.y), (nx, ny)) {
			return false;
		}
		(self.x, self.y) = (nx, ny);
		self.pos = tile_center(nx, ny) + Vec3::Y * terrain.tile(nx, ny);
		true
	}
}
