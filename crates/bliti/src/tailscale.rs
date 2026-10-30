//! Reporting Tailscale's state and carrying out a client's acts on it.
//!
//! Behaviour is specified in `.workhorse/specs/control/tailscale.md` (TSC), and the facts it reports in
//! the Tailscale section of `.workhorse/specs/device-info.md` (NFO). One [`Tailscale`] serves every
//! session, since the rule that one act is under way at a time spans every stream on every channel.
//!
//! State is held by a watcher on tailscaled's notification bus, so a change arrives as an event and
//! nothing polls. The acts go through tailscaled's local API, except `netcheck`, which Tailscale runs
//! in its command line and which the device runs as a command.

use std::{
	path::PathBuf,
	sync::{Arc, Mutex},
};

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::{Message, Secret},
	stream::{read_message, write_message},
};
use futures::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::{
	sync::{broadcast, mpsc, watch},
	task::JoinSet,
};

use crate::session::{Peer, SessionError};
use acts::{Act, CannotAct, Request, Slot};
use client::{LocalApi, Notify};
use netcheck::Runner;
use relays::Names;
use state::Observed;

pub mod acts;
pub mod client;
pub mod netcheck;
pub mod ping;
pub mod relays;
pub mod restriction;
mod run;
pub mod state;

#[cfg(test)]
pub(crate) mod fake;

/// The machine `ping` tests where a client names none and the deployment names none either.
pub const PING_DEFAULT: &str = "canopy";

/// What a device is configured with beyond tailscaled itself.
#[derive(Debug, Clone)]
pub struct Options {
	/// The one tailnet the device accepts, where it accepts one (TSC, "Signing in").
	pub tailnet: Option<String>,
	/// The machine `ping` tests where a client names none (TSC, "Ping").
	pub ping_default: String,
	/// Where what the device learns of Tailscale is kept, where it is kept at all.
	pub state_dir: Option<PathBuf>,
}

impl Default for Options {
	fn default() -> Self {
		Self {
			tailnet: None,
			ping_default: PING_DEFAULT.to_owned(),
			state_dir: None,
		}
	}
}

/// The one handle every session shares.
#[derive(Clone)]
pub struct Tailscale {
	inner: Arc<Inner>,
}

struct Inner {
	/// tailscaled, where the device runs one.
	daemon: Option<Daemon>,
	netcheck: Arc<dyn Runner>,
	names: Arc<Names>,
	options: Options,
	/// The act under way, which every other asked for is refused against.
	slot: Arc<Mutex<Option<Act>>>,
}

struct Daemon {
	api: Arc<dyn LocalApi>,
	observed: watch::Receiver<Observed>,
	/// What a sign-in waits on from the notification bus.
	events: broadcast::Sender<Notify>,
	/// The task keeping `observed` current, which ends with the last handle to this Tailscale.
	watcher: tokio::task::AbortHandle,
}

impl Drop for Daemon {
	fn drop(&mut self) {
		self.watcher.abort();
	}
}

impl Tailscale {
	/// A device running tailscaled, reached through `api`.
	///
	/// # Panics
	///
	/// Outside a Tokio runtime, which the watcher runs on.
	pub async fn start(
		api: Arc<dyn LocalApi>,
		netcheck: Arc<dyn Runner>,
		options: Options,
	) -> Self {
		let names = Arc::new(Names::new(options.state_dir.as_deref()));
		let first = state::observe(&*api).await;
		if first.state() == Some(state::State::Running) {
			names.refresh(&*api).await;
		}
		let (publish, observed) = watch::channel(first);
		let (events, _) = broadcast::channel(64);
		let watcher = tokio::spawn(state::watch(
			api.clone(),
			publish,
			events.clone(),
			names.clone(),
		))
		.abort_handle();
		Self::with(
			Some(Daemon {
				api,
				observed,
				events,
				watcher,
			}),
			netcheck,
			names,
			options,
		)
	}

	/// The Tailscale of this machine: tailscaled where the device runs Tailscale, and none where it
	/// runs none.
	///
	/// Refuses where the device is signed in to a tailnet other than the one it accepts.
	pub async fn connect(options: Options) -> anyhow::Result<Self> {
		let netcheck: Arc<dyn Runner> = Arc::new(netcheck::Command::on_path());
		#[cfg(unix)]
		{
			Self::connect_at(client::socket_path(), netcheck, options).await
		}
		#[cfg(not(unix))]
		{
			Ok(Self::without_tailscaled(netcheck, options))
		}
	}

