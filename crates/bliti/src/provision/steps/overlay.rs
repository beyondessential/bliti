//! The step `overlay`: install the HAT overlay into the system on the disk, in the current boot slot
//! and where the system's kernel updates take their overlays from (PDSK).

use std::{
	fs,
	path::{Path, PathBuf},
	sync::Arc,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;

use super::{IMAGES, blocking};
use crate::provision::{
	context::Context,
	luks::Luks,
	step::{Done, Step},
	storage::{FIRMWARE, Storage, partition, single_disk},
	system::{open_root, read, replace},
};

/// The overlay's file name, on the medium and in both places it is installed.
const NAME: &str = "tamanu-iti-v4.dtbo";

/// Where, under the firmware partition, the firmware reads the current boot slot's overlays.
const SLOT: &str = "current/overlays";

/// Where, under the root filesystem, the system's kernel updates take the overlays of each boot slot
/// they stage from.
const UPDATES: &str = "etc/flash-kernel/dtbs/overlays";

/// The step.
pub struct Overlay {
	storage: Arc<dyn Storage>,
	luks: Arc<dyn Luks>,
	images: PathBuf,
}

impl Overlay {
	/// A step installing into the system on the disk `storage` finds.
	pub fn new(storage: Arc<dyn Storage>, luks: Arc<dyn Luks>) -> Self {
		Self::with_images(storage, luks, IMAGES)
	}

	fn with_images(
		storage: Arc<dyn Storage>,
		luks: Arc<dyn Luks>,
		images: impl AsRef<Path>,
	) -> Self {
		Self {
			storage,
			luks,
			images: images.as_ref().to_owned(),
		}
	}

	/// The overlay the medium carries.
	async fn overlay(&self) -> Result<Vec<u8>, String> {
		let path = self.images.join(NAME);
		blocking(move || read(&path))
			.await
			.map_err(|err| {
				format!("cannot read the HAT overlay the provisioning medium carries: {err}")
			})?
			.ok_or_else(|| "the provisioning medium carries no HAT overlay".to_owned())
	}

	/// What the disk lacks of the overlay, in a phrase for each place, or why that cannot be told.
	async fn lacks(&self, overlay: &[u8]) -> Result<Vec<String>, String> {
		let (storage, luks, overlay) = (self.storage.clone(), self.luks.clone(), overlay.to_vec());
		blocking(move || {
			let disk = single_disk(&*storage)?;
			let firmware = storage
				.mount(&partition(&disk, FIRMWARE), "vfat", "ro")
				.map_err(|_| "the disk has no firmware partition to install into".to_owned())?;
			let root = open_root(&*storage, &luks, &disk, false)?;
			Ok(lacks(firmware.path(), root.path(), &overlay))
		})
		.await
	}
}

#[async_trait]
impl Step for Overlay {
	fn id(&self) -> &str {
		"overlay"
	}

	fn title(&self) -> &str {
		"Install the HAT overlay"
	}

	// spec: PDSK#done-state-of-the-step-overlay
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		let overlay = match self.overlay().await {
			Ok(overlay) => overlay,
			Err(why) => return unknown(why),
		};
		match self.lacks(&overlay).await {
			Ok(lacks) if lacks.is_empty() => Done {
				state: DoneState::Done,
				why: Some("the HAT overlay is installed".to_owned()),
			},
			Ok(lacks) => Done {
				state: DoneState::NotDone,
				why: Some(lacks.join("; ")),
			},
			Err(why) => unknown(why),
		}
	}

	// spec: PDSK#installing-the-overlay
	async fn run(&self, _context: &mut Context) -> Result<(), String> {
		let overlay = self.overlay().await?;
		let (storage, luks) = (self.storage.clone(), self.luks.clone());
		blocking(move || {
			let disk = single_disk(&*storage)?;
			let firmware = storage
				.mount(&partition(&disk, FIRMWARE), "vfat", "rw")
				.map_err(|_| "the disk has no firmware partition to install into".to_owned())?;
			if !firmware.path().join(SLOT).is_dir() {
				return Err("the disk's firmware partition has no current boot slot".to_owned());
			}
			let root = open_root(&*storage, &luks, &disk, true)?;
			if !root.path().join("etc").is_dir() {
				return Err("the disk holds no root filesystem to install into".to_owned());
			}
			if lacks(firmware.path(), root.path(), &overlay).is_empty() {
				return Ok(());
			}
			for place in places(firmware.path(), root.path()) {
				if let Some(dir) = place.parent() {
					fs::create_dir_all(dir)
						.map_err(|err| format!("cannot make {}: {err}", dir.display()))?;
				}
				replace(&place, &overlay)
					.map_err(|err| format!("cannot write {}: {err}", place.display()))?;
			}
			let lacks = lacks(firmware.path(), root.path(), &overlay);
			if lacks.is_empty() {
				Ok(())
			} else {
				Err(format!(
					"the overlay did not read back: {}",
					lacks.join("; ")
				))
			}
		})
		.await
	}
}

/// The two places the overlay is installed.
fn places(firmware: &Path, root: &Path) -> [PathBuf; 2] {
	[
		firmware.join(SLOT).join(NAME),
		root.join(UPDATES).join(NAME),
	]
}

/// A phrase for each place that does not hold `overlay`.
fn lacks(firmware: &Path, root: &Path, overlay: &[u8]) -> Vec<String> {
	let [slot, updates] = places(firmware, root);
	let mut lacks = Vec::new();
	for (place, what) in [
		(slot, "the current boot slot"),
		(updates, "the directory kernel updates take overlays from"),
	] {
		match read(&place) {
			Ok(Some(held)) if held == overlay => {}
			Ok(Some(_)) => lacks.push(format!("the overlay in {what} differs")),
			Ok(None) => lacks.push(format!("the overlay is missing from {what}")),
			Err(err) => lacks.push(format!("cannot read the overlay in {what}: {err}")),
		}
	}
	lacks
}

#[cfg(test)]
mod tests;
