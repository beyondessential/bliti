//! The step `eeprom`: write the HAT EEPROM the image for the board's version, while the operator
//! bridges the pads that lift its write protection (PEEP).

use std::{
	path::{Path, PathBuf},
	sync::Arc,
	time::Duration,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use tokio::time::{sleep, timeout};

use self::hat::{Held, describe};
use super::{
	IMAGES, blocking,
	hardware::place::{Air, place},
};
use crate::{
	facts::{i2c, product::Version},
	provision::{
		context::Context,
		step::{Done, Step},
	},
};

mod chip;
mod hat;

/// The ID bus, which the provisioning medium's firmware configuration brings up as this node.
const BUS: &str = "/dev/i2c-9";

/// How long the operator has to bridge the pads (PEEP, "Writing").
const WAIT: Duration = Duration::from_secs(5 * 60);

/// How long between tries at a write while the pads are not bridged.
const RETRY: Duration = Duration::from_millis(500);

/// How much of the EEPROM is read to tell what is on it: more than any HAT's header and vendor
/// information take, and little enough to read in a moment.
const PEEK: usize = 1024;

/// The EEPROM, read and written whole, each by blocking calls that say why they failed.
pub trait Memory: Send + Sync {
	/// Read the first `len` bytes.
	fn read(&self, len: usize) -> Result<Vec<u8>, String>;

	/// Try once to write `image` from the first byte. Fails while the EEPROM is write-protected.
	fn write(&self, image: &[u8]) -> Result<(), String>;
}

/// The CAT24C256 on the ID bus.
pub struct Cat24c256;

impl Cat24c256 {
	fn with<T>(work: impl FnOnce(&mut i2c::Bus) -> Result<T, i2c::Error>) -> Result<T, String> {
		let mut bus = i2c::Bus::open(BUS).map_err(|err| err.to_string())?;
		work(&mut bus).map_err(|err| err.to_string())
	}
}

impl Memory for Cat24c256 {
	fn read(&self, len: usize) -> Result<Vec<u8>, String> {
		Self::with(|bus| chip::read(bus, len))
	}

	fn write(&self, image: &[u8]) -> Result<(), String> {
		Self::with(|bus| chip::write(bus, image))
	}
}

/// The step.
pub struct Eeprom {
	air: Arc<dyn Air>,
	memory: Arc<dyn Memory>,
	images: PathBuf,
}

impl Eeprom {
	/// A step placing the board through `air` and reaching the EEPROM through `memory`.
	pub fn new(air: Arc<dyn Air>, memory: Arc<dyn Memory>) -> Self {
		Self::with_images(air, memory, IMAGES)
	}

	fn with_images(air: Arc<dyn Air>, memory: Arc<dyn Memory>, images: impl AsRef<Path>) -> Self {
		Self {
			air,
			memory,
			images: images.as_ref().to_owned(),
		}
	}

	/// The image the medium carries for `version`.
	// spec: PEEP
	async fn image(&self, version: Version) -> Result<Vec<u8>, String> {
		let path = self.images.join(format!("tamanu-iti-v{version}.eep"));
		blocking(move || std::fs::read(path)).await.map_err(|err| {
			if err.kind() == std::io::ErrorKind::NotFound {
				format!("the provisioning medium carries no image for v{version}")
			} else {
				format!("cannot read the image for v{version}: {err}")
			}
		})
	}

	/// What the EEPROM holds, read as far as it takes to tell.
	// spec: PEEP
	async fn held(&self, image: &[u8]) -> Result<Held, String> {
		let memory = self.memory.clone();
		let wanted = image.len().max(PEEK);
		let head = blocking(move || memory.read(wanted)).await?;
		if head.starts_with(image) {
			return Ok(Held::Image);
		}
		if head.iter().all(|byte| *byte == 0xff) {
			let memory = self.memory.clone();
			let all = blocking(move || memory.read(chip::SIZE)).await?;
			return Ok(describe(&all));
		}
		Ok(describe(&head))
	}

	/// Try to write `image` until the EEPROM reads back as holding it, or `WAIT` has run out.
	// spec: PEEP
	async fn write(&self, image: &[u8]) -> bool {
		let attempts = async {
			loop {
				let (memory, bytes) = (self.memory.clone(), image.to_vec());
				let written = blocking(move || {
					memory.write(&bytes)?;
					memory.read(bytes.len()).map(|read| read == bytes)
				})
				.await;
				if written == Ok(true) {
					return;
				}
				sleep(RETRY).await;
			}
		};
		timeout(WAIT, attempts).await.is_ok()
	}
}

#[async_trait]
impl Step for Eeprom {
	fn id(&self) -> &str {
		"eeprom"
	}

	fn title(&self) -> &str {
		"Program the EEPROM"
	}

	// spec: PEEP
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		let Some(version) = place(&self.air).await else {
			return unknown("the board could not be placed".to_owned());
		};
		let image = match self.image(version).await {
			Ok(image) => image,
			Err(why) => return unknown(why),
		};
		match self.held(&image).await {
			Ok(Held::Image) => Done {
				state: DoneState::Done,
				why: Some(format!("holds the image for Tamanu Iti v{version}")),
			},
			Ok(held) => Done {
				state: DoneState::NotDone,
				why: Some(format!("holds {held}")),
			},
			Err(err) => unknown(format!("cannot read the EEPROM: {err}")),
		}
	}

	// spec: PEEP
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let version = place(&self.air).await.ok_or(
			"the board could not be placed as a version of the Tamanu Iti and needs looking at",
		)?;
		let image = self.image(version).await?;
		let held = self
			.held(&image)
			.await
			.map_err(|err| format!("cannot read the EEPROM: {err}"))?;
		match &held {
			Held::Image => return Ok(()),
			Held::Blank => {}
			Held::Iti(held) if *held == version => {}
			held => {
				let text = format!(
					"The EEPROM holds {held}. Write the image for Tamanu Iti v{version} over it?"
				);
				if !context.confirm("overwrite", &text).await {
					return Err("the EEPROM was left as it was".to_owned());
				}
			}
		}

		let notice = context.notice(
			"id-wp",
			"Bridge the two ID WP pads with bare metal, such as the blades of scissors. \
			 Coated tools do not conduct. Hold the bridge until this step passes.",
		);
		let written = self.write(&image).await;
		notice.close();
		if written {
			Ok(())
		} else {
			Err("the EEPROM could not be written within five minutes: check that bare metal bridges both ID WP pads"
				.to_owned())
		}
	}
}

#[cfg(test)]
mod tests;
