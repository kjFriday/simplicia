// Effects: what the view shows happening, beyond the level itself.
//   - Particles: bursts when things are hit, die, or chests open, plus
//     drifting embers by the lights, dust around the player, and a shimmer
//     of motes around monsters.
//   - The weapon in the player's hand (a sword once equipped, a knife till
//     then), which swings on every attack and bobs while walking.
//   - A shake of the view when the player is hurt.
// The world reports what happened as `Event`s; `play_events` turns them
// into effects.
use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use rand::RngExt;

use super::editor::Editor;
use super::window::WorldCamera;
use super::world::Event;
use super::Game;
use crate::engine::mesh::Mesh as ModelMesh;
use crate::engine::scene::model_parts;

/// Most particles alive at once; ambient ones stop spawning past this.
const MAX_PARTICLES: usize = 600;
/// How long a weapon swing takes, and a hurt shake lasts, in seconds.
const SWING_TIME: f32 = 0.3;
const SHAKE_TIME: f32 = 0.35;
const SHAKE_SIZE: f32 = 0.08;
/// How far from the player ambient particles appear, in meters.
const AMBIENT_REACH: f32 = 18.0;

// ---------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------

/// The colors particles come in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Spark {
	Gold,
	Blood,
	Soul,
	Ember,
	Dust,
	White,
}

impl Spark {
	const ALL: [Spark; 6] = [Spark::Gold, Spark::Blood, Spark::Soul, Spark::Ember, Spark::Dust, Spark::White];

	fn color(self) -> Color {
		match self {
			Spark::Gold => Color::srgb(1.0, 0.8, 0.3),
			Spark::Blood => Color::srgb(0.85, 0.12, 0.08),
			Spark::Soul => Color::srgb(0.6, 0.35, 1.0),
			Spark::Ember => Color::srgb(1.0, 0.5, 0.15),
			Spark::Dust => Color::srgb(0.35, 0.32, 0.28),
			Spark::White => Color::srgb(0.95, 0.95, 1.0),
		}
	}
}

/// A tiny glowing cube that flies, falls (or rises), shrinks, and vanishes.
#[derive(Component)]
pub(super) struct Particle {
	vel: Vec3,
	age: f32,
	life: f32,
	/// Downward pull in m/s²; negative floats it up
	gravity: f32,
	size: f32,
}

/// The mesh and one material per color all particles are drawn with.
#[derive(Resource)]
pub(super) struct EffectAssets {
	cube: Handle<Mesh>,
	materials: Vec<Handle<StandardMaterial>>,
}

impl EffectAssets {
	fn material(&self, spark: Spark) -> Handle<StandardMaterial> {
		self.materials[Spark::ALL.iter().position(|&s| s == spark).unwrap_or(0)].clone()
	}
}

/// Spawns `count` particles flying out from `at` at up to `speed` m/s.
pub(super) fn burst(commands: &mut Commands, assets: &EffectAssets, at: Vec3, spark: Spark, count: usize, speed: f32, gravity: f32, life: f32) {
	let mut rng = rand::rng();
	for _ in 0..count {
		let dir = Vec3::new(rng.random_range(-1.0..1.0), rng.random_range(-0.2..1.0), rng.random_range(-1.0..1.0)).normalize_or_zero();
		let vel = dir * speed * rng.random_range(0.4..1.0);
		let size = rng.random_range(0.04..0.09);
		spawn_particle(commands, assets, at, spark, vel, gravity, life * rng.random_range(0.6..1.0), size);
	}
}

#[allow(clippy::too_many_arguments)]
fn spawn_particle(commands: &mut Commands, assets: &EffectAssets, at: Vec3, spark: Spark, vel: Vec3, gravity: f32, life: f32, size: f32) {
	commands.spawn((
		Mesh3d(assets.cube.clone()),
		MeshMaterial3d(assets.material(spark)),
		Transform::from_translation(at).with_scale(Vec3::splat(size)),
		Particle { vel, age: 0.0, life, gravity, size },
	));
}

/// Moves, pulls, and shrinks every particle; removes the ones that are done.
pub(super) fn update_particles(mut commands: Commands, time: Res<Time>, mut particles: Query<(Entity, &mut Particle, &mut Transform)>) {
	let dt = time.delta_secs();
	for (entity, mut p, mut transform) in &mut particles {
		p.age += dt;
		if p.age >= p.life {
			commands.entity(entity).despawn();
			continue;
		}
		p.vel.y -= p.gravity * dt;
		p.vel *= 1.0 - (1.5 * dt).min(0.5);
		transform.translation += p.vel * dt;
		transform.scale = Vec3::splat(p.size * (1.0 - p.age / p.life));
	}
}

/// Ambient particles: embers drifting up from lights, dust hanging in the
/// air around the player, and a shimmer of motes around monsters.
pub(super) fn ambient_particles(
	mut commands: Commands,
	game: Res<Game>,
	time: Res<Time>,
	assets: Res<EffectAssets>,
	particles: Query<(), With<Particle>>,
	enemies: Query<&EnemyAnim>,
	mut timers: Local<[f32; 3]>,
) {
	if game.screen != super::menu::Screen::Playing || particles.iter().count() > MAX_PARTICLES {
		return;
	}
	let world = &game.levels[game.depth];
	let feet = Vec3::from_array(world.player.pos.to_array());
	let dt = time.delta_secs();
	let mut rng = rand::rng();
	for t in timers.iter_mut() {
		*t += dt;
	}

	// Embers
	while timers[0] > 0.06 {
		timers[0] -= 0.06;
		let near: Vec<Vec3> = world
			.lights
			.iter()
			.map(|l| Vec3::from_array(l.to_array()))
			.filter(|l| l.distance(feet) < AMBIENT_REACH)
			.collect();
		if let Some(&light) = near.get(rng.random_range(0..near.len().max(1))) {
			let at = light + Vec3::new(rng.random_range(-0.15..0.15), -0.2, rng.random_range(-0.15..0.15));
			let vel = Vec3::new(rng.random_range(-0.2..0.2), rng.random_range(0.5..1.1), rng.random_range(-0.2..0.2));
			spawn_particle(&mut commands, &assets, at, Spark::Ember, vel, -0.2, rng.random_range(0.8..1.6), 0.03);
		}
	}
	// Dust
	while timers[1] > 0.2 {
		timers[1] -= 0.2;
		let at = feet + Vec3::new(rng.random_range(-4.0..4.0), rng.random_range(0.4..3.2), rng.random_range(-4.0..4.0));
		let vel = Vec3::new(rng.random_range(-0.05..0.05), rng.random_range(-0.03..0.03), rng.random_range(-0.05..0.05));
		spawn_particle(&mut commands, &assets, at, Spark::Dust, vel, 0.0, 4.0, 0.025);
	}
	// Monster shimmer
	while timers[2] > 0.25 {
		timers[2] -= 0.25;
		for anim in &enemies {
			if anim.pos.distance(feet) > 14.0 {
				continue;
			}
			let at = anim.pos + Vec3::new(rng.random_range(-0.6..0.6), rng.random_range(0.3..1.6), rng.random_range(-0.6..0.6));
			let vel = Vec3::new(0.0, rng.random_range(0.2..0.5), 0.0);
			spawn_particle(&mut commands, &assets, at, Spark::Soul, vel, -0.1, 0.9, 0.035);
		}
	}
}

// ---------------------------------------------------------------------------
// Monsters coming alive
// ---------------------------------------------------------------------------

/// How a monster's sprite moves: where it's shown (sliding toward its tile
/// rather than jumping), and when it was last hit (it flinches).
#[derive(Component)]
pub(super) struct EnemyAnim {
	pub(super) id: u32,
	pub(super) pos: Vec3,
	pub(super) hurt: f32,
}

/// The sprite's transform for monster `anim` standing at `target`: it slides
/// there, breathes, sways, hops while moving, and squashes and shakes for a
/// moment after a hit. `base` is its plain camera-facing transform at `pos`.
pub(super) fn animate_enemy(anim: &mut EnemyAnim, target: Vec3, base: impl Fn(Vec3) -> Transform, t: f32, dt: f32) -> Transform {
	anim.pos = anim.pos.lerp(target, 1.0 - (-12.0 * dt).exp());
	let phase = t * 3.0 + anim.id as f32 * 1.7;
	let breathe = 1.0 + 0.05 * phase.sin();
	let hop = anim.pos.distance(target).min(1.0) * 0.6;
	let since_hit = t - anim.hurt;
	let (wide, tall, shake) = if since_hit < 0.3 {
		let k = 1.0 - since_hit / 0.3;
		(1.0 + 0.3 * k, 1.0 - 0.25 * k, (t * 60.0).sin() * 0.08 * k)
	} else {
		(1.0, 1.0, 0.0)
	};
	let mut tr = base(anim.pos + Vec3::Y * hop);
	let size = tr.scale.y;
	tr.scale = Vec3::new(tr.scale.x * wide * (2.0 - breathe), size * tall * breathe, 1.0);
	// Keep the feet on the floor as it stretches
	tr.translation.y += (tr.scale.y - size) / 2.0;
	tr.translation += tr.rotation * Vec3::X * shake;
	tr.rotation *= Quat::from_rotation_z(0.05 * (phase * 0.7).sin());
	tr
}

