pub mod mapgen;

#[derive(Debug)]
pub(crate) struct Dungeon {
	pub(crate) string: String,
}

impl Dungeon {
	pub fn new() -> Self {
		Dungeon {
			string: String::from("Dungeon logic"),
		}
	}
}