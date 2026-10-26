//! A device that accepts one tailnet only (TSC, "Signing in").

use std::time::Duration;

use anyhow::{Result, bail};
use tokio::time::Instant;

use super::client::{LocalApi, Status};

/// How long tailscaled has to settle after it starts, before the tailnet it is in is taken as
/// unknown.
const SETTLE: Duration = Duration::from_secs(15);

/// How often a status that has not settled is read again.
const SETTLE_POLL: Duration = Duration::from_millis(500);

/// Refuse where the device is signed in to a tailnet other than `accepted`.
///
/// A device signed out, or in the accepted tailnet, is fine, and so is one whose tailscaled does not
/// answer: the restriction applies when something signs in, and the device stays reachable to be
/// diagnosed.
///
/// A tailscaled that has only just started names no tailnet until it has its network map, so a
/// status that has not settled is read again, and the tailnet is taken as unknown, with a warning,
/// only if it does not settle in time.
pub async fn check_at_startup(api: &dyn LocalApi, accepted: Option<&str>) -> Result<()> {
	check_within(api, accepted, SETTLE).await
}

async fn check_within(api: &dyn LocalApi, accepted: Option<&str>, settle: Duration) -> Result<()> {
	let Some(accepted) = accepted else {
		return Ok(());
	};
	let deadline = Instant::now() + settle;
	loop {
		let status = match api.status().await {
			Ok(status) => status,
			Err(err) => {
				tracing::warn!(%err, "cannot tell which tailnet the device is in; starting regardless");
				return Ok(());
			}
		};
		let joined = status
			.current_tailnet
			.as_ref()
			.map(|tailnet| tailnet.name.as_str())
			.filter(|name| !name.is_empty());
		match joined {
			Some(joined) if joined != accepted => bail!(
				"this device accepts only the {accepted} tailnet and is signed in to {joined}"
			),
			Some(_) => return Ok(()),
			None if settled(&status) => return Ok(()),
			None if Instant::now() >= deadline => {
				tracing::warn!(
					"tailscaled has not said which tailnet the device is in; starting regardless"
				);
				return Ok(());
			}
			None => tokio::time::sleep(SETTLE_POLL).await,
		}
	}
}

/// Whether tailscaled has got past starting, so that no tailnet in its status means none.
fn settled(status: &Status) -> bool {
	!matches!(status.backend_state.as_str(), "" | "NoState" | "Starting")
}

#[cfg(test)]
mod tests;
