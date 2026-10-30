//! The step `bootloader`: write the Iti's bootloader configuration, and bring the board's
//! bootloader up to the one the medium carries (PBL).

use std::{
	fs, io,
	path::{Path, PathBuf},
	sync::Arc,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use jiff::civil::Date;

use super::{IMAGES, blocking};
use crate::provision::{
	bootloader::{Firmware, Held, config::Config, held},
	context::Context,
	step::{Done, Step},
};

/// The Iti's bootloader configuration (PBL, "The configuration").
const SETTINGS: [(&str, &str); 6] = [
	("BOOT_ORDER", "0xf61"),
	("POWER_OFF_ON_HALT", "1"),
	("PSU_MAX_CURRENT", "5000"),
	("PCIE_PROBE", "1"),
	("BOOT_UART", "1"),
	("HDMI_DELAY", "0"),
];

/// The bootloader the medium carries.
struct Medium {
	image: Vec<u8>,
	date: Date,
}

/// The step.
pub struct Bootloader {
	firmware: Arc<dyn Firmware>,
	images: PathBuf,
}

impl Bootloader {
	/// A step reaching the board's bootloader through `firmware`.
	pub fn new(firmware: Arc<dyn Firmware>) -> Self {
		Self::with_images(firmware, IMAGES)
	}

	fn with_images(firmware: Arc<dyn Firmware>, images: impl AsRef<Path>) -> Self {
		Self {
			firmware,
			images: images.as_ref().to_owned(),
		}
	}

	/// The bootloader the medium carries, and its date from the manifest.
	// spec: PBL#the-bootloader
	async fn medium(&self) -> Result<Medium, String> {
		let (bootloader, manifest) = (
			self.images.join("pieeprom.bin"),
			self.images.join("manifest"),
		);
		blocking(move || {
			let image = fs::read(&bootloader).map_err(|err| match err.kind() {
				io::ErrorKind::NotFound => {
					"the provisioning medium carries no bootloader".to_owned()
				}
				_ => format!("cannot read the bootloader the provisioning medium carries: {err}"),
			})?;
			let manifest = fs::read_to_string(&manifest)
				.map_err(|err| format!("cannot read the provisioning medium's manifest: {err}"))?;
			let date = manifest
				.lines()
				.find_map(|line| line.strip_prefix("BOOTLOADER_VERSION="))
				.and_then(|date| date.trim().parse().ok())
				.ok_or("the provisioning medium's manifest gives no date for its bootloader")?;
			Ok(Medium { image, date })
		})
		.await
	}

	/// What the board's bootloader holds.
	async fn held(&self) -> Result<Held, String> {
		let firmware = self.firmware.clone();
		blocking(move || held(&*firmware))
			.await
			.map_err(|err| format!("cannot read the board's bootloader: {err}"))
	}

	/// How the board's bootloader and configuration differ from a bootloader dated `medium` and the
	/// Iti's configuration, one phrase for each.
	fn differences(medium: Date, held: &Held) -> Vec<String> {
		let mut differences = Vec::new();
		if held.date < medium {
			differences.push(format!(
				"the bootloader is dated {} and the medium carries one dated {medium}",
				held.date
			));
		}
		differences.extend(held.config.differences(&SETTINGS));
		differences
	}
}

#[async_trait]
impl Step for Bootloader {
	fn id(&self) -> &str {
		"bootloader"
	}

	fn title(&self) -> &str {
		"Configure the bootloader"
	}

	// spec: PBL#done-state
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		let medium = match self.medium().await {
			Ok(medium) => medium,
			Err(why) => return unknown(why),
		};
		let held = match self.held().await {
			Ok(held) => held,
			Err(why) => return unknown(why),
		};
		let differences = Self::differences(medium.date, &held);
		if differences.is_empty() {
			Done {
				state: DoneState::Done,
				why: Some(format!(
					"the bootloader is dated {}, with the Iti's configuration",
					held.date
				)),
			}
		} else {
			Done {
				state: DoneState::NotDone,
				why: Some(differences.join("; ")),
			}
		}
	}

	// spec: PBL#carrying-out-the-step
	async fn run(&self, _context: &mut Context) -> Result<(), String> {
		let medium = self.medium().await?;
		let held = self.held().await?;
		if Self::differences(medium.date, &held).is_empty() {
			return Ok(());
		}

		// A bootloader as new as the medium's is kept, and only its configuration replaced.
		let date = medium.date;
		let base = if held.date < date {
			medium.image
		} else {
			held.image
		};
		let config = Config::of(&SETTINGS).to_string();
		let firmware = self.firmware.clone();
		blocking(move || {
			let image = firmware.with_config(&base, &config)?;
			firmware.write(&image)
		})
		.await
		.map_err(|err| format!("the board's bootloader could not be set: {err}"))?;

		let held = self.held().await?;
		let differences = Self::differences(date, &held);
		if differences.is_empty() {
			Ok(())
		} else {
			Err(format!(
				"the board's bootloader could not be set: it still differs: {}",
				differences.join("; ")
			))
		}
	}
}

#[cfg(test)]
mod tests;
