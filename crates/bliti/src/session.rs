//! One authenticated session: the handshake, the streams above it, and the messages they carry.
//!
//! This is written against any byte transport rather than against GATT, so the whole session can be
//! exercised over an in-memory duplex with no adapter involved, and so the same code serves a
//! different transport later without changing.
//!
//! As soon as the handshake completes the device opens two streams unprompted (MSG): a hello stream
//! naming itself, and a feed serving the `default` topic. A client declines the feed by closing it,
//! and resumes with a `subscribe` for `default`. A topic is served on at most one stream, so a
//! `subscribe` for one already being served is skipped. A stream whose client sends `configure` is
//! handed to the configuration session of [`crate::network::session`], one whose client sends
//! `power` to the power stream of [`crate::power`], and one whose client sends `curve` to the curve
//! stream of [`crate::battery`]. Beyond that it never answers a message on the wire: one it does not recognise is passed over, one carrying something critical it
//! does not know is refused, one it knows but has nothing to do about is a no-op, and one that breaks
//! the protocol closes the stream it arrived on.

use std::{
	collections::HashSet,
	sync::{Arc, Mutex},
	time::Duration,
};

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::Message,
		readings::Entry,
		stream::{
			Mode, Streams, accept_responder, is_peer_fault, multiplex, read_message, write_message,
		},
	},
	key_schedule::DeviceKeys,
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::task::{AbortHandle, JoinSet};
use tracing::Instrument;

use crate::{
	battery,
	facts::{self, Supply},
	network::{
		self,
		session::{Backend, Configurator},
	},
	power::{self, Controller, SessionGuard},
	sampler::{ENDED, Sampler, identity_key},
};

/// How often to look for a change in the facts the device reports about itself.
const FACTS_POLL: Duration = Duration::from_secs(2);

/// The one topic an end may push, and so the one a resume asks for by name (NFO, MSG).
const DEFAULT_TOPIC: &str = "default";

/// What the device calls itself to a client, and the version it is at.
///
/// Both are opaque to the client, which displays them and never acts on them (MSG). They are the
/// package's own name and version, so a device reports what was actually built and installed. A
/// development build's version carries the build it names itself as (see `build.rs`).
const DEVICE_NAME: &str = env!("CARGO_PKG_NAME");
const DEVICE_VERSION: &str = env!("BLITI_VERSION");

/// How long a deliberate teardown waits for the connection to close before dropping it.
const CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

/// What the client named itself as in its `hello`, once it has (MSG).
#[derive(Clone, Default)]
pub struct Peer(Arc<Mutex<Option<(String, String)>>>);

impl Peer {
	/// Record what the client named itself as.
	pub fn name(&self, name: String, version: String) {
		*self
			.0
			.lock()
			.expect("the name is never held across a panic") = Some((name, version));
	}

	/// The client's name and version, for a log line, or placeholders where it has not named itself.
	pub fn named(&self) -> (String, String) {
		self.0
			.lock()
			.expect("the name is never held across a panic")
			.clone()
			.unwrap_or_else(|| ("unnamed".to_owned(), "unknown".to_owned()))
	}
}

/// Which topics are being served right now. A topic is served on at most one stream (MSG), so a
/// second attempt to serve one already in this set is skipped.
type Served = Arc<Mutex<HashSet<String>>>;

