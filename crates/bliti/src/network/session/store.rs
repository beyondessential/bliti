//! The one recorded configuration a device holds (CFG).
//!
//! Held as the raw document a client confirmed rather than as a parsed one, so members a newer client
//! added survive and are echoed back, as `bliti_core::channel::config` explains. It is replaced only
//! on `confirm`, and atomically, so a power cut leaves either the old configuration or the new one.

use std::{
	fs,
	io::{self, Write},
	path::{Path, PathBuf},
};

use serde_json::{Map, Value as Json};

/// Where the recorded configuration lives on a device.
pub const DEFAULT_PATH: &str = "/var/lib/bliti/network.json";

/// The default path, for the daemon's command line.
pub fn default_path() -> PathBuf {
	PathBuf::from(DEFAULT_PATH)
}

/// The file holding the recorded configuration.
#[derive(Debug, Clone)]
pub struct Store {
	path: PathBuf,
}

impl Store {
	/// A store at `path`. Nothing is read or written until asked.
	pub fn new(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}

	/// The recorded configuration, or `None` where nothing has ever been recorded.
	pub fn load(&self) -> Result<Option<Map<String, Json>>, StoreError> {
		let bytes = match fs::read(&self.path) {
			Ok(bytes) => bytes,
			Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(err) => return Err(StoreError::Io(self.path.clone(), err)),
		};
		match serde_json::from_slice(&bytes) {
			Ok(Json::Object(document)) => Ok(Some(document)),
			Ok(_) => Err(StoreError::NotADocument(self.path.clone())),
			Err(err) => Err(StoreError::Json(self.path.clone(), err)),
		}
	}

	/// Replace the recorded configuration with `document`.
	///
	/// Through a temporary file beside it, synced, renamed over it, and the directory synced, so the
	/// rename itself survives a power cut.
	pub async fn record(&self, document: Map<String, Json>) -> Result<(), StoreError> {
		let path = self.path.clone();
		tokio::task::spawn_blocking(move || {
			write_atomically(&path, &document).map_err(|err| StoreError::Io(path, err))
		})
		.await
		.map_err(|err| StoreError::Io(self.path.clone(), io::Error::other(err)))?
	}
}

fn write_atomically(path: &Path, document: &Map<String, Json>) -> io::Result<()> {
	let directory = match path.parent() {
		Some(parent) if !parent.as_os_str().is_empty() => parent,
		_ => Path::new("."),
	};
	fs::create_dir_all(directory)?;

	let mut temporary = path.as_os_str().to_owned();
	temporary.push(".new");
	let temporary = PathBuf::from(temporary);

	let body = serde_json::to_vec_pretty(document).map_err(io::Error::other)?;
	// A temporary left by a write that was cut short keeps the mode it was made with, so it goes.
	match fs::remove_file(&temporary) {
		Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
		_ => {}
	}
	let mut options = fs::OpenOptions::new();
	options.write(true).create(true).truncate(true);
	// The document carries every key the device joins with (NET), so only the daemon reads it.
	#[cfg(unix)]
	{
		use std::os::unix::fs::OpenOptionsExt;
		options.mode(0o600);
	}
	let mut file = options.open(&temporary)?;
	file.write_all(&body)?;
	file.sync_all()?;
	drop(file);

	fs::rename(&temporary, path)?;
	fs::File::open(directory)?.sync_all()
}

/// Why the recorded configuration could not be read or replaced.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
	/// The file could not be read or written.
	#[error("{0}: {1}")]
	Io(PathBuf, #[source] io::Error),

	/// The file is not JSON.
	#[error("{0} is not JSON: {1}")]
	Json(PathBuf, #[source] serde_json::Error),

	/// The file is JSON but not an object, so not a document.
	#[error("{0} does not hold a configuration document")]
	NotADocument(PathBuf),
}

#[cfg(test)]
pub(super) mod tests {
	use std::sync::atomic::{AtomicUsize, Ordering};

	use serde_json::json;

	use super::*;

	/// A directory of its own for one test, removed when it ends.
	pub struct Scratch(pub PathBuf);

	impl Scratch {
		pub fn new() -> Self {
			static NEXT: AtomicUsize = AtomicUsize::new(0);
			let n = NEXT.fetch_add(1, Ordering::Relaxed);
			let dir =
				std::env::temp_dir().join(format!("bliti-network-{}-{n}", std::process::id()));
			let _ = fs::remove_dir_all(&dir);
			Self(dir)
		}

		pub fn store(&self) -> Store {
			Store::new(self.0.join("network.json"))
		}
	}

	impl Drop for Scratch {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	pub fn object(value: Json) -> Map<String, Json> {
		match value {
			Json::Object(map) => map,
			_ => unreachable!(),
		}
	}

	#[test]
	fn nothing_recorded_loads_as_none() {
		let scratch = Scratch::new();
		assert!(scratch.store().load().unwrap().is_none());
	}

	#[tokio::test]
	async fn a_record_is_read_back_raw_and_leaves_no_temporary_behind() {
		let scratch = Scratch::new();
		let store = scratch.store();
		let document = object(json!({"attachments": [], "x-from-a-newer-client": {"kept": true}}));
		store.record(document.clone()).await.unwrap();
		assert_eq!(store.load().unwrap(), Some(document));

		let entries: Vec<_> = fs::read_dir(&scratch.0)
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert_eq!(entries, ["network.json"]);
	}

	#[tokio::test]
	async fn a_record_replaces_the_one_before_it() {
		let scratch = Scratch::new();
		let store = scratch.store();
		store
			.record(object(json!({"attachments": [], "n": 1})))
			.await
			.unwrap();
		store
			.record(object(json!({"attachments": [], "n": 2})))
			.await
			.unwrap();
		assert_eq!(
			store.load().unwrap(),
			Some(object(json!({"attachments": [], "n": 2})))
		);
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn only_the_daemon_can_read_the_record() {
		use std::os::unix::fs::PermissionsExt;
		let scratch = Scratch::new();
		let store = scratch.store();
		store
			.record(object(json!({"attachments": []})))
			.await
			.unwrap();
		let mode = fs::metadata(scratch.0.join("network.json"))
			.unwrap()
			.permissions()
			.mode();
		assert_eq!(mode & 0o777, 0o600);
	}

	#[test]
	fn a_file_that_is_not_a_document_is_an_error() {
		let scratch = Scratch::new();
		fs::create_dir_all(&scratch.0).unwrap();
		fs::write(scratch.0.join("network.json"), b"[1, 2]").unwrap();
		assert!(matches!(
			scratch.store().load(),
			Err(StoreError::NotADocument(_))
		));
		fs::write(scratch.0.join("network.json"), b"{not json").unwrap();
		assert!(matches!(scratch.store().load(), Err(StoreError::Json(..))));
	}
}
