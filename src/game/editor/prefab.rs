// Prefab files: the plain text the editor saves and loads. edit.rs calls
// `save` and `load`; levels/cells.rs reads cell files with `parse`.
use std::fmt::Write as _;
use std::path::Path;

use bevy::prelude::*;

use super::Placed;

/// One line per model: `path x y z yaw-degrees`. Lines starting with # are comments.
pub(super) fn save(path: &Path, placed: &[Placed]) -> std::io::Result<()> {
	let mut text = String::from("# prefab: model x y z yaw_degrees (meters, Y up; tiles are 4 m)\n");
	for p in placed {
		let _ = writeln!(text, "{} {} {} {} {}", p.model.replace(' ', "%20"), p.pos.x, p.pos.y, p.pos.z, p.yaw);
	}
	if let Some(dir) = path.parent() {
		std::fs::create_dir_all(dir)?;
	}
	std::fs::write(path, text)
}

pub(super) fn load(path: &Path) -> Result<Vec<(String, Vec3, f32)>, String> {
	let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
	parse(&text)
}

pub(crate) fn parse(text: &str) -> Result<Vec<(String, Vec3, f32)>, String> {
	let mut out = Vec::new();
	for (n, line) in text.lines().enumerate() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let f: Vec<&str> = line.split_whitespace().collect();
		let bad = || format!("line {}: expected `model x y z yaw`", n + 1);
		if f.len() != 5 {
			return Err(bad());
		}
		let num = |s: &str| s.parse::<f32>().map_err(|_| bad());
		out.push((f[0].replace("%20", " "), Vec3::new(num(f[1])?, num(f[2])?, num(f[3])?), num(f[4])?));
	}
	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn prefab_text_round_trips() {
		let text = "# comment\n\
			Tiles/Dungeon_Hall_Center.glb 0 0 4 1\n\
			nature/plants/Green%20Bush.glb -2 0.5 0 3\n";
		let parsed = parse(text).unwrap();
		assert_eq!(
			parsed[0],
			("Tiles/Dungeon_Hall_Center.glb".to_string(), Vec3::new(0.0, 0.0, 4.0), 1.0)
		);
		assert_eq!(parsed[1].0, "nature/plants/Green Bush.glb");
		assert!(parse("just one field").is_err());
	}
}
