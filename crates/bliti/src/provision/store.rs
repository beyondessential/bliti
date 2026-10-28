//! What a run keeps on the provisioning medium so it survives a reboot (RUN, "Surviving a reboot").

use std::path::Path;

/// The state of the runs of one board on one medium.
#[derive(Debug, Clone)]
pub struct Store {}

impl Store {
	/// The store of the board with platform serial `serial`, under `medium`.
	pub fn new(_medium: &Path, _serial: &[u8]) -> Self {
		Self {}
	}
}