// ---------------------------------------------------------------------------
// The weapon in hand
// ---------------------------------------------------------------------------

/// The weapon held in view: the sword (`sword`) or the knife.
#[derive(Component)]
pub(super) struct Weapon {
	sword: bool,
}

/// When the current swing and hurt-shake started, in seconds since startup.
#[derive(Resource)]
pub(super) struct Motion {
	swing: f32,
	shake: f32,
}

/// How each weapon rests in view: off to the lower right, tilted in.
fn rest(sword: bool) -> Transform {
	if sword {
		// The sword model hangs blade down from its hilt: flip it up, lean it forward and in
		Transform::from_xyz(0.34, -0.36, -0.6)
			.with_rotation(Quat::from_rotation_z(0.35) * Quat::from_rotation_x(-0.9) * Quat::from_rotation_z(PI))
			.with_scale(Vec3::splat(0.75))
	} else {
		// The knife's blade runs along +X: point it forward, tipped up
		Transform::from_xyz(0.3, -0.3, -0.5)
			.with_rotation(Quat::from_rotation_x(0.35) * Quat::from_rotation_y(FRAC_PI_2))
			.with_scale(Vec3::splat(1.6))
	}
}

/// Startup: the particle assets, and the two weapons, held by the camera.
pub(super) fn setup(
	mut commands: Commands,
	mut meshes: ResMut<Assets<Mesh>>,
	mut materials: ResMut<Assets<StandardMaterial>>,
	mut images: ResMut<Assets<Image>>,
	cameras: Query<Entity, With<WorldCamera>>,
) {
	let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
	let spark_materials = Spark::ALL
		.iter()
		.map(|s| {
			let color = s.color();
			materials.add(StandardMaterial {
				base_color: color,
				emissive: color.to_linear() * if *s == Spark::Dust { 0.5 } else { 3.0 },
				unlit: true,
				alpha_mode: AlphaMode::Add,
				..default()
			})
		})
		.collect();
	commands.insert_resource(EffectAssets { cube, materials: spark_materials });
	commands.insert_resource(Motion { swing: -10.0, shake: -10.0 });

	let Ok(camera) = cameras.single() else { return };
	// Steel-grey: the models' colors come out white without the kit's texture
	let steel = [(true, "Objects/Sword.glb", glam::Vec3::new(0.75, 0.75, 0.8)), (false, "Objects/Knife.glb", glam::Vec3::new(0.7, 0.7, 0.72))];
	for (sword, model, tint) in steel {
		let Ok(mut mesh) = ModelMesh::load(model) else { continue };
		for v in &mut mesh.vertices {
			v.color *= tint;
		}
		let parts = model_parts(&mesh, &mut meshes, &mut materials, &mut images);
		let root = commands.spawn((Weapon { sword }, rest(sword), Visibility::Hidden)).id();
		for (part, material) in parts {
			let child = commands.spawn((Mesh3d(part), MeshMaterial3d(material), Transform::default())).id();
			commands.entity(root).add_child(child);
		}
		commands.entity(camera).add_child(root);
	}
}