/// Run a session to completion over a transport, as the device.
pub async fn run<S, B>(
	transport: S,
	keys: &DeviceKeys,
	sampler: Sampler,
	configurator: Configurator<B>,
	controller: Controller,
	supply: Supply,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
	B: Backend,
{
	let encrypted = accept_responder(transport, &keys.presence_token, &keys.static_key)
		.await
		.map_err(|err| SessionError::Handshake(err.to_string()))?;
	tracing::info!("handshake complete");

	let (mut streams, driver) = multiplex(encrypted, Mode::Server);
	let mut driving = tokio::spawn(
		async move {
			if let Err(err) = driver.await {
				if is_peer_fault(&err) {
					tracing::warn!(%err, "the peer is not speaking the protocol; closing the connection");
				} else {
					tracing::debug!(%err, "connection closed");
				}
			}
		}
		.in_current_span(),
	);
	// A session can be dropped from outside, as the daemon does when its client unsubscribes, and the
	// connection must end with it rather than keep serving streams nobody reads.
	let _driving = AbortOnDrop(driving.abort_handle());

	// Holds sampling open for as long as this session lasts, so a device that had gone quiet starts
	// sampling again the moment somebody connects (NFO).
	let _session = sampler.session();
	// Held until the connection has closed, so a device going away drops no link still closing (CTL).
	let mut held = controller.session();

	let result = converse(
		&mut streams,
		&sampler,
		&configurator,
		&controller,
		&supply,
		&mut held,
	)
	.await;

	// Hand the connection to the driver to close rather than abort it, so the client reads the clean
	// ending this is. Bounded, because a client already out of range never takes the closing frames.
	drop(streams);
	if tokio::time::timeout(CLOSE_TIMEOUT, &mut driving)
		.await
		.is_err()
	{
		tracing::debug!("the connection did not close in time; dropping it");
		driving.abort();
	}
	result
}

/// The device's half of the conversation: name itself and push the `default` feed, both unprompted,
/// and serve whatever streams the client opens.
async fn converse<B: Backend>(
	streams: &mut Streams,
	sampler: &Sampler,
	configurator: &Configurator<B>,
	controller: &Controller,
	supply: &Supply,
	held: &mut SessionGuard,
) -> Result<(), SessionError> {
	let served: Served = Arc::new(Mutex::new(HashSet::new()));
	let peer = Peer::default();
	// Every stream this session serves, so that dropping the session ends them all. A configuration
	// session left running would keep the device's one session busy and its proposal applied (CFG).
	let mut tasks = JoinSet::new();

	// The device names itself on a stream of its own, so declining the feed does not cost the client
	// the device's identity and version (MSG).
	let mut hello = streams
		.open()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	let message = Message::Hello {
		name: DEVICE_NAME.to_owned(),
		version: DEVICE_VERSION.to_owned(),
	};
	write_message(&mut hello, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	// The device pushes the `default` feed without being asked, on a separate stream (MSG, NFO).
	let feed = streams
		.open()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	{
		let sampler = sampler.clone();
		let served = served.clone();
		let configurator = configurator.clone();
		let controller = controller.clone();
		tasks.spawn(
			async move {
				let mut feed = feed;
				if let Err(err) =
					serve_default(&mut feed, &sampler, &served, &configurator, &controller).await
				{
					tracing::debug!(%err, "the pushed feed ended");
				}
			}
			.in_current_span(),
		);
	}

	loop {
		let accepted = tokio::select! {
			accepted = streams.accept() => accepted,
			// Reaped as they finish, so a long session does not accumulate them.
			Some(_) = tasks.join_next(), if !tasks.is_empty() => continue,
			// The device is going away, and every connection ends before it does (CTL).
			() = held.ending() => {
				tracing::info!("the device is going away; ending the session");
				return Ok(());
			}
		};
		let Some(mut stream) = accepted else {
			tracing::info!("client disconnected");
			return Ok(());
		};
		let context = Context {
			sampler: sampler.clone(),
			served: served.clone(),
			configurator: configurator.clone(),
			controller: controller.clone(),
			supply: supply.clone(),
			peer: peer.clone(),
		};
		tasks.spawn(
			async move {
				if let Err(err) = serve_stream(&mut stream, &context).await {
					tracing::debug!(%err, "stream ended");
				}
			}
			.in_current_span(),
		);
	}
}

/// Aborts a task when dropped, so it ends with whatever holds this rather than outliving it.
pub(crate) struct AbortOnDrop(pub(crate) AbortHandle);

impl Drop for AbortOnDrop {
	fn drop(&mut self) {
		self.0.abort();
	}
}

/// What a stream a client opened is served with.
struct Context<B> {
	sampler: Sampler,
	served: Served,
	configurator: Configurator<B>,
	controller: Controller,
	supply: Supply,
	peer: Peer,
}

/// Serve one stream a client opened, until it ends. Whatever happens here leaves the other streams
/// and the connection alive.
async fn serve_stream<S, B>(stream: &mut S, context: &Context<B>) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
	B: Backend,
{
	let Context {
		sampler,
		served,
		configurator,
		controller,
		supply,
		peer,
	} = context;
	loop {
		let raw = match read_message(stream).await {
			Ok(Some(raw)) => raw,
			Ok(None) => {
				tracing::debug!("client closed a stream");
				return Ok(());
			}
			Err(err) => return Err(SessionError::Fault(err.to_string())),
		};

		match read::<Message>(&raw) {
			Ok(Reading::Message(Message::Hello { name, version })) => {
				// Recorded so what is talking to these devices can be known. Nothing branches on it
				// beyond naming the client in the log of the acts it asks for (CTL).
				tracing::info!(client = %name, client_version = %version, "client named itself");
				peer.name(name, version);
			}
			Ok(Reading::Message(Message::Subscribe { topic })) if topic == DEFAULT_TOPIC => {
				tracing::info!(%topic, "serving a subscription");
				return serve_default(stream, sampler, served, configurator, controller).await;
			}
			Ok(Reading::Message(Message::Subscribe { topic })) => {
				// A topic this device does not serve is skipped: it sends nothing and leaves the stream
				// open until the client closes it. That is what a device older than its client looks
				// like, and it fails nothing (MSG).
				tracing::info!(%topic, "subscription to a topic this device does not serve");
			}
			Ok(Reading::Message(Message::Fact(_) | Message::Reading(_))) => {
				// A client reporting what the device cannot measure about itself. This device has
				// nothing to do about it, which MSG makes a no-op rather than a fault.
				tracing::debug!("a fact or reading from the client; nothing to do about it");
			}
			Ok(Reading::Message(Message::Configure)) => {
				return network::session::serve(stream, configurator).await;
			}
			Ok(Reading::Message(Message::Power)) => {
				return power::serve(stream, controller, peer).await;
			}
			Ok(Reading::Message(Message::Act { .. })) => {
				// An act on a stream that opened no power stream. Nothing to act on, which MSG makes a
				// no-op rather than a fault.
				tracing::debug!("an act outside a power stream; nothing to do");
			}
			Ok(Reading::Message(Message::Curve)) => {
				return battery::serve_stream(stream, supply, peer).await;
			}
			Ok(Reading::Message(Message::Load { .. } | Message::Reset)) => {
				// A curve change on a stream that opened no curve stream. Nothing to act on, which MSG
				// makes a no-op rather than a fault.
				tracing::debug!("a curve change outside a curve stream; nothing to do");
			}
			Ok(Reading::Message(
				Message::Configuration { .. }
				| Message::Confirm
				| Message::Discard
				| Message::Scan { .. }
				| Message::Survey { .. }
				| Message::Wps { .. },
			)) => {
				// A configuration-session message on a stream that opened no session. Nothing to act
				// on, which MSG makes a no-op rather than a fault.
				tracing::debug!("a configuration-session message outside a session; nothing to do");
			}
			Ok(Reading::Message(
				Message::Applied { .. }
				| Message::State { .. }
				| Message::Pin { .. }
				| Message::Invalid { .. }
				| Message::Busy
				| Message::Networks { .. }
				| Message::Spectrum { .. }
				| Message::Acts { .. }
				| Message::Accepted
				| Message::Refused { .. }
				| Message::GoingAway { .. }
				| Message::Curves { .. },
			)) => {
				// Answers only a device sends. A client sending one has nothing for this device to do
				// about it, a no-op rather than a fault (MSG).
				tracing::debug!("a device-only message from the client; nothing to do about it");
			}
			Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
			Ok(Reading::Refused(refusal)) => {
				tracing::warn!(%refusal, "message refused");
			}
			Err(fault) => {
				tracing::warn!(%fault, "protocol fault; closing the stream");
				return Err(SessionError::Fault(fault.to_string()));
			}
		}
	}
}

/// Serve the `default` topic on a stream: the device's facts and readings, current at once and then
/// live, until the client closes the stream, and `going-away` once an act has been accepted (CTL).
///
/// Claims the topic first. A topic is served on at most one stream, so if it is already being served
/// this returns without sending anything, which is how a `subscribe` for a feed already pushed is
/// skipped (MSG).
async fn serve_default<S, B>(
	stream: &mut S,
	sampler: &Sampler,
	served: &Served,
	configurator: &Configurator<B>,
	controller: &Controller,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let Some(_guard) = TopicGuard::claim(served, DEFAULT_TOPIC) else {
		tracing::info!("default is already being served; skipping this one");
		return Ok(());
	};
	let mut going = controller.feed();
	let mut told = false;

	// The facts first, so the header and the shapes a reading is drawn against are present, then the
	// current readings, so every tile fills at once rather than over the next few seconds.
	let mut last_facts = facts_now(configurator);
	for fact in &last_facts {
		write_message(stream, &Message::Fact(fact.clone()).to_json())
			.await
			.map_err(|err| SessionError::Stream(err.to_string()))?;
	}
	for reading in sampler.current() {
		write_message(stream, &Message::Reading(reading).to_json())
			.await
			.map_err(|err| SessionError::Stream(err.to_string()))?;
	}

	let mut live = sampler.live();
	let mut facts_poll = tokio::time::interval(FACTS_POLL);
	facts_poll.tick().await;

	// The client closing its sending side is the unsubscribe, read as end of stream. A half-close
	// leaves this end's write side open, so it must be read for: a device watching only for a failed
	// write would go on sending to a client that has said it is done (MSG).
	let (reader, mut writer) = stream.split();
	let mut ended = std::pin::pin!(until_end_of_stream(reader));

	loop {
		tokio::select! {
			biased;

			() = &mut ended => {
				tracing::debug!("client closed the feed; stopping");
				let _ = writer.close().await;
				return Ok(());
			}

			going_away = going.going(), if !told => {
				let message = Message::GoingAway {
					act: going_away.act.name().to_owned(),
					cause: going_away.cause.name().to_owned(),
				};
				if write_message(&mut writer, &message.to_json()).await.is_err() {
					return Ok(());
				}
				let _ = writer.flush().await;
				going.told();
				told = true;
			}

			received = live.recv() => match received {
				Ok(readings) => {
					for reading in readings {
						if write_message(&mut writer, &Message::Reading(reading).to_json())
							.await
							.is_err()
						{
							return Ok(());
						}
					}
				}
				Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
					tracing::debug!(missed, "subscriber fell behind");
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
			},

			_ = facts_poll.tick() => {
				let current = facts_now(configurator);
				if !same_facts(&current, &last_facts) {
					tracing::info!("what the device reports about itself changed");
					// A fact left out says nothing to a client already showing it (NFO).
					let kept: HashSet<String> = current.iter().map(identity_key).collect();
					let at = now();
					let ended = last_facts
						.iter()
						.filter(|fact| !kept.contains(&identity_key(fact)))
						.map(|fact| fact.ended(at, ENDED));
					for fact in ended.chain(current.iter().cloned()) {
						if write_message(&mut writer, &Message::Fact(fact).to_json())
							.await
							.is_err()
						{
							return Ok(());
						}
					}
					last_facts = current;
				}
			}
		}
	}
}

/// The facts the device reports now: what it reads off the system, and whether its network runs the
/// recorded configuration (NFO).
fn facts_now<B>(configurator: &Configurator<B>) -> Vec<Entry> {
	let at = now();
	let mut facts = facts::facts(at);
	let network = if configurator.provisional() {
		"provisional"
	} else {
		"recorded"
	};
	facts.push(Entry::text(at, "network-configuration", network));
	facts
}

/// Milliseconds since boot, for stamping a message as it is sent.
fn now() -> u64 {
	crate::facts::Facts::since_boot()
}

/// Whether two sets of facts carry the same information, ignoring when each was taken: `at` moves
/// every poll, and resending an unchanged fact each poll is what this exists to avoid.
fn same_facts(a: &[Entry], b: &[Entry]) -> bool {
	let strip = |entries: &[Entry]| -> Vec<Entry> {
		entries
			.iter()
			.map(|entry| {
				let mut entry = entry.clone();
				entry.at = 0;
				entry
			})
			.collect()
	};
	strip(a) == strip(b)
}

/// Holds a topic claimed for as long as it is being served, releasing it when the serve ends however
/// it ends.
struct TopicGuard {
	served: Served,
	topic: String,
}

impl TopicGuard {
	/// Claim a topic, or nothing where it is already being served.
	fn claim(served: &Served, topic: &str) -> Option<Self> {
		let mut set = served.lock().expect("the set is never held across a panic");
		if !set.insert(topic.to_owned()) {
			return None;
		}
		Some(Self {
			served: served.clone(),
			topic: topic.to_owned(),
		})
	}
}

impl Drop for TopicGuard {
	fn drop(&mut self) {
		self.served
			.lock()
			.expect("the set is never held across a panic")
			.remove(&self.topic);
	}
}

/// Resolve when the peer closes its sending side, or the stream ends any other way.
async fn until_end_of_stream<R: AsyncRead + Unpin>(mut reader: R) {
	let mut scratch = [0u8; 64];
	loop {
		match reader.read(&mut scratch).await {
			Ok(0) | Err(_) => return,
			Ok(_) => continue,
		}
	}
}

/// A failure within one session. None of these stops the daemon.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
	/// The handshake did not complete.
	#[error("handshake failed: {0}")]
	Handshake(String),

	/// A stream failed or the connection went away.
	#[error("stream: {0}")]
	Stream(String),

	/// A client sent something that is not this protocol. The stream it arrived on is closed; the
	/// connection and every other stream are left alone.
	#[error("protocol fault: {0}")]
	Fault(String),
}

#[cfg(test)]
mod tests;
