//! How the device stands in Tailscale, and the watcher that keeps it current (NFO, "Tailscale").

use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use jiff::Timestamp;
use tokio::sync::{broadcast, watch};

use super::{
	client::{ApiError, LocalApi, Notify, Status},
	relays::Names,
};

/// How long to wait before looking for tailscaled again after it stopped answering.
const RECONNECT: Duration = Duration::from_secs(2);

/// How the device stands, by the wire names of `tailscale-state` (NFO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
	/// Connected.
	Running,
	/// Connecting.
	Starting,
	/// Signed in and turned off.
	Stopped,
	/// Holds no sign-in.
	NeedsLogin,
	/// The sign-in has lapsed.
	Expired,
	/// Signed in, and waiting for an administrator to approve the device.
	NeedsApproval,
}

impl State {
	/// The wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::Running => "running",
			Self::Starting => "starting",
			Self::Stopped => "stopped",
			Self::NeedsLogin => "needs-login",
			Self::Expired => "expired",
			Self::NeedsApproval => "needs-approval",
		}
	}
}

/// What the device knows of its standing, once tailscaled has told it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
	/// How the device stands.
	pub state: State,
	/// The name the device is reached by, without its trailing dot.
	pub name: Option<String>,
	/// The tailnet the device is a member of.
	pub tailnet: Option<String>,
	/// When the sign-in lapses, where it does.
	pub key_expiry: Option<Timestamp>,
	/// What Tailscale says is wrong with the connection, while connected.
	pub warning: Option<String>,
}

/// What the device observes of Tailscale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
	/// tailscaled answered.
	Standing(Snapshot),
	/// tailscaled is there and does not answer, or answers with something unreadable; says why.
	Broken(String),
}

impl Observed {
	/// How the device stands, where tailscaled answered.
	pub fn state(&self) -> Option<State> {
		match self {
			Self::Standing(snapshot) => Some(snapshot.state),
			Self::Broken(_) => None,
		}
	}

	/// What `status` says, mapped as the notes of the plan give (NFO).
	pub fn from_status(status: &Status) -> Self {
		let this = status.this.as_ref();
		let state = match status.backend_state.as_str() {
			"Running" => State::Running,
			"Starting" | "NoState" => State::Starting,
			"Stopped" => State::Stopped,
			"NeedsMachineAuth" => State::NeedsApproval,
			"NeedsLogin" if this.is_some_and(|node| node.expired) && status.have_node_key => {
				State::Expired
			}
			"NeedsLogin" => State::NeedsLogin,
			other => {
				return Self::Broken(format!("tailscaled reports an unknown state, {other:?}"));
			}
		};
		let name = this
			.map(|node| node.dns_name.trim_end_matches('.'))
			.filter(|name| !name.is_empty())
			.map(ToOwned::to_owned);
		let tailnet = status
			.current_tailnet
			.as_ref()
			.map(|tailnet| tailnet.name.clone())
			.filter(|name| !name.is_empty());
		let key_expiry = this
			.and_then(|node| node.key_expiry.as_deref())
			.and_then(|expiry| expiry.parse::<Timestamp>().ok())
			// Go's zero time, which is how a sign-in that does not lapse is written.
			.filter(|expiry| expiry.as_second() > -62_000_000_000);
		let warning = (state == State::Running)
			.then(|| status.health.as_ref()?.first().cloned())
			.flatten();
		Self::Standing(Snapshot {
			state,
			name,
			tailnet,
			key_expiry,
			warning,
		})
	}
}

/// Read tailscaled's status once.
pub async fn observe(api: &dyn LocalApi) -> Observed {
	match api.status().await {
		Ok(status) => Observed::from_status(&status),
		Err(err) => Observed::Broken(err.to_string()),
	}
}

/// Keep `observed` current from the notification bus, forwarding what a sign-in waits on to `events`
/// and refreshing the relay names whenever the device connects.
///
/// Reconnects with a short delay when tailscaled goes away, holding [`Observed::Broken`] meanwhile.
/// Runs until dropped.
pub async fn watch(
	api: Arc<dyn LocalApi>,
	observed: watch::Sender<Observed>,
	events: broadcast::Sender<Notify>,
	names: Arc<Names>,
) {
	loop {
		match api.watch().await {
			Ok(mut bus) => {
				publish(&observed, observe(&*api).await, &api, &names).await;
				while let Some(message) = bus.next().await {
					match message {
						Ok(notify) => {
							if notify.err_message.is_some() || notify.browse_to_url.is_some() {
								let _ = events.send(notify);
							}
							publish(&observed, observe(&*api).await, &api, &names).await;
						}
						Err(err) => {
							tracing::debug!(%err, "the notification bus broke");
							break;
						}
					}
				}
				publish(
					&observed,
					Observed::Broken(ApiError::Unreachable("the bus closed".into()).to_string()),
					&api,
					&names,
				)
				.await;
			}
			Err(err) => {
				publish(&observed, Observed::Broken(err.to_string()), &api, &names).await;
			}
		}
		tokio::time::sleep(RECONNECT).await;
	}
}

async fn publish(
	observed: &watch::Sender<Observed>,
	now: Observed,
	api: &Arc<dyn LocalApi>,
	names: &Names,
) {
	let connected = now.state() == Some(State::Running);
	let changed = observed.send_if_modified(|held| {
		if *held == now {
			return false;
		}
		*held = now.clone();
		true
	});
	if changed {
		tracing::info!(state = ?now.state().map(State::name), "Tailscale changed");
		if connected {
			names.refresh(api.as_ref()).await;
		}
	}
}

#[cfg(test)]
mod tests;
