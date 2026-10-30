//! The steps a device in provisioning mode offers, in the order a run carries them out (RUN, "Steps").

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use bliti_core::provisioning::{Name, Suffix};
use tokio::sync::watch;

use super::{
	bootloader::Spi,
	button::{self, Seen},
	luks::{Cryptsetup, Luks},
	step::Step,
	storage::{Machine, Storage},
	system::Dracut,
};
use crate::facts::{environment, i2c};

pub mod bootloader;
pub mod disk;
pub mod eeprom;
pub mod encryption;
pub mod hardware;
pub mod overlay;
pub mod tailscale;
#[cfg(test)]
mod testing;

/// Where the provisioning medium carries the images, as the build installs them.
const IMAGES: &str = "/usr/share/bliti/provision";

/// The steps, in order. `name` is the device's name, which is published once the board's ID has been
/// established, and `medium` is where the provisioning medium's data partition is mounted.
pub fn all(user: Seen, name: watch::Receiver<Option<Name>>, medium: &Path) -> Vec<Arc<dyn Step>> {
	let board = Arc::new(Board);
	let storage: Arc<dyn Storage> = Arc::new(Machine);
	let luks: Arc<dyn Luks> = Arc::new(Cryptsetup);
	vec![
		Arc::new(hardware::Hardware::new(
			board.clone(),
			user,
			button::case_switch(),
		)),
		Arc::new(eeprom::Eeprom::new(
			board.clone(),
			Arc::new(eeprom::Cat24c256),
		)),
		Arc::new(disk::Disk::new(storage.clone())),
		Arc::new(overlay::Overlay::new(storage.clone(), luks.clone())),
		Arc::new(encryption::Encryption::new(
			storage.clone(),
			luks.clone(),
			Arc::new(Dracut),
			board,
			Arc::new(Named(name.clone())),
			medium,
		)),
		Arc::new(tailscale::Tailscale::new(storage, luks, name)),
		Arc::new(bootloader::Bootloader::new(Arc::new(Spi))),
	]
}

/// The board this device is, reached through the hardware it runs on.
struct Board;

impl hardware::place::Air for Board {
	fn chip_id(&self) -> Result<u8, i2c::Error> {
		environment::air_chip_id()
	}
}

impl hardware::Checks for Board {
	fn air(&self) -> Result<(), String> {
		environment::check_air()
	}

	fn light(&self) -> Result<(), String> {
		environment::check_light()
	}

	fn tpm(&self) -> Result<(), String> {
		tpm()
	}
}

/// The board's TPM and the suffix of its name, which the daemon publishes once it has derived it.
struct Named(watch::Receiver<Option<Name>>);

#[async_trait]
impl encryption::Identity for Named {
	fn tpm(&self) -> Result<(), String> {
		tpm()
	}

	async fn suffix(&self) -> Suffix {
		let mut name = self.0.clone();
		match name.wait_for(Option::is_some).await {
			Ok(name) => name
				.as_ref()
				.map_or(Suffix::Fault, |name| name.suffix.clone()),
			// The daemon went away without establishing a name.
			Err(_) => Suffix::Fault,
		}
	}
}

/// Read the board ID from the TPM's Endorsement Key, as the daemon does (BID).
// spec: PHW
fn tpm() -> Result<(), String> {
	#[cfg(feature = "tpm")]
	{
		use bliti_core::board_id::{
			BoardId, BoardIdSource as _, Presence, TpmEndorsementKeySource,
		};

		let source = TpmEndorsementKeySource::new();
		match source.probe() {
			Ok(Presence::Present) => {}
			Ok(Presence::Absent | Presence::Placeholder) => {
				return Err("the TPM does not answer: no device node for it".to_owned());
			}
			Err(err) => return Err(format!("the TPM does not answer: {err}")),
		}
		let name = source
			.read()
			.map_err(|err| format!("the TPM yields no board ID: {err}"))?;
		BoardId::new(source.kind(), name)
			.map(drop)
			.map_err(|err| format!("the TPM yields no board ID: {err}"))
	}
	#[cfg(not(feature = "tpm"))]
	{
		Err("this build cannot read the TPM".to_owned())
	}
}

/// Run blocking work off the async runtime.
///
/// Under test it runs in place, since a paused clock moves on while a task waits on a thread, and
/// that would run a step's time limit out in the middle of work that takes no time.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
	#[cfg(test)]
	{
		work()
	}
	#[cfg(not(test))]
	{
		tokio::task::spawn_blocking(work)
			.await
			.expect("blocking work does not panic")
	}
}
