#[derive(Debug)]
pub(crate) struct Quest {
	pub(crate) string: String,
}	


impl Quest {
	pub fn new() -> Self {
		Quest {
			string: String::from("Quest logic"),
		}
	}
}