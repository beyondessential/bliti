//! The curve file, `/var/lib/bliti/battery-curve.json`: the curve document in force, and what the
//! device keeps beside it that is its own rather than the document's (CHG).
//!
//! Replaced atomically, so a power cut, which is exactly when a run's learning is saved, leaves
//! either the old file or the new one.

use std::{
	fs,
	io::{self, Write},
	path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Map, Value as Json};

use super::{Document, Invalid};

/// Where the curve file lives on a device, beside `network.json`.
pub const DEFAULT_PATH: &str = "/var/lib/bliti/battery-curve.json";

/// What the curve file holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
	pub document: Document,
	/// The gauge's state of charge, as a share, when a charge last finished: the figure the gauge
	/// gives for a full cell, which scales its readings on mains (CHG).
	pub gauge_full: Option<f64>,
}

impl Default for Stored {
	fn default() -> Self {
		Self {
			document: Document::shipped(),
			gauge_full: None,
		}
	}
}

/// The file on disk.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct File {
	document: Json,
	#[serde(default)]
	gauge_full: Option<f64>,
}

/// The curve file at a path. Nothing is read or written until asked.
#[derive(Debug, Clone)]
pub struct Store {
	path: PathBuf,
}

impl Store {
	pub fn new(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}

	#[expect(
		dead_code,
		reason = "named by the command line's messages, still to come (T2)"
	)]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// What the file holds, `None` where there is no file, or why it cannot be used.
	pub fn read(&self) -> Result<Option<Stored>, StoreError> {
		let bytes = match fs::read(&self.path) {
			Ok(bytes) => bytes,
			Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(err) => return Err(StoreError::Io(self.path.clone(), err)),
		};
		let file: File = serde_json::from_slice(&bytes)
			.map_err(|err| StoreError::Json(self.path.clone(), err))?;
		let document = Document::from_json(&file.document)
			.map_err(|err| StoreError::Invalid(self.path.clone(), err))?;
		if let Some(full) = file.gauge_full
			&& !(full.is_finite() && full > 0.0)
		{
			return Err(StoreError::GaugeFull(self.path.clone(), full));
		}
		Ok(Some(Stored {
			document,
			gauge_full: file.gauge_full,
		}))
	}

	/// What to start from: the file, or the shipped curve where there is none or it cannot be used,
	/// saying why in the latter case.
	pub fn load(&self) -> Stored {
		match self.read() {
			Ok(Some(stored)) => stored,
			Ok(None) => Stored::default(),
			Err(err) => {
				tracing::warn!(%err, "the battery curve file cannot be used; starting from the shipped curve");
				Stored::default()
			}
		}
	}

	/// Replace the file with `stored`, creating its directory where needed.
	///
	/// Through a temporary file beside it, synced, renamed over it, and the directory synced, so the
	/// rename itself survives a power cut.
	pub fn save(&self, stored: &Stored) -> Result<(), StoreError> {
		write_atomically(&self.path, &to_json(stored))
			.map_err(|err| StoreError::Io(self.path.clone(), err))
	}
}

fn to_json(stored: &Stored) -> Json {
	let mut file = Map::new();
	file.insert("document".to_owned(), stored.document.to_json());
	if let Some(full) = stored.gauge_full {
		file.insert("gauge-full".to_owned(), full.into());
	}
	Json::Object(file)
}

fn write_atomically(path: &Path, body: &Json) -> io::Result<()> {
	let directory = match path.parent() {
		Some(parent) if !parent.as_os_str().is_empty() => parent,
		_ => Path::new("."),
	};
	fs::create_dir_all(directory)?;

	let mut temporary = path.as_os_str().to_owned();
	temporary.push(".new");
	let temporary = PathBuf::from(temporary);

	let mut body = serde_json::to_vec_pretty(body).map_err(io::Error::other)?;
	body.push(b'\n');
	let mut file = fs::File::create(&temporary)?;
	file.write_all(&body)?;
	file.sync_all()?;
	drop(file);

	fs::rename(&temporary, path)?;
	fs::File::open(directory)?.sync_all()
}

/// Why the curve file could not be read or replaced.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
	#[error("{0}: {1}")]
	Io(PathBuf, #[source] io::Error),

	#[error("{0} is not a curve file: {1}")]
	Json(PathBuf, #[source] serde_json::Error),

	#[error("{0} holds a curve document that cannot be loaded: {1}")]
	Invalid(PathBuf, #[source] Invalid),

	#[error("{0} holds a gauge full reading of {1}, which is not above 0")]
	GaugeFull(PathBuf, f64),
}
