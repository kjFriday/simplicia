// Tests for the game world: generation, movement, combat, and items.
use glam::Vec3;
use rand::SeedableRng;
use rand::rngs::StdRng;

use super::combat::*;
use super::items::*;
use super::player::*;
use super::world::*;
use super::minimap;
use crate::engine::hud::Framebuffer;

#[test]
fn worlds_generate_with_player_on_floor() {
	for seed in 0..10 {
		let world = World::generate(&mut StdRng::seed_from_u64(seed), 0).unwrap();
		assert!(!world.map.is_wall(world.player.x, world.player.y), "seed {seed}");
		let tris: usize = world.objects.iter().map(|o| world.meshes[o.mesh].indices.len()).sum();
		println!("seed {seed}: {} objects, {tris} triangles", world.objects.len());
	}
}

#[test]
fn walls_block_and_turning_works() {
	let mut world = World::generate(&mut StdRng::seed_from_u64(3), 0).unwrap();
	// Walk forward until a wall stops us; we must never end up inside one
	for _ in 0..100 {
		world.player.act(Action::Forward, &world.map, &world.terrain);
		assert!(!world.map.is_wall(world.player.x, world.player.y));
	}
	let (dx, dy) = world.player.forward();
	assert!(world.map.is_wall(world.player.x + dx, world.player.y + dy));

	let facing = world.player.facing;
	for _ in 0..4 {
		world.player.act(Action::TurnRight, &world.map, &world.terrain);
	}
	assert_eq!(world.player.facing, facing);
}

#[test]
fn mouselook_turns_facing_past_45_degrees_and_clamps_pitch() {
	let mut player = Player::new(0, 0, 0);
	player.look(50f32.to_radians(), 0.0);
	assert_eq!(player.facing(), 1);
	assert!((player.look_yaw - (-40f32).to_radians()).abs() < 1e-4);
	// -40° - 10° = -50°, past -45°, so it turns back to north at +40°
	player.look(-10f32.to_radians(), 0.0);
	assert_eq!(player.facing(), 0);
	player.look(0.0, 10.0);
	assert_eq!(player.look_pitch, MAX_PITCH);
}

/// Stands the player on a floor tile next to `target`, facing it.
fn stand_next_to(world: &mut World, target: (i32, i32, i32)) {
	let target = (target.0, target.1);
	for (facing, &(dx, dy)) in DIRECTIONS.iter().enumerate() {
		let from = (target.0 - dx, target.1 - dy);
		if !world.map.is_wall(from.0, from.1) && from != target {
			world.player.x = from.0;
			world.player.y = from.1;
			world.player.facing = facing;
			return;
		}
	}
	panic!("no floor next to {target:?}");
}

#[test]
fn stairs_are_on_floor_and_stepping_on_them_is_reported() {
	for seed in 0..10 {
		let mut world = World::generate(&mut StdRng::seed_from_u64(seed), 1).unwrap();
		let down = world.stairs_down;
		let up = world.stairs_up.expect("depth 1 has a way up");
		assert!(!world.map.is_wall(down.0, down.1) && !world.map.is_wall(up.0, up.1));
		assert_ne!(down, up);

		stand_next_to(&mut world, down);
		assert_eq!(world.act(Action::Forward), Some(Arrival::StairsDown));
		stand_next_to(&mut world, up);
		assert_eq!(world.act(Action::Forward), Some(Arrival::StairsUp));
	}
}

#[test]
fn walking_slides_along_walls_and_tracks_the_tile() {
	let mut world = World::generate(&mut StdRng::seed_from_u64(4), 0).unwrap();
	world.enemies.clear();
	for _ in 0..200 {
		world.walk(1.0, 0.3, 0.05);
		assert!(!Player::blocked(&world.map, &world.blockers, world.player.pos), "walked into a wall");
		assert_eq!(world.player.x, (world.player.pos.x / TILE).floor() as i32);
	}
}

#[test]
fn combat_snaps_to_tiles_and_bumping_an_enemy_attacks_it() {
	let mut world = World::generate(&mut StdRng::seed_from_u64(6), 0).unwrap();
	world.enemies.clear();
	// An enemy two tiles ahead of the player, along open floor
	let facing = open_facing(&world.map, (world.player.x, world.player.y, 0));
	world.player.facing = facing;
	world.player.pos += Vec3::new(0.7, 0.0, 0.3);
	let (dx, dy) = DIRECTIONS[facing];
	let (ex, ey) = (world.player.x + 2 * dx, world.player.y + 2 * dy);
	world.enemies.push(Enemy::new(&mut StdRng::seed_from_u64(1), 0, EnemyKind::ShadowImp, ex, ey));
	world.update_combat();
	assert!(world.combat);
	assert_eq!(world.player.pos, tile_center(world.player.x, world.player.y));

	// Waiting a turn (stepping back) lets it close in, then bump it until it dies
	world.act(Action::Back);
	for _ in 0..10 {
		if world.enemies.is_empty() {
			break;
		}
		world.player.facing = facing;
		world.act(Action::Forward);
	}
	assert!(world.enemies.is_empty(), "the slime should be dead");
	assert!(world.hero.hp < PLAYER_HP, "it should have hit back");
	assert!(!world.combat);
}

