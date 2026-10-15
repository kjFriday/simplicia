// The player: where they stand, which way they look, and how they move,
// either tile by tile (in combat) or freely.
//
// `World` (world.rs) owns the `Player` and calls in here to move them; the
// floor heights come from terrain.rs. view/ turns key presses into an
// `Action` and asks for `view_matrix` to place the camera.
use glam::{Mat4, Vec3};

use super::terrain::{Terrain, STEP_HEIGHT};
use super::world::{tile_center, Blocker, DIRECTIONS, EYE_HEIGHT, TILE, WALL_THICKNESS};
use crate::dungeons::mapgen::MapGrid;

/// Furthest you can look up or down.
pub(crate) const MAX_PITCH: f32 = 80.0 * std::f32::consts::PI / 180.0;

/// Free walking speed, meters per second.
pub(crate) const WALK_SPEED: f32 = 5.0;
/// Holding Sprint multiplies the walking speed by this.
pub(crate) const SPRINT_MULTIPLIER: f32 = 1.5;
/// How close the player's center can get to a wall, in meters.
pub(crate) const PLAYER_RADIUS: f32 = 0.6;

/// What the player can do in one turn. The window maps keys to these.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Action {
	Forward,
	Back,
	TurnLeft,
	TurnRight,
	/// Swing at what's in front
	Attack,
}

pub(crate) struct Player {
	pub(crate) x: i32,
	pub(crate) y: i32,
	pub(crate) z: i32,
	/// Index into DIRECTIONS: 0 north, 1 east, 2 south, 3 west
	pub(crate) facing: usize,
	/// Mouselook, in radians, on top of `facing`. Yaw stays within ±45°: past
	/// that, `facing` turns to the nearer direction so W walks where you look.
	pub(crate) look_yaw: f32,
	pub(crate) look_pitch: f32,
	/// Where the feet are, in world space. In combat it's always the center of
	/// tile (x, y); walking freely it can be anywhere, and (x, y) is the tile
	/// it's in.
	pub(crate) pos: Vec3,
}

impl Player {
	pub(crate) fn new(x: i32, y: i32, z: i32) -> Self {
		Self { x, y, z, facing: 0, look_yaw: 0.0, look_pitch: 0.0, pos: tile_center(x, y) }
	}

	/// Where the player looks, flat on the floor: the facing turned by mouselook.
	pub(crate) fn heading(&self) -> Vec3 {
		let (dx, dy) = self.forward();
		glam::Quat::from_rotation_y(-self.look_yaw) * Vec3::new(dx as f32, 0.0, dy as f32)
	}

	/// Puts the feet back in the middle of the current tile and squares the
	/// view to the facing, for turn-by-turn play.
	pub(crate) fn snap_to_tile(&mut self, terrain: &Terrain) {
		self.pos = tile_center(self.x, self.y) + Vec3::Y * terrain.tile(self.x, self.y);
		self.look_yaw = 0.0;
	}

	/// True when a square of PLAYER_RADIUS at `pos` overlaps a wall or one of
	/// the `blockers` (door frames, torches). Wall panels' faces stand
	/// WALL_THICKNESS inside the rock tiles' edges, so walls count as that
	/// much closer.
	pub(crate) fn blocked(map: &MapGrid, blockers: &[Blocker], pos: Vec3) -> bool {
		let r = PLAYER_RADIUS;
		let w = PLAYER_RADIUS + WALL_THICKNESS;
		let in_wall = [(-w, -w), (w, -w), (-w, w), (w, w)].iter().any(|(ox, oz)| {
			let tx = ((pos.x + ox) / TILE).floor() as i32;
			let ty = ((pos.z + oz) / TILE).floor() as i32;
			map.is_wall(tx, ty)
		});
		in_wall || blockers.iter().any(|b| b.touches(pos, r))
	}

	/// Walks freely for `dt` seconds: `forward` and `strafe` are -1 to 1.
	/// Each axis moves on its own, so the player slides along walls. The feet
	/// follow the floor up and down; a rise or drop bigger than a step blocks.
	pub(crate) fn walk(
		&mut self,
		map: &MapGrid,
		blockers: &[Blocker],
		terrain: &Terrain,
		forward: f32,
		strafe: f32,
		dt: f32,
	) {
		let ahead = self.heading();
		let right = Vec3::new(-ahead.z, 0.0, ahead.x);
		let step = (ahead * forward + right * strafe).normalize_or_zero() * WALK_SPEED * dt;
		// Blockers the player already stands in (snapped onto a chest's tile
		// when combat started, say) don't hold them, so they can walk out
		let start = self.pos;
		let solid: Vec<Blocker> =
			blockers.iter().filter(|b| !b.touches(start, PLAYER_RADIUS)).copied().collect();
		for axis in [Vec3::X, Vec3::Z] {
			let next = self.pos + step * axis;
			let climb = (terrain.at(next) - terrain.at(self.pos)).abs();
			if !Self::blocked(map, &solid, next) && climb <= STEP_HEIGHT {
				self.pos = next;
			}
		}
		self.pos.y = terrain.at(self.pos);
		self.x = (self.pos.x / TILE).floor() as i32;
		self.y = (self.pos.z / TILE).floor() as i32;
	}

	/// Turns the view by mouse movement, in radians. Positive yaw turns right,
	/// positive pitch looks up.
	pub(crate) fn look(&mut self, yaw: f32, pitch: f32) {
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
	pub(crate) fn facing(&self) -> usize {
		self.facing
	}

	/// One tile ahead, as a step in map space.
	pub(crate) fn forward(&self) -> (i32, i32) {
		DIRECTIONS[self.facing]
	}

	/// Where the eye is, in world space.
	pub(crate) fn eye(&self) -> Vec3 {
		self.pos + Vec3::new(0.0, EYE_HEIGHT, 0.0)
	}

	/// The camera transform for looking out of the player's eyes.
	pub(crate) fn view_matrix(&self) -> Mat4 {
		// Map east (+X) is to the right of north (-Z), so turning right is a
		// negative rotation about +Y
		let dir = self.heading();
		let dir = Vec3::new(dir.x * self.look_pitch.cos(), self.look_pitch.sin(), dir.z * self.look_pitch.cos());
		glam::camera::rh::view::look_to_mat4(self.eye(), dir, Vec3::Y)
	}

	/// One action, one turn. Walls, and rises too high to step, block movement.
	/// Returns true if the player did something.
	pub(crate) fn act(&mut self, action: Action, map: &MapGrid, terrain: &Terrain) -> bool {
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

	/// One tile forward (`sign` 1) or back (`sign` -1), if nothing blocks it.
	/// Returns true if the player moved.
	pub(crate) fn step(&mut self, map: &MapGrid, terrain: &Terrain, sign: i32) -> bool {
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
