//! The steps a device in provisioning mode offers, in the order a run carries them out (RUN, "Steps").

use std::sync::Arc;

use super::{
	bootloader::Spi,
	button::{self, Seen},
	step::Step,
};
use crate::facts::{environment, i2c};

pub mod bootloader;
pub mod eeprom;
pub mod hardware;
#[cfg(test)]
mod testing;

/// The steps, in order.
pub fn all(user: Seen) -> Vec<Arc<dyn Step>> {
	let board = Arc::new(Board);
	vec![
		Arc::new(hardware::Hardware::new(
			board.clone(),
			user,
			button::case_switch(),
		)),
		Arc::new(eeprom::Eeprom::new(board, Arc::new(eeprom::Cat24c256))),
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
