//! A tailscaled that answers from a script, for the tests above the client.
//!
//! It changes state the way the recorded sequences do: a call changes what `status` says and sends a
//! notification, so the watcher above it sees exactly what it would see from the real daemon.

use std::{
	collections::VecDeque,
	net::IpAddr,
	sync::{Arc, Mutex, PoisonError},
};

use async_trait::async_trait;
use bliti_core::channel::messages::Secret;
use futures::stream::{BoxStream, StreamExt};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

use super::client::{
	ApiError, DerpMap, LocalApi, Node, Notify, Pong, Prefs, Region, Status, Tailnet,
};

/// The key the fake accepts.
pub const GOOD_KEY: &str = "tskey-auth-good";

/// The tailnet a good key or an approved link joins.
pub const JOINED: &str = "fleet.example";

pub struct Fake {
	state: Mutex<Inner>,
	bus: broadcast::Sender<Notify>,
}

#[derive(Default)]
struct Inner {
	status: Status,
	calls: Vec<String>,
	pongs: VecDeque<Pong>,
	derp: Option<DerpMap>,
	joins: String,
	/// Whether the next call of its kind fails with the tailscaled's own refusal.
	refuse: bool,
	/// Whether a call changes state; a stuck daemon takes calls and does nothing.
	stuck: bool,
	/// Whether tailscaled answers at all.
	down: bool,
	/// Whether the key last given to `start` is one the fake accepts, and so whether the next
	/// `login-interactive` signs in.
	key_accepted: Option<bool>,
}

fn node(name: &str) -> Node {
	Node {
		dns_name: format!("{name}.{JOINED}."),
		host_name: name.to_owned(),
		tailscale_ips: Some(vec!["100.64.0.2".parse().unwrap()]),
		..Node::default()
	}
}

impl Fake {
	/// A daemon in `backend_state`, holding a node key where it is signed in.
	pub fn in_state(backend_state: &str) -> Arc<Self> {
		let signed_in = !matches!(backend_state, "NeedsLogin");
		let status = Status {
			backend_state: backend_state.to_owned(),
			have_node_key: signed_in,
			current_tailnet: signed_in.then(|| Tailnet {
				name: JOINED.to_owned(),
			}),
			this: Some(if signed_in {
				node("iti")
			} else {
				Node::default()
			}),
			..Status::default()
		};
		let (bus, _) = broadcast::channel(64);
		Arc::new(Self {
			state: Mutex::new(Inner {
				status,
				joins: JOINED.to_owned(),
				..Inner::default()
			}),
			bus,
		})
	}

	fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn set(&self, backend_state: &str, signed_in: bool) {
		{
			let mut inner = self.inner();
			inner.status.backend_state = backend_state.to_owned();
			inner.status.have_node_key = signed_in;
			inner.status.current_tailnet = signed_in.then(|| Tailnet {
				name: inner.joins.clone(),
			});
			inner.status.this = Some(if signed_in {
				node("iti")
			} else {
				Node::default()
			});
		}
		self.notify(Notify::default());
	}

	fn notify(&self, notify: Notify) {
		let _ = self.bus.send(notify);
	}

	/// Which tailnet the next successful sign-in joins.
	pub fn joins(&self, tailnet: &str) {
		self.inner().joins = tailnet.to_owned();
	}

	/// Move to `backend_state`, as tailscaled does on its own, and announce it.
	pub fn move_to(&self, backend_state: &str) {
		self.set(backend_state, backend_state != "NeedsLogin");
	}

	/// Make tailscaled stop answering.
	pub fn go_down(&self) {
		self.inner().down = true;
	}

	/// Make calls take and change nothing.
	pub fn stick(&self) {
		self.inner().stuck = true;
	}

	/// Make calls that change state fail.
	pub fn refuse(&self) {
		self.inner().refuse = true;
	}

	/// Add a peer, as `status` lists it.
	pub fn peer(&self, host_name: &str) {
		let mut inner = self.inner();
		inner
			.status
			.peer
			.get_or_insert_default()
			.insert(host_name.to_owned(), node(host_name));
	}

	/// Queue the answer to a probe.
	pub fn pong(&self, pong: Pong) {
		self.inner().pongs.push_back(pong);
	}

