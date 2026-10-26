//! The names of Tailscale's relay regions, held on the device so a result can name a relay after the
//! device has signed out (TSC, "Relays in a result").
//!
//! A device that has never signed in holds no relay map, which is why a name may be unknown.

use std::{
	collections::BTreeMap,
	path::{Path, PathBuf},
	sync::{Mutex, PoisonError},
};

use serde_json::{Map, Value as Json, json};

use super::client::LocalApi;

/// Where the names are kept, within the state directory.
const FILE: &str = "relays.json";

/// Region names by number, kept on disk.
#[derive(Debug, Default)]
pub struct Names {
	path: Option<PathBuf>,
	held: Mutex<BTreeMap<u32, String>>,
}

impl Names {
	/// Names kept in `directory` where there is one, read now. A missing or unreadable file is an empty
	/// cache.
	pub fn new(directory: Option<&Path>) -> Self {
		directory.map_or_else(Self::default, Self::load)
	}

	/// Names kept in `directory`, read now.
	pub fn load(directory: impl AsRef<Path>) -> Self {
		let path = directory.as_ref().join(FILE);
		let held = match std::fs::read_to_string(&path) {
			Ok(raw) => parse(&raw).unwrap_or_else(|| {
				tracing::warn!(path = %path.display(), "the relay names are unreadable; starting without");
				BTreeMap::new()
			}),
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
			Err(err) => {
				tracing::warn!(path = %path.display(), %err, "cannot read the relay names; starting without");
				BTreeMap::new()
			}
		};
		Self {
			path: Some(path),
			held: Mutex::new(held),
		}
	}

	#[cfg(test)]
	pub fn in_memory() -> Self {
		Self::default()
	}

	/// The name of region `id`, where it is known.
	pub fn name(&self, id: u32) -> Option<String> {
		self.held
			.lock()
			.unwrap_or_else(PoisonError::into_inner)
			.get(&id)
			.cloned()
	}

	/// Replace the names with `names`, and keep them.
	pub fn replace(&self, names: BTreeMap<u32, String>) {
		let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
		if *held == names {
			return;
		}
		if let Some(path) = &self.path {
			if let Err(err) = write_atomically(path, &names) {
				tracing::warn!(path = %path.display(), %err, "cannot keep the relay names");
			}
		}
		*held = names;
	}

	/// Take the names from the relay map tailscaled holds, where it holds one.
	pub async fn refresh(&self, api: &dyn LocalApi) {
		match api.derp_map().await {
			Ok(Some(map)) if !map.regions.is_empty() => self.replace(
				map.regions
					.into_iter()
					.filter(|(_, region)| !region.region_name.is_empty())
					.map(|(id, region)| (id, region.region_name))
					.collect(),
			),
			Ok(_) => {}
			Err(err) => tracing::debug!(%err, "cannot read the relay map"),
		}
	}
}

fn parse(raw: &str) -> Option<BTreeMap<u32, String>> {
	let Json::Object(object) = serde_json::from_str(raw).ok()? else {
		return None;
	};
	object
		.into_iter()
		.map(|(id, name)| Some((id.parse().ok()?, name.as_str()?.to_owned())))
		.collect()
}

fn write_atomically(path: &std::path::Path, names: &BTreeMap<u32, String>) -> std::io::Result<()> {
	let object: Map<String, Json> = names
		.iter()
		.map(|(id, name)| (id.to_string(), json!(name)))
		.collect();
	if let Some(directory) = path.parent() {
		std::fs::create_dir_all(directory)?;
	}
	let temporary = path.with_extension("json.tmp");
	std::fs::write(&temporary, Json::Object(object).to_string())?;
	std::fs::rename(temporary, path)
}

#[cfg(test)]
mod tests;
