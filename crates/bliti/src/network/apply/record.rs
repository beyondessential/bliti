//! The record of what bliti last put in place, which is what a render is compared with.
//!
//! It holds each file's rendered contents rather than a digest of them, which needs no hash and
//! cannot collide. Those contents carry the same secrets as the files themselves, so the record is
//! kept with the mode of a secret.

use std::{
	collections::BTreeMap,
	fs::{self, DirBuilder},
	io,
	os::unix::fs::DirBuilderExt as _,
	path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
	super::render::{File, SECRET},
	Error, files,
};

/// The record's file within the state directory.
const NAME: &str = "applied.json";

#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Record {
	files: BTreeMap<PathBuf, Entry>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
	contents: String,
	mode: u32,
}

impl Record {
	/// The record under `state`, or an empty one where there is none.
	///
	/// A record that cannot be read is taken as empty, which costs one apply writing and picking up
	/// everything again and nothing more.
	pub(super) fn load(state: &Path) -> Self {
		let path = state.join(NAME);
		let bytes = match fs::read(&path) {
			Ok(bytes) => bytes,
			Err(e) if e.kind() == io::ErrorKind::NotFound => return Self::default(),
			Err(e) => {
				tracing::warn!(
					?path,
					"cannot read the network record, applying afresh: {e}"
				);
				return Self::default();
			}
		};
		serde_json::from_slice(&bytes).unwrap_or_else(|e| {
			tracing::warn!(
				?path,
				"cannot parse the network record, applying afresh: {e}"
			);
			Self::default()
		})
	}

	pub(super) fn save(&self, state: &Path) -> Result<(), Error> {
		let path = state.join(NAME);
		let saved = serde_json::to_vec(self)
			.map_err(io::Error::other)
			.and_then(|json| {
				DirBuilder::new()
					.recursive(true)
					.mode(0o700)
					.create(state)?;
				files::write_atomically(&path, &json, SECRET)
			});
		saved.map_err(|source| Error::Record { path, source })
	}

	/// Whether `file` is what was last put at its path.
	pub(super) fn holds(&self, file: &File) -> bool {
		self.files
			.get(&file.path)
			.is_some_and(|entry| entry.contents == file.contents && entry.mode == file.mode)
	}

	pub(super) fn contains(&self, path: &Path) -> bool {
		self.files.contains_key(path)
	}

	pub(super) fn paths(&self) -> impl Iterator<Item = &Path> {
		self.files.keys().map(PathBuf::as_path)
	}

	pub(super) fn retain(&mut self, mut keep: impl FnMut(&Path) -> bool) {
		self.files.retain(|path, _| keep(path));
	}

	pub(super) fn insert(&mut self, file: &File) {
		self.files.insert(
			file.path.clone(),
			Entry {
				contents: file.contents.clone(),
				mode: file.mode,
			},
		);
	}

	pub(super) fn remove(&mut self, path: &Path) {
		self.files.remove(path);
	}
}