	/// Set the relay map.
	pub fn relays(&self, regions: &[(u32, &str)]) {
		self.inner().derp = Some(DerpMap {
			regions: regions
				.iter()
				.map(|(id, name)| {
					(
						*id,
						Region {
							region_name: (*name).to_owned(),
						},
					)
				})
				.collect(),
		});
	}

	/// Say what the signed-in link sign-in does: approve it, so the device joins.
	pub fn approve(&self) {
		self.set("Running", true);
	}

	/// Every call received, by name.
	pub fn calls(&self) -> Vec<String> {
		self.inner().calls.clone()
	}

	/// Announce an error on the bus, as a rejected key does.
	pub fn err(&self, message: &str) {
		self.notify(Notify {
			err_message: Some(message.to_owned()),
			..Notify::default()
		});
	}

	fn record(&self, call: &str) -> Result<(), ApiError> {
		let mut inner = self.inner();
		inner.calls.push(call.to_owned());
		if inner.refuse && call != "status" && call != "prefs" {
			return Err(ApiError::Refused {
				status: 403,
				message: "prefs write access denied".into(),
			});
		}
		Ok(())
	}

	fn stuck(&self) -> bool {
		self.inner().stuck
	}
}

#[async_trait]
impl LocalApi for Fake {
	async fn status(&self) -> Result<Status, ApiError> {
		let inner = self.inner();
		if inner.down {
			return Err(ApiError::Unreachable("down".to_owned()));
		}
		Ok(inner.status.clone())
	}

	async fn watch(&self) -> Result<BoxStream<'static, Result<Notify, ApiError>>, ApiError> {
		if self.inner().down {
			return Err(ApiError::Unreachable("down".to_owned()));
		}
		let first = futures::stream::once(async { Ok(Notify::default()) });
		let rest = BroadcastStream::new(self.bus.subscribe())
			.filter_map(|item| async move { item.ok().map(Ok) });
		Ok(first.chain(rest).boxed())
	}

	async fn prefs(&self) -> Result<Prefs, ApiError> {
		self.record("prefs")?;
		Ok(
			json!({"ControlURL": "https://control.example", "Hostname": "", "WantRunning": false})
				.as_object()
				.unwrap()
				.clone(),
		)
	}

	async fn set_running(&self, on: bool) -> Result<(), ApiError> {
		self.record(if on { "turn-on" } else { "turn-off" })?;
		if !self.stuck() {
			self.set(if on { "Running" } else { "Stopped" }, true);
		}
		Ok(())
	}

	async fn start(&self, prefs: Prefs, auth_key: Option<Secret>) -> Result<(), ApiError> {
		self.record(&format!(
			"start key={} hostname={}",
			auth_key.is_some(),
			prefs
				.get("Hostname")
				.and_then(|name| name.as_str())
				.unwrap_or("?")
		))?;
		let mut inner = self.inner();
		inner.key_accepted = auth_key.map(|key| key.expose() == GOOD_KEY && !inner.stuck);
		Ok(())
	}

	async fn login_interactive(&self) -> Result<(), ApiError> {
		self.record("login-interactive")?;
		let key_accepted = self.inner().key_accepted;
		match key_accepted {
			Some(true) => self.set("Running", true),
			Some(false) => self.err("the last login error was: invalid key"),
			None => self.notify(Notify {
				browse_to_url: Some("https://login.example/a/abc123".to_owned()),
				..Notify::default()
			}),
		}
		Ok(())
	}

	async fn logout(&self) -> Result<(), ApiError> {
		self.record("logout")?;
		if !self.stuck() {
			self.set("NeedsLogin", false);
		}
		Ok(())
	}

	async fn ping(&self, address: IpAddr) -> Result<Pong, ApiError> {
		self.record(&format!("ping {address}"))?;
		Ok(self.inner().pongs.pop_front().unwrap_or(Pong {
			err: "timeout".to_owned(),
			..Pong::default()
		}))
	}

	async fn derp_map(&self) -> Result<Option<DerpMap>, ApiError> {
		self.record("derpmap")?;
		Ok(self.inner().derp.clone())
	}
}