	/// [`Self::connect`], with tailscaled's socket at `socket`.
	///
	/// The socket's absence is not the absence of Tailscale: tailscaled may be down, or not yet up
	/// when the daemon starts. A device that carries the `tailscale` package runs Tailscale, so it
	/// reports `broken` until tailscaled answers, and is picked up when it does (NFO, "Tailscale").
	#[cfg(unix)]
	async fn connect_at(
		socket: PathBuf,
		netcheck: Arc<dyn Runner>,
		options: Options,
	) -> anyhow::Result<Self> {
		if !socket.exists() && !netcheck.available() {
			return Ok(Self::without_tailscaled(netcheck, options));
		}
		let api: Arc<dyn LocalApi> = Arc::new(client::Unix::new(socket));
		restriction::check_at_startup(&*api, options.tailnet.as_deref()).await?;
		Ok(Self::start(api, netcheck, options).await)
	}

	/// The Tailscale of a tailscaled that another caller started, listening on `socket`.
	#[cfg(unix)]
	pub async fn on_socket(socket: PathBuf, options: Options) -> Self {
		let netcheck: Arc<dyn Runner> = Arc::new(netcheck::Command::on_path());
		Self::start(Arc::new(client::Unix::new(socket)), netcheck, options).await
	}

	/// A device that runs no Tailscale, and so reports none and lists no act but `netcheck`.
	pub fn without_tailscaled(netcheck: Arc<dyn Runner>, options: Options) -> Self {
		let names = Arc::new(Names::new(options.state_dir.as_deref()));
		Self::with(None, netcheck, names, options)
	}

	/// A device with nothing to do with Tailscale at all.
	#[cfg(test)]
	pub fn none() -> Self {
		struct Absent;
		#[async_trait::async_trait]
		impl Runner for Absent {
			fn available(&self) -> bool {
				false
			}
			async fn run(&self) -> Result<String, String> {
				Err("this device cannot run the network check".to_owned())
			}
		}
		Self::with(
			None,
			Arc::new(Absent),
			Arc::new(Names::in_memory()),
			Options::default(),
		)
	}

	fn with(
		daemon: Option<Daemon>,
		netcheck: Arc<dyn Runner>,
		names: Arc<Names>,
		options: Options,
	) -> Self {
		Self {
			inner: Arc::new(Inner {
				daemon,
				netcheck,
				names,
				options,
				slot: Arc::new(Mutex::new(None)),
			}),
		}
	}

	/// What the device observes of Tailscale now, where it runs it.
	pub fn observed(&self) -> Option<Observed> {
		let daemon = self.inner.daemon.as_ref()?;
		Some(daemon.observed.borrow().clone())
	}

	/// Sign this machine in with the auth key `key`, registering as `name`, and say whether it now
	/// waits on an administrator's approval. The act of TSC, "Signing in", for a caller that is not
	/// a client, so its tailnet restriction and its refusal to echo the key apply.
	// spec: PTS
	pub async fn sign_in_by_key(&self, key: Secret, name: &str) -> Result<bool, String> {
		self.inner.sign_in_by_key(key, name).await
	}

	/// The acts listed now.
	pub fn listed(&self) -> Vec<Act> {
		acts::listed(self.observed().as_ref(), self.inner.netcheck.available())
	}

	/// Settle what to answer an act asked for, logging it with the client that asked (TSC, "Logging").
	fn ask(&self, act: &str, params: Params, peer: &Peer) -> Result<(Request, Slot), CannotAct> {
		let (client, client_version) = peer.named();
		let answer = self.decide(act, params);
		match &answer {
			Ok((request, _)) => {
				tracing::info!(act = %request.act(), %client, %client_version, "act asked for and accepted");
			}
			Err(CannotAct { at, reason }) => {
				tracing::info!(%act, %client, %client_version, %at, %reason, "act asked for and refused");
			}
		}
		answer
	}

	fn decide(&self, act: &str, params: Params) -> Result<(Request, Slot), CannotAct> {
		let Some(listed) = Act::from_name(act).filter(|act| self.listed().contains(act)) else {
			return Err(CannotAct::not_listed(act));
		};
		let Params {
			method,
			key,
			name,
			peer,
		} = params;
		let request = Request::parse(listed, method.as_deref(), key, name, peer)?;
		let slot = Slot::take(&self.inner.slot, listed).map_err(CannotAct::busy)?;
		Ok((request, slot))
	}
}

