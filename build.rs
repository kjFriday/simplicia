use std::{env, fs, path::{Path, PathBuf}};

fn main() {
	let root = Path::new("models");
	println!("cargo:rerun-if-changed=models");

	let mut files = Vec::new();
	// Ensure the collection of model files succeeds
	collect(root, root, &mut files);
	files.sort();

	let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
	let mut code =String::from("pub static MODEL_FILES: &[(&str, &[u8])] = &[\n");
	for (name, path) in &files {
		println!("cargo:rerun-if-changed={}", path.display());
		// include_bytes! resolves relative paths against OUT_DIR, so pass an absolute one
		let abs = Path::new(&manifest_dir).join(path);
		code.push_str(&format!("\t({:?}, include_bytes!({:?})),\n", name, abs));
	}
	code.push_str("];\n");

	let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("models.rs");
	fs::write(out, code).unwrap();
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
	for entry in fs::read_dir(dir).unwrap() {
		let path = entry.unwrap().path();
		if path.is_dir() {
			collect(root, &path, files);
		} else if path.is_file() {
			let name = path
				.strip_prefix(root)
				.unwrap()
				.to_string_lossy()
				.replace('\\', "/");
			files.push((name, path));
		}
	}
}