/// Shows the weapon the hero has (the sword once equipped), swinging it
/// through an arc on attacks and bobbing it while walking. It's put away in
/// the main menu and the Generator, and while the level editor is open.
pub(super) fn animate_weapon(
	game: Res<Game>,
	editor: Option<Res<Editor>>,
	time: Res<Time>,
	motion: Res<Motion>,
	mut weapons: Query<(&Weapon, &mut Transform, &mut Visibility)>,
	mut last_feet: Local<Vec3>,
) {
	let world = &game.levels[game.depth];
	let editing = editor.is_some_and(|e| e.active);
	let playing = !editing && !matches!(game.screen, super::menu::Screen::MainMenu { .. } | super::menu::Screen::Generator { .. });
	let t = time.elapsed_secs();
	let feet = Vec3::from_array(world.player.pos.to_array());
	let moving = ((feet - *last_feet).length() / time.delta_secs().max(1e-4)).min(8.0) / 8.0;
	*last_feet = feet;
	let p = ((t - motion.swing) / SWING_TIME).clamp(0.0, 1.0);
	let swinging = p < 1.0;
	// Ease in and out across the arc
	let e = p * p * (3.0 - 2.0 * p);
	let arc = (PI * p).sin();
	for (weapon, mut transform, mut visibility) in &mut weapons {
		let shown = playing && weapon.sword == world.hero.sword;
		*visibility = if shown { Visibility::Inherited } else { Visibility::Hidden };
		let mut tr = rest(weapon.sword);
		if swinging {
			// From the right, across and down to the left
			let sweep = Quat::from_rotation_y(0.8 - 1.8 * e) * Quat::from_rotation_x(-1.2 * arc);
			tr.rotation = sweep * tr.rotation;
			tr.translation += Vec3::new(0.15 - 0.5 * e, 0.1 * arc, -0.15 * arc);
		} else {
			// A gentle bob while walking, and a slow sway at rest
			tr.translation += Vec3::new(0.0, (t * 10.0).sin() * 0.02 * moving + (t * 1.5).sin() * 0.004, 0.0);
		}
		*transform = tr;
	}
}

// ---------------------------------------------------------------------------
// Turning the world's events into effects
// ---------------------------------------------------------------------------

/// Empties the world's events: swings swing the weapon (with a streak),
/// hits spray and make the monster flinch, deaths burst, chests shower gold,
/// and getting hurt shakes the view.
pub(super) fn play_events(
	mut commands: Commands,
	mut game: ResMut<Game>,
	time: Res<Time>,
	assets: Res<EffectAssets>,
	mut motion: ResMut<Motion>,
	mut enemies: Query<&mut EnemyAnim>,
	cameras: Query<&Transform, With<WorldCamera>>,
) {
	let depth = game.depth;
	let events: Vec<Event> = game.levels[depth].events.drain(..).collect();
	let t = time.elapsed_secs();
	let v = |p: glam::Vec3| Vec3::from_array(p.to_array());
	for event in events {
		match event {
			Event::Swing => {
				motion.swing = t;
				// A faint streak through the air in front
				if let Ok(cam) = cameras.single() {
					for k in 0..8 {
						let a = 0.7 - 1.4 * k as f32 / 7.0;
						let at = cam.translation + cam.rotation * Vec3::new(a.sin() * 0.9, -0.15, -a.cos() * 0.9);
						spawn_particle(&mut commands, &assets, at, Spark::White, Vec3::ZERO, 0.0, 0.18, 0.03);
					}
				}
			}
			Event::Hit { id, at } => {
				burst(&mut commands, &assets, v(at), Spark::Blood, 14, 3.0, 6.0, 0.6);
				for mut anim in &mut enemies {
					if anim.id == id {
						anim.hurt = t;
					}
				}
			}
			Event::Kill { at } => {
				burst(&mut commands, &assets, v(at), Spark::Soul, 30, 2.5, -0.5, 1.2);
				burst(&mut commands, &assets, v(at), Spark::Blood, 12, 3.5, 6.0, 0.7);
			}
			Event::Hurt => motion.shake = t,
			Event::ChestOpened { at } => {
				let lid = v(at) + Vec3::Y * 0.8;
				burst(&mut commands, &assets, lid, Spark::Gold, 40, 3.5, 6.0, 1.0);
				burst(&mut commands, &assets, lid, Spark::White, 10, 2.0, 2.0, 0.6);
			}
		}
	}
}

/// How far the view is knocked off its spot right now by a hurt-shake.
pub(super) fn shake_offset(motion: &Motion, t: f32) -> Vec3 {
	let since = t - motion.shake;
	if !(0.0..SHAKE_TIME).contains(&since) {
		return Vec3::ZERO;
	}
	let k = SHAKE_SIZE * (1.0 - since / SHAKE_TIME);
	Vec3::new((t * 53.0).sin() * k, (t * 41.0).cos() * k, 0.0)
}
