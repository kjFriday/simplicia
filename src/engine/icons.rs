// Icons from the Raven Fantasy Icons pack (Clockwork Raven Studios), built
// into the executable. The sheet is 16 icons across, 32 x 32 pixels each, and
// icon `n` (as numbered in assets/sprites/catalog.json) is cell `n - 1`.
// Icons are drawn in the world as sprites: flat, camera-facing quads.
use bevy::prelude::*;

use super::mesh::Texture;
use super::scene::to_bevy_image;

const SHEET_PNG: &[u8] = include_bytes!("../../assets/sprites/raven-32.png");
/// Icons across the sheet, and each icon's size in pixels.
const COLUMNS: u32 = 16;
pub(crate) const ICON_SIZE: u32 = 32;

/// The decoded icon sheet.
pub(crate) struct IconSheet {
	width: u32,
	rgba: Vec<u8>,
}

impl IconSheet {
	pub(crate) fn load() -> Result<Self, String> {
		let sheet = image::load_from_memory(SHEET_PNG).map_err(|e| format!("icon sheet: {e}"))?.to_rgba8();
		Ok(Self { width: sheet.width(), rgba: sheet.into_raw() })
	}

	/// How many icons the sheet holds.
	pub(crate) fn count(&self) -> u32 {
		let rows = self.rgba.len() as u32 / 4 / self.width / ICON_SIZE;
		rows * COLUMNS
	}

	/// Icon `n` (1-based, as in the catalog) as a 32 x 32 RGBA texture.
	/// Out-of-range numbers give a transparent icon.
	pub(crate) fn icon(&self, n: u32) -> Texture {
		let mut rgba = vec![0; (ICON_SIZE * ICON_SIZE * 4) as usize];
		if (1..=self.count()).contains(&n) {
			let cell = n - 1;
			let (x0, y0) = ((cell % COLUMNS) * ICON_SIZE, (cell / COLUMNS) * ICON_SIZE);
			let row_bytes = (ICON_SIZE * 4) as usize;
			for y in 0..ICON_SIZE {
				let from = (((y0 + y) * self.width + x0) * 4) as usize;
				let to = (y * ICON_SIZE * 4) as usize;
				rgba[to..to + row_bytes].copy_from_slice(&self.rgba[from..from + row_bytes]);
			}
		}
		Texture { width: ICON_SIZE, height: ICON_SIZE, rgba }
	}
}

/// What a sprite is drawn with: a unit quad facing +Z, and a material per icon.
/// Scale the quad's transform to size the sprite.
pub(crate) struct Sprites {
	quad: Handle<Mesh>,
	materials: Vec<(u32, Handle<StandardMaterial>)>,
}

impl Sprites {
	pub(crate) fn new(meshes: &mut Assets<Mesh>) -> Self {
		Self { quad: meshes.add(Rectangle::new(1.0, 1.0)), materials: Vec::new() }
	}

	/// The mesh and material for icon `n`, building the material the first time.
	pub(crate) fn get(
		&mut self,
		sheet: &IconSheet,
		n: u32,
		materials: &mut Assets<StandardMaterial>,
		images: &mut Assets<Image>,
	) -> (Mesh3d, MeshMaterial3d<StandardMaterial>) {
		let material = match self.materials.iter().find(|(icon, _)| *icon == n) {
			Some((_, m)) => m.clone(),
			None => {
				let image = images.add(to_bevy_image(&sheet.icon(n)));
				let m = materials.add(StandardMaterial {
					base_color_texture: Some(image),
					// Cut out the transparent background; no blending to sort
					alpha_mode: AlphaMode::Mask(0.5),
					perceptual_roughness: 1.0,
					cull_mode: None,
					double_sided: true,
					..default()
				});
				self.materials.push((n, m.clone()));
				m
			}
		};
		(Mesh3d(self.quad.clone()), MeshMaterial3d(material))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn icons_are_cut_from_the_right_cell() {
		let sheet = IconSheet::load().unwrap();
		assert_eq!(sheet.count(), 16 * 137);
		for n in [1, 123, 861, 1066, 1067, 1381, 1391, 2113] {
			let icon = sheet.icon(n);
			assert_eq!((icon.width, icon.height), (32, 32));
			assert!(icon.rgba.chunks(4).any(|p| p[3] > 128), "icon {n} is empty");
		}
		assert!(sheet.icon(0).rgba.iter().all(|&b| b == 0));
	}
}
