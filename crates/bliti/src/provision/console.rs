//! The provisioning console command: a run driven from the device's own console with text prompts
//! (PCON).

use std::path::Path;

use anyhow::Result;

/// Drive a run through the daemon listening on `socket`.
pub async fn run(_socket: &Path) -> Result<()> {
	anyhow::bail!("not yet")
}
