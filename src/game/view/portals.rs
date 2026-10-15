// Portals: the staircases glow like magic gateways. Each gets a column of
// light from the pit up through the ceiling (which also hides the hole the
// stairs leave in it), a colored light, and sparks that spiral up through it.
// scene.rs spawns one on each staircase when a level is shown; `animate`
// moves the sparks every frame.
use bevy::prelude::*;

/// The way down glows violet; the way up, cyan.
const DOWN_COLOR: Color = Color::srgb(0.7, 0.3, 1.0);
const UP_COLOR: Color = Color::srgb(0.3, 0.9, 1.0);

/// The column: its radius, and how far it reaches below and above the floor.
const COLUMN_RADIUS: f32 = 1.6;
const COLUMN_BOTTOM: f32 = -5.0;
const COLUMN_TOP: f32 = 5.0;
/// Sparks per portal, how fast they rise (m/s), and how fast they circle (rad/s).
const SPARKS: usize = 40;
const SPARK_RISE: f32 = 1.2;
const SPARK_SPIN: f32 = 1.4;
const SPARK_SIZE: f32 = 0.06;
/// The portal's light, in lumens, and how far it reaches.
const PORTAL_LUMENS: f32 = 120_000.0;
const PORTAL_RANGE: f32 = 12.0;

/// One spark: where on its spiral it is. `phase` (0 to 1) is its height in
/// the loop; `angle` and `radius` place it around the column's axis.
#[derive(Component)]
pub(super) struct Spark {
	center: Vec3,
	phase: f32,
	angle: f32,
	radius: f32,
}

/// Which staircase a portal stands on.
#[derive(Clone, Copy)]
pub(super) enum PortalKind {
	Down,
	Up,
}

/// Spawns a portal at `center` (the middle of the stair tile, at floor
/// height) and returns its entities so the level can remove them later.
pub(super) fn spawn(
	commands: &mut Commands,
	meshes: &mut Assets<Mesh>,
	materials: &mut Assets<StandardMaterial>,
	center: Vec3,
	kind: PortalKind,
) -> Vec<Entity> {
	let color = match kind {
		PortalKind::Down => DOWN_COLOR,
		PortalKind::Up => UP_COLOR,
	};
	let linear = color.to_linear();
	let mut entities = Vec::new();

	// The column of light: see-through, glowing, unlit so it reads in the dark
	let height = COLUMN_TOP - COLUMN_BOTTOM;
	let column = materials.add(StandardMaterial {
		base_color: color.with_alpha(0.25),
		emissive: linear * 2.0,
		alpha_mode: AlphaMode::Add,
		unlit: true,
		cull_mode: None,
		double_sided: true,
		..default()
	});
	entities.push(
		commands
			.spawn((
				Mesh3d(meshes.add(Cylinder::new(COLUMN_RADIUS, height))),
				MeshMaterial3d(column),
				Transform::from_translation(center + Vec3::Y * (COLUMN_BOTTOM + height / 2.0)),
			))
			.id(),
	);

	entities.push(
		commands
			.spawn((
				PointLight { color, intensity: PORTAL_LUMENS, range: PORTAL_RANGE, ..default() },
				Transform::from_translation(center + Vec3::Y * 1.5),
			))
			.id(),
	);

	// Sparks, spread evenly through the loop so the column is never empty
	let spark_mesh = meshes.add(Sphere::new(SPARK_SIZE));
	let spark_material = materials.add(StandardMaterial {
		base_color: Color::WHITE,
		emissive: linear * 8.0,
		unlit: true,
		..default()
	});
	for i in 0..SPARKS {
		let t = i as f32 / SPARKS as f32;
		let spark = Spark {
			center,
			phase: t,
			angle: t * 37.0, // scattered around the circle
			radius: COLUMN_RADIUS * (0.3 + 0.6 * ((t * 13.0).fract())),
		};
		entities.push(
			commands
				.spawn((Mesh3d(spark_mesh.clone()), MeshMaterial3d(spark_material.clone()), spark, Transform::default()))
				.id(),
		);
	}
	entities
}

/// Moves every spark up its spiral, looping back to the bottom of the
/// column when it reaches the top.
pub(super) fn animate(time: Res<Time>, mut sparks: Query<(&mut Spark, &mut Transform)>) {
	let dt = time.delta_secs();
	let height = COLUMN_TOP - COLUMN_BOTTOM;
	for (mut spark, mut transform) in &mut sparks {
		spark.phase = (spark.phase + SPARK_RISE * dt / height).fract();
		spark.angle += SPARK_SPIN * dt;
		let (s, c) = spark.angle.sin_cos();
		let y = COLUMN_BOTTOM + spark.phase * height;
		transform.translation = spark.center + Vec3::new(c * spark.radius, y, s * spark.radius);
		// Fade in at the bottom and out at the top by shrinking
		let fade = (spark.phase * std::f32::consts::PI).sin();
		transform.scale = Vec3::splat(fade.max(0.05));
	}
}