/// What a `tailscale-act` carries beyond the act.
#[derive(Debug, Default)]
struct Params {
	method: Option<String>,
	key: Option<Secret>,
	name: Option<String>,
	peer: Option<String>,
}

/// Serve a Tailscale stream a client opened with `tailscale`, until it ends (TSC, "The exchange").
pub async fn serve<S>(
	stream: &mut S,
	tailscale: &Tailscale,
	peer: &Peer,
) -> Result<(), SessionError>
where
	S: futures::AsyncRead + AsyncWrite + Unpin,
{
	let (mut reader, mut writer) = stream.split();
	let mut observed = tailscale
		.inner
		.daemon
		.as_ref()
		.map(|daemon| daemon.observed.clone());

	let mut listed = tailscale.listed();
	send(&mut writer, &acts_message(&listed)).await?;

	// Read in a future of its own, so a message half read is never abandoned for another event.
	let (received, mut messages) = mpsc::unbounded_channel();
	let mut reading = std::pin::pin!(async move {
		loop {
			match read_message(&mut reader).await {
				Ok(Some(raw)) => {
					if received.send(Ok(raw)).is_err() {
						return;
					}
				}
				Ok(None) => return,
				Err(err) => {
					let _ = received.send(Err(err));
					return;
				}
			}
		}
	});
	let mut reading_done = false;

	let (outgoing, mut outbox) = mpsc::unbounded_channel::<Message>();
	// Dropped with this function however it ends, and so are the acts that wait on the outside.
	let mut owned = JoinSet::new();

	loop {
		tokio::select! {
			() = &mut reading, if !reading_done => reading_done = true,

			message = messages.recv() => {
				let Some(message) = message else {
					return Ok(());
				};
				let raw = message.map_err(|err| SessionError::Fault(err.to_string()))?;
				match read::<Message>(&raw) {
					Ok(Reading::Message(Message::TailscaleAct { act, method, key, name, peer: machine })) => {
						let params = Params { method, key, name, peer: machine };
						match tailscale.ask(&act, params, peer) {
							Ok((request, slot)) => {
								send(&mut writer, &Message::TailscaleAccepted).await?;
								let inner = tailscale.inner.clone();
								let outgoing = outgoing.clone();
								let waits = request.ends_with_its_stream();
								let run = async move {
									let _slot = slot;
									let outcome = inner.carry_out(&request, &outgoing).await;
									if let Some(message) = outcome.message() {
										let _ = outgoing.send(message);
									}
								};
								if waits {
									owned.spawn(run);
								} else {
									tokio::spawn(run);
								}
							}
							Err(CannotAct { at, reason }) => {
								send(&mut writer, &Message::CannotAct { act, at, reason }).await?;
							}
						}
					}
					Ok(Reading::Message(Message::Hello { name, version })) => {
						tracing::info!(client = %name, client_version = %version, "client named itself");
					}
					Ok(Reading::Message(_)) => {
						// Nothing a Tailscale stream does anything about, which MSG makes a no-op.
						tracing::debug!("a message the Tailscale stream has nothing to do about");
					}
					Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
					Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
					Err(fault) => {
						tracing::warn!(%fault, "protocol fault; closing the stream");
						return Err(SessionError::Fault(fault.to_string()));
					}
				}
			}

			Some(message) = outbox.recv() => send(&mut writer, &message).await?,

			// Reaped as they finish.
			Some(_) = owned.join_next(), if !owned.is_empty() => {}

			changed = async {
				match observed.as_mut() {
					Some(observed) => observed.changed().await.is_ok(),
					None => std::future::pending().await,
				}
			} => {
				if !changed {
					observed = None;
					continue;
				}
				let now = tailscale.listed();
				if now != listed {
					send(&mut writer, &acts_message(&now)).await?;
					listed = now;
				}
			}
		}
	}
}

fn acts_message(listed: &[Act]) -> Message {
	Message::TailscaleActs {
		acts: listed.iter().map(|act| act.name().to_owned()).collect(),
	}
}

async fn send<W: AsyncWrite + Unpin>(
	writer: &mut W,
	message: &Message,
) -> Result<(), SessionError> {
	write_message(writer, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	writer
		.flush()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}

#[cfg(test)]
mod tests;
