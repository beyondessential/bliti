//! The one configuration a device holds, and how it is faring (CFG).
//!
//! Held as the raw document a client proposed rather than as a parsed one, so members a newer client
//! added survive and are echoed back, as `bliti_core::channel::config` explains. The file is replaced
//! atomically, so a power cut leaves either what was held before or what was being recorded.

use std::{
	fs,
	io::{self, Write},
	path::{Path, PathBuf},
};

use bliti_core::channel::config::{Failure, Outcome};
use serde_json::{Map, Value as Json};

/// Where the held configuration lives on a device.
pub const DEFAULT_PATH: &str = "/var/lib/bliti/network.json";

/// The default path, for the daemon's command line.
pub fn default_path() -> PathBuf {
	PathBuf::from(DEFAULT_PATH)
}

/// What the file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
	/// The configuration the device holds.
	pub document: Map<String, Json>,
	/// How its verification went, or `verifying` where it has not ended.
	pub outcome: Outcome,
	/// Why it failed, where it did.
	pub failure: Option<Failure>,
	/// The configuration it replaced, kept only while verifying, so a restart that interrupts the
	/// verification judges it against the same one.
	pub replaced: Option<Map<String, Json>>,
}

impl Stored {
	fn to_json(&self) -> Json {
		let mut file = Map::new();
		file.insert("document".to_owned(), Json::Object(self.document.clone()));
		file.insert("outcome".to_owned(), self.outcome.as_str().into());
		if let Some(failure) = &self.failure {
			file.insert("failure".to_owned(), Json::Object(failure.to_map()));
		}
		if let Some(replaced) = &self.replaced {
			file.insert("replaced".to_owned(), Json::Object(replaced.clone()));
		}
		Json::Object(file)
	}

	fn from_json(file: Json) -> Result<Self, String> {
		let Json::Object(mut file) = file else {
			return Err("not an object".to_owned());
		};
		let mut object = |member: &str| match file.remove(member) {
			Some(Json::Object(object)) => Ok(Some(object)),
			None => Ok(None),
			Some(_) => Err(format!("`{member}` is not an object")),
		};
		let document = object("document")?.ok_or("no `document`")?;
		let failure = object("failure")?
			.map(|failure| Failure::read(&failure))
			.transpose()?;
		let replaced = object("replaced")?;
		let outcome = match file.get("outcome") {
			Some(Json::String(outcome)) => Outcome::read(outcome)?,
			_ => return Err("no `outcome`".to_owned()),
		};
		Ok(Self {
			document,
			outcome,
			failure,
			replaced,
		})
	}
}

/// The file holding the held configuration.
#[derive(Debug, Clone)]
pub struct Store {
	path: PathBuf,
}

impl Store {
	/// A store at `path`. Nothing is read or written until asked.
	pub fn new(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}

	/// What was last recorded, or `None` where nothing has ever been recorded.
	pub fn load(&self) -> Result<Option<Stored>, StoreError> {
		let bytes = match fs::read(&self.path) {
			Ok(bytes) => bytes,
			Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(err) => return Err(StoreError::Io(self.path.clone(), err)),
		};
		let file = serde_json::from_slice(&bytes)
			.map_err(|err| StoreError::Json(self.path.clone(), err))?;
		Stored::from_json(file)
			.map(Some)
			.map_err(|reason| StoreError::NotAConfiguration(self.path.clone(), reason))
	}

	/// Replace what is recorded with `stored`.
	///
	/// Through a temporary file beside it, synced, renamed over it, and the directory synced, so the
	/// rename itself survives a power cut.
	pub async fn record(&self, stored: &Stored) -> Result<(), StoreError> {
		let path = self.path.clone();
		let file = stored.to_json();
		tokio::task::spawn_blocking(move || {
			write_atomically(&path, &file).map_err(|err| StoreError::Io(path, err))
		})
		.await
		.map_err(|err| StoreError::Io(self.path.clone(), io::Error::other(err)))?
	}
}

