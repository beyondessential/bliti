//! The provisioning mode daemon: the USER button, the advertising window, the GATT server, the LED
//! and the console's socket, around one [`Provisioner`](super::Provisioner) (PRV).

use std::path::PathBuf;

use anyhow::Result;

/// What the provisioning daemon is told by means outside the protocol (PRV).
#[derive(Debug, Clone)]
pub struct Options {
	/// Bluetooth adapter to use. The system's first where absent.
	pub adapter: Option<String>,
	/// Where the provisioning medium is mounted, which a run's state is kept on.
	pub medium: PathBuf,
	/// Which kind of source of the board ID the device's own is to come from.
	pub board_id_source: bliti_core::board_id::SourceKind,
	/// Where the daemon listens for the console.
	pub socket: PathBuf,
}

/// Run provisioning mode until interrupted.
pub async fn run(_options: Options) -> Result<()> {
	anyhow::bail!("not yet")
}
