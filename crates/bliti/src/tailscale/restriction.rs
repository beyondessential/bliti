//! A device that accepts one tailnet only (TSC, "Signing in").

use anyhow::{Result, bail};

use super::client::LocalApi;

/// Refuse where the device is signed in to a tailnet other than `accepted`.
///
/// A device signed out, or in the accepted tailnet, is fine, and so is one whose tailscaled does not
/// answer: the restriction applies when something signs in, and the device stays reachable to be
/// diagnosed.
pub async fn check_at_startup(api: &dyn LocalApi, accepted: Option<&str>) -> Result<()> {
	let Some(accepted) = accepted else {
		return Ok(());
	};
	let status = match api.status().await {
		Ok(status) => status,
		Err(err) => {
			tracing::warn!(%err, "cannot tell which tailnet the device is in; starting regardless");
			return Ok(());
		}
	};
	match status
		.current_tailnet
		.filter(|tailnet| !tailnet.name.is_empty())
	{
		Some(tailnet) if tailnet.name != accepted => bail!(
			"this device accepts only the {accepted} tailnet and is signed in to {}",
			tailnet.name
		),
		_ => Ok(()),
	}
}

#[cfg(test)]
mod tests;