fn write_atomically(path: &Path, file: &Json) -> io::Result<()> {
	let directory = match path.parent() {
		Some(parent) if !parent.as_os_str().is_empty() => parent,
		_ => Path::new("."),
	};
	fs::create_dir_all(directory)?;

	let mut temporary = path.as_os_str().to_owned();
	temporary.push(".new");
	let temporary = PathBuf::from(temporary);

	let body = serde_json::to_vec_pretty(file).map_err(io::Error::other)?;
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

	/// The file is JSON but not a held configuration.
	#[error("{0} does not hold a configuration: {1}")]
	NotAConfiguration(PathBuf, String),
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

	pub fn stored(document: Json) -> Stored {
		Stored {
			document: object(document),
			outcome: Outcome::Applied,
			failure: None,
			replaced: None,
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
		let held = stored(json!({"attachments": [], "x-from-a-newer-client": {"kept": true}}));
		store.record(&held).await.unwrap();
		assert_eq!(store.load().unwrap(), Some(held));

		let entries: Vec<_> = fs::read_dir(&scratch.0)
			.unwrap()
			.map(|entry| entry.unwrap().file_name())
			.collect();
		assert_eq!(entries, ["network.json"]);
	}

	#[tokio::test]
	async fn each_shape_of_a_record_is_read_back() {
		let scratch = Scratch::new();
		let store = scratch.store();
		let verifying = Stored {
			outcome: Outcome::Verifying,
			replaced: Some(object(json!({"attachments": [], "n": 0}))),
			..stored(json!({"attachments": [], "n": 1}))
		};
		let failed = Stored {
			outcome: Outcome::Failed,
			failure: Some(Failure {
				at: "$['attachments'][0]['passphrase']".to_owned(),
				reason: "the access point refused the key".to_owned(),
				reached: Some("association".to_owned()),
			}),
			..stored(json!({"attachments": [], "n": 1}))
		};
		let failed_without_a_stage = Stored {
			failure: Some(Failure {
				at: "$".to_owned(),
				reason: "hostapd would not start".to_owned(),
				reached: None,
			}),
			..failed.clone()
		};
		for held in [
			verifying,
			failed,
			failed_without_a_stage,
			stored(json!({"attachments": []})),
		] {
			store.record(&held).await.unwrap();
			assert_eq!(store.load().unwrap(), Some(held));
		}
	}

	#[tokio::test]
	async fn a_settled_record_names_neither_failure_nor_replaced_unless_it_has_them() {
		let scratch = Scratch::new();
		let store = scratch.store();
		store
			.record(&stored(json!({"attachments": []})))
			.await
			.unwrap();
		let file: Json =
			serde_json::from_slice(&fs::read(scratch.0.join("network.json")).unwrap()).unwrap();
		assert_eq!(
			file,
			json!({"document": {"attachments": []}, "outcome": "applied"})
		);
	}

	#[tokio::test]
	async fn a_record_replaces_the_one_before_it() {
		let scratch = Scratch::new();
		let store = scratch.store();
		store
			.record(&stored(json!({"attachments": [], "n": 1})))
			.await
			.unwrap();
		store
			.record(&stored(json!({"attachments": [], "n": 2})))
			.await
			.unwrap();
		assert_eq!(
			store.load().unwrap(),
			Some(stored(json!({"attachments": [], "n": 2})))
		);
	}

	#[cfg(unix)]
	#[tokio::test]
	async fn only_the_daemon_can_read_the_record() {
		use std::os::unix::fs::PermissionsExt;
		let scratch = Scratch::new();
		let store = scratch.store();
		store
			.record(&stored(json!({"attachments": []})))
			.await
			.unwrap();
		let mode = fs::metadata(scratch.0.join("network.json"))
			.unwrap()
			.permissions()
			.mode();
		assert_eq!(mode & 0o777, 0o600);
	}

	#[test]
	fn a_file_that_is_not_a_held_configuration_is_an_error() {
		let scratch = Scratch::new();
		fs::create_dir_all(&scratch.0).unwrap();
		let write = |body: &[u8]| fs::write(scratch.0.join("network.json"), body).unwrap();
		write(b"[1, 2]");
		assert!(matches!(
			scratch.store().load(),
			Err(StoreError::NotAConfiguration(..))
		));
		// What a build before the held shape wrote: the bare document.
		write(br#"{"attachments": []}"#);
		assert!(matches!(
			scratch.store().load(),
			Err(StoreError::NotAConfiguration(..))
		));
		write(br#"{"document": {}, "outcome": "settled"}"#);
		assert!(matches!(
			scratch.store().load(),
			Err(StoreError::NotAConfiguration(..))
		));
		write(b"{not json");
		assert!(matches!(scratch.store().load(), Err(StoreError::Json(..))));
	}
}
