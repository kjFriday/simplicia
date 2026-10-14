#[derive(Debug)]
pub(crate) struct History {
	#[allow(dead_code)]
	pub(crate) string: String,
}

impl History {
	pub fn new() -> Self {
		History {
			string: String::from("History logic"),
		}
	}
}