//! What a run keeps on the provisioning medium so it survives a reboot (RUN, "Surviving a reboot").
//!
//! One file per board, named by its platform serial, so a medium serving several boards in turn holds
//! each one's run apart and a device reads only its own. The answer to a prompt is never written here.

use std::{
	fs::{self, File},
	io::{self, Write as _},
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Where under the medium the runs are kept.
const DIRECTORY: &str = "runs";

/// The state of a run, as much of it as survives a reboot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stored {
	/// The steps chosen, by id.
	pub chosen: Vec<String>,
	/// The steps that have passed, by id.
	pub passed: Vec<String>,
	/// The step that failed and why, where the run has failed.
	pub failed: Option<StoredFailure>,
}

/// A step that failed, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredFailure {
	/// The step, by id.
	pub step: String,
	/// Why it failed.
	pub reason: String,
}

/// The state of the runs of one board on one medium.
#[derive(Debug, Clone)]
pub struct Store {
	path: PathBuf,
}

impl Store {
	/// The store of the board with platform serial `serial`, under `medium`.
	pub fn new(medium: &Path, serial: &[u8]) -> Self {
		Self {
			path: medium
				.join(DIRECTORY)
				.join(format!("{}.json", hex::encode(serial))),
		}
	}

	/// The run held for this board, or nothing where none is held or what is held cannot be read.
	///
	/// A file that cannot be read is treated as none: a run cannot be resumed from it, and refusing
	/// to start would leave the board unprovisionable.
	pub fn load(&self) -> io::Result<Option<Stored>> {
		let raw = match fs::read(&self.path) {
			Ok(raw) => raw,
			Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(err) => return Err(err),
		};
		match serde_json::from_slice(&raw) {
			Ok(stored) => Ok(Some(stored)),
			Err(err) => {
				tracing::warn!(%err, path = %self.path.display(), "the stored run is unreadable; ignoring it");
				Ok(None)
			}
		}
	}

	/// Keep `run` as this board's run, through a temporary file and a rename so a power cut leaves
	/// either the run as it was or as it is now.
	pub fn save(&self, run: &Stored) -> io::Result<()> {
		if let Some(parent) = self.path.parent() {
			fs::create_dir_all(parent)?;
		}
		let temporary = self.path.with_extension("json.new");
		let mut file = File::create(&temporary)?;
		file.write_all(&serde_json::to_vec_pretty(run).map_err(io::Error::other)?)?;
		file.sync_all()?;
		fs::rename(&temporary, &self.path)?;
		if let Some(parent) = self.path.parent() {
			File::open(parent)?.sync_all()?;
		}
		Ok(())
	}

	/// Discard this board's run.
	pub fn clear(&self) -> io::Result<()> {
		match fs::remove_file(&self.path) {
			Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
			_ => Ok(()),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn run() -> Stored {
		Stored {
			chosen: vec!["a".to_owned(), "b".to_owned()],
			passed: vec!["a".to_owned()],
			failed: Some(StoredFailure {
				step: "b".to_owned(),
				reason: "the stick was pulled".to_owned(),
			}),
		}
	}

	#[test]
	fn a_saved_run_is_loaded_back() {
		let medium = tempfile::tempdir().unwrap();
		let store = Store::new(medium.path(), &[0xf3, 0x75]);
		assert_eq!(store.load().unwrap(), None);
		store.save(&run()).unwrap();
		assert_eq!(store.load().unwrap(), Some(run()));
	}

	#[test]
	fn a_cleared_run_is_gone() {
		let medium = tempfile::tempdir().unwrap();
		let store = Store::new(medium.path(), &[1]);
		store.save(&run()).unwrap();
		store.clear().unwrap();
		assert_eq!(store.load().unwrap(), None);
		store.clear().unwrap();
	}

	#[test]
	fn each_board_reads_only_its_own_run() {
		let medium = tempfile::tempdir().unwrap();
		let mine = Store::new(medium.path(), &[1]);
		let theirs = Store::new(medium.path(), &[2]);
		mine.save(&run()).unwrap();
		assert_eq!(theirs.load().unwrap(), None);
		assert_eq!(mine.load().unwrap(), Some(run()));
	}

	#[test]
	fn a_file_that_is_not_a_run_is_no_run() {
		let medium = tempfile::tempdir().unwrap();
		let store = Store::new(medium.path(), &[3]);
		store.save(&run()).unwrap();
		fs::write(&store.path, b"not json").unwrap();
		assert_eq!(store.load().unwrap(), None);
	}

	#[test]
	fn saving_leaves_no_temporary_file() {
		let medium = tempfile::tempdir().unwrap();
		let store = Store::new(medium.path(), &[4]);
		store.save(&run()).unwrap();
		let names: Vec<_> = fs::read_dir(medium.path().join(DIRECTORY))
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert_eq!(names, [std::ffi::OsString::from("04.json")]);
	}
}