#[test]
fn items_are_picked_up_and_potions_heal_and_gear_toggles() {
	let mut world = World::generate(&mut StdRng::seed_from_u64(8), 0).unwrap();
	world.enemies.clear();
	world.items.clear();
	let (x, y) = (world.player.x, world.player.y);
	world.items.push(GroundItem { id: 0, x, y, kind: ItemKind::Potion, from: None });
	world.items.push(GroundItem { id: 1, x, y, kind: ItemKind::Sword, from: None });
	world.items.push(GroundItem { id: 2, x, y, kind: ItemKind::Gold, from: None });
	world.pick_up();
	assert!(world.items.is_empty());
	assert_eq!(world.hero.bag, vec![ItemKind::Potion, ItemKind::Sword]);
	assert_eq!(world.hero.gold, 10);

	world.hero.hp = 3;
	world.use_item(0);
	assert_eq!(world.hero.hp, 3 + POTION_HEAL);
	assert_eq!(world.hero.bag, vec![ItemKind::Sword]);
	world.use_item(0);
	assert!(world.hero.sword);
}

#[test]
fn top_level_has_no_way_up_and_turning_never_triggers_stairs() {
	let mut world = World::generate(&mut StdRng::seed_from_u64(5), 0).unwrap();
	assert!(world.stairs_up.is_none());
	assert_eq!(world.act(Action::TurnRight), None);
}

#[test]
fn minimap_draws_in_the_top_right_and_reveals_only_nearby_tiles() {
	let world = World::generate(&mut StdRng::seed_from_u64(2), 0).unwrap();
	assert!(world.has_seen(world.player.x, world.player.y, false));
	assert!(!world.has_seen(world.stairs_down.0, world.stairs_down.1, false) || {
		let (dx, dy) = (world.stairs_down.0 - world.player.x, world.stairs_down.1 - world.player.y);
		dx.abs() <= REVEAL_RADIUS && dy.abs() <= REVEAL_RADIUS
	});

	let mut fb = Framebuffer::new(120, 40);
	minimap::draw(&world, &mut fb, false, false);
	let row0: String = fb.color_buffer[..120].iter().map(|c| c.ch).collect();
	assert!(row0.ends_with('┐') && row0.contains("Depth 1"), "{row0}");
	let arrow = ['▲', '►', '▼', '◄'][world.player.facing];
	assert!(fb.color_buffer.iter().any(|c| c.ch == arrow));
	// Nothing drawn in the left half
	assert!(fb.color_buffer.iter().enumerate().all(|(i, c)| i % 120 >= 60 || c.ch == ' '));
}

/// A level with no monsters or items, the player facing open floor for at
/// least `reach` tiles; returns the direction faced.
fn open_corridor(seed: u64, reach: i32) -> (World, (i32, i32)) {
	for s in seed.. {
		let mut world = World::generate(&mut StdRng::seed_from_u64(s), 0).unwrap();
		world.enemies.clear();
		world.items.clear();
		for f in 0..4 {
			let (dx, dy) = DIRECTIONS[f];
			let (px, py) = (world.player.x, world.player.y);
			if (1..=reach).all(|k| !world.map.is_wall(px + dx * k, py + dy * k)) {
				world.player.facing = f;
				return (world, (dx, dy));
			}
		}
	}
	unreachable!()
}

#[test]
fn enemy_kinds_move_at_their_own_speed() {
	let (mut world, (dx, dy)) = open_corridor(20, 4);
	let (px, py) = (world.player.x, world.player.y);
	let mut rng = StdRng::seed_from_u64(2);
	world.enemies.push(Enemy::new(&mut rng, 0, EnemyKind::ThornVine, px + 4 * dx, py + 4 * dy));
	world.update_combat();
	world.enemies_act();
	assert_eq!((world.enemies[0].x, world.enemies[0].y), (px + 4 * dx, py + 4 * dy), "vines never move");

	world.enemies[0] = Enemy::new(&mut rng, 0, EnemyKind::GreenSpider, px + 4 * dx, py + 4 * dy);
	world.enemies_act();
	assert_eq!((world.enemies[0].x, world.enemies[0].y), (px + 2 * dx, py + 2 * dy), "spiders step twice");

	world.enemies[0] = Enemy::new(&mut rng, 0, EnemyKind::ShadowBlob, px + 4 * dx, py + 4 * dy);
	world.enemies_act();
	assert_eq!((world.enemies[0].x, world.enemies[0].y), (px + 4 * dx, py + 4 * dy), "blobs wait a turn");
	world.enemies_act();
	assert_eq!((world.enemies[0].x, world.enemies[0].y), (px + 3 * dx, py + 3 * dy));
}

#[test]
fn every_enemy_kind_has_icons_and_a_map_letter() {
	for kind in EnemyKind::ALL {
		let s = kind.stats();
		assert!(!s.icons.is_empty() && s.glyph.is_ascii_lowercase() && s.hp > 0, "{kind:?}");
	}
}
