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
//! handed to the configuration session of [`crate::network::session`]. Beyond that it never answers a
//! message on the wire: one it does not recognise is passed over, one carrying something critical it
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
	key_schedule::PresenceToken,
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::task::{AbortHandle, JoinSet};

use crate::{
	facts,
	network::{
		self,
		session::{Backend, Configurator},
	},
	sampler::Sampler,
};

/// How often to look for a change in the facts the device reports about itself.
const FACTS_POLL: Duration = Duration::from_secs(2);

/// The one topic an end may push, and so the one a resume asks for by name (NFO, MSG).
const DEFAULT_TOPIC: &str = "default";

/// What the device calls itself to a client, and the version it is at.
///
/// Both are opaque to the client, which displays them and never acts on them (MSG). They are the
/// package's own name and version, so a device reports what was actually built and installed.
const DEVICE_NAME: &str = env!("CARGO_PKG_NAME");
const DEVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a deliberate teardown waits for the connection to close before dropping it.
const CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

/// Which topics are being served right now. A topic is served on at most one stream (MSG), so a
/// second attempt to serve one already in this set is skipped.
type Served = Arc<Mutex<HashSet<String>>>;

/// Run a session to completion over a transport, as the device.
pub async fn run<S, B>(
	transport: S,
	secret: &PresenceToken,
	sampler: Sampler,
	configurator: Configurator<B>,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
	B: Backend,
{
	let encrypted = accept_responder(transport, secret)
		.await
		.map_err(|err| SessionError::Handshake(err.to_string()))?;
	tracing::info!("handshake complete");

	let (mut streams, driver) = multiplex(encrypted, Mode::Server);
	let mut driving = tokio::spawn(async move {
		if let Err(err) = driver.await {
			if is_peer_fault(&err) {
				tracing::warn!(%err, "the peer is not speaking the protocol; closing the connection");
			} else {
				tracing::debug!(%err, "connection closed");
			}
		}
	});
	// A session can be dropped from outside, as the daemon does when its client unsubscribes, and the
	// connection must end with it rather than keep serving streams nobody reads.
	let _driving = AbortOnDrop(driving.abort_handle());

	// Holds sampling open for as long as this session lasts, so a device that had gone quiet starts
	// sampling again the moment somebody connects (NFO).
	let _session = sampler.session();

	let result = converse(&mut streams, &sampler, &configurator).await;

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
) -> Result<(), SessionError> {
	let served: Served = Arc::new(Mutex::new(HashSet::new()));
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
		tasks.spawn(async move {
			let mut feed = feed;
			if let Err(err) = serve_default(&mut feed, &sampler, &served).await {
				tracing::debug!(%err, "the pushed feed ended");
			}
		});
	}

	loop {
		let accepted = tokio::select! {
			accepted = streams.accept() => accepted,
			// Reaped as they finish, so a long session does not accumulate them.
			Some(_) = tasks.join_next(), if !tasks.is_empty() => continue,
		};
		let Some(mut stream) = accepted else {
			tracing::info!("client disconnected");
			return Ok(());
		};
		let sampler = sampler.clone();
		let served = served.clone();
		let configurator = configurator.clone();
		tasks.spawn(async move {
			if let Err(err) = serve_stream(&mut stream, &sampler, &served, &configurator).await {
				tracing::debug!(%err, "stream ended");
			}
		});
	}
}

/// Aborts a task when dropped.
struct AbortOnDrop(AbortHandle);

impl Drop for AbortOnDrop {
	fn drop(&mut self) {
		self.0.abort();
	}
}

/// Serve one stream a client opened, until it ends. Whatever happens here leaves the other streams
/// and the connection alive.
async fn serve_stream<S, B>(
	stream: &mut S,
	sampler: &Sampler,
	served: &Served,
	configurator: &Configurator<B>,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
	B: Backend,
{
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
				// Recorded so what is talking to these devices can be known. Nothing branches on it.
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Ok(Reading::Message(Message::Subscribe { topic })) if topic == DEFAULT_TOPIC => {
				tracing::info!(%topic, "serving a subscription");
				return serve_default(stream, sampler, served).await;
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
				Message::Applied
				| Message::Invalid { .. }
				| Message::Busy
				| Message::Networks { .. }
				| Message::Spectrum { .. },
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
/// live, until the client closes the stream.
///
/// Claims the topic first. A topic is served on at most one stream, so if it is already being served
/// this returns without sending anything, which is how a `subscribe` for a feed already pushed is
/// skipped (MSG).
async fn serve_default<S>(
	stream: &mut S,
	sampler: &Sampler,
	served: &Served,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let Some(_guard) = TopicGuard::claim(served, DEFAULT_TOPIC) else {
		tracing::info!("default is already being served; skipping this one");
		return Ok(());
	};

	// The facts first, so the header and the shapes a reading is drawn against are present, then the
	// current readings, so every tile fills at once rather than over the next few seconds.
	let mut last_facts = facts::facts(now());
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
				let current = facts::facts(now());
				if !same_facts(&current, &last_facts) {
					tracing::info!("what the device reports about itself changed");
					for fact in &current {
						if write_message(&mut writer, &Message::Fact(fact.clone()).to_json())
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
mod tests {
	use bliti_core::channel::stream::{Stream, connect_initiator};
	use tokio_util::compat::TokioAsyncReadCompatExt;

	use super::*;
	use crate::network::session::{Inert, Store};

	fn secret(byte: u8) -> PresenceToken {
		PresenceToken::from_bytes([byte; 32])
	}

	/// A configurator that configures nothing, over a recorded configuration nothing here writes.
	async fn inert() -> Configurator<Inert> {
		let path = std::env::temp_dir().join("bliti-session-tests-never-written/network.json");
		let fallback = serde_json::json!({"attachments": []})
			.as_object()
			.cloned()
			.unwrap();
		Configurator::start(Inert, Store::new(path), fallback)
			.await
			.unwrap()
	}

	/// Open a client against a device session over an in-memory duplex, with no BLE involved.
	async fn paired(psk: &PresenceToken) -> Streams {
		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let device_psk = psk.clone();
		let configurator = inert().await;
		tokio::spawn(async move {
			let _ = run(
				device_side.compat(),
				&device_psk,
				Sampler::start(),
				configurator,
			)
			.await;
		});

		let encrypted = connect_initiator(client_side.compat(), psk).await.unwrap();
		let (streams, driver) = multiplex(encrypted, Mode::Client);
		tokio::spawn(async move {
			let _ = driver.await;
		});
		streams
	}

	/// Collect the messages a stream carries until it goes quiet for a moment.
	async fn drain(stream: &mut Stream) -> Vec<Message> {
		let mut messages = Vec::new();
		while let Ok(Ok(Some(raw))) =
			tokio::time::timeout(Duration::from_millis(400), read_message(stream)).await
		{
			if let Ok(Reading::Message(message)) = read::<Message>(&raw) {
				messages.push(message);
			}
		}
		messages
	}

	/// The device opens two streams unprompted: a hello, and the default feed with no announcement of
	/// the topic it serves (MSG).
	#[tokio::test]
	async fn the_device_pushes_a_hello_and_the_default_feed_unprompted() {
		let mut streams = paired(&secret(0x42)).await;

		let mut first = streams.accept().await.expect("a stream");
		let mut second = streams.accept().await.expect("a second stream");

		let a = drain(&mut first).await;
		let b = drain(&mut second).await;
		let all: Vec<Message> = a.into_iter().chain(b).collect();

		// One is the hello; the rest are facts and readings on the feed, none announcing a topic.
		assert!(
			all.iter()
				.any(|m| matches!(m, Message::Hello { name, .. } if name == DEVICE_NAME)),
			"the device names itself"
		);
		assert!(
			all.iter()
				.any(|m| matches!(m, Message::Fact(_) | Message::Reading(_))),
			"the feed carries facts and readings"
		);
		assert!(
			!all.iter().any(|m| matches!(m, Message::Subscribe { .. })),
			"the feed announces no topic"
		);
	}

	/// A client that declines the feed by closing it still holds the device's name and version from
	/// the hello, and can resume with a `subscribe` for `default` (MSG).
	#[tokio::test]
	async fn declining_the_feed_keeps_the_hello_and_a_subscribe_resumes() {
		let mut streams = paired(&secret(0x42)).await;

		// Find the hello stream and the feed stream among the two the device pushes.
		let mut one = streams.accept().await.unwrap();
		let two = streams.accept().await.unwrap();
		let first = drain(&mut one).await;
		let named = first.iter().any(|m| matches!(m, Message::Hello { .. }));
		let (mut hello_stream, mut feed) = if named { (one, two) } else { (two, one) };
		let _ = drain(&mut feed).await;

		// Decline the feed by closing it; the hello stream is untouched.
		feed.close().await.unwrap();
		let _ = &mut hello_stream;

		// Resume with a subscribe for default, and be served current data.
		let mut resume = streams.open().await.unwrap();
		write_message(
			&mut resume,
			&Message::Subscribe {
				topic: DEFAULT_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();
		let served = drain(&mut resume).await;
		assert!(
			served
				.iter()
				.any(|m| matches!(m, Message::Fact(_) | Message::Reading(_))),
			"a resume is served what is current"
		);
	}

	/// A subscribe for a topic already being served is skipped, so a client cannot receive it twice
	/// (MSG). The pushed feed is already serving default, so a subscribe for it draws nothing.
	#[tokio::test]
	async fn a_subscribe_for_an_already_served_topic_is_skipped() {
		let mut streams = paired(&secret(0x42)).await;
		// Leave the two pushed streams open, so default stays served.
		let _pushed_a = streams.accept().await.unwrap();
		let _pushed_b = streams.accept().await.unwrap();

		let mut duplicate = streams.open().await.unwrap();
		write_message(
			&mut duplicate,
			&Message::Subscribe {
				topic: DEFAULT_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();

		let quiet = drain(&mut duplicate).await;
		assert!(quiet.is_empty(), "a second serve of default sends nothing");
	}

	/// An unrecognised message draws no reply and costs nothing: the stream stays open (MSG).
	#[tokio::test]
	async fn an_unrecognised_message_is_passed_over_in_silence() {
		let mut streams = paired(&secret(0x42)).await;
		let _a = streams.accept().await.unwrap();

		let mut stream = streams.open().await.unwrap();
		write_message(&mut stream, br#"{"type":"reboot","when":"now"}"#)
			.await
			.unwrap();

		let quiet =
			tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
		assert!(quiet.is_err(), "nothing is answered on the wire");

		write_message(
			&mut stream,
			&Message::Hello {
				name: "test-client".to_owned(),
				version: "0.0.0".to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();
	}

	/// A known message the device has nothing to do about is a no-op, not a fault: the stream stays
	/// open and the session carries on (MSG).
	#[tokio::test]
	async fn a_known_but_unactionable_message_is_a_no_op() {
		let mut streams = paired(&secret(0x42)).await;
		let _a = streams.accept().await.unwrap();

		// A client reporting a reading of its own. The device has nothing to do about it.
		let mut stream = streams.open().await.unwrap();
		write_message(
			&mut stream,
			&Message::Reading(Entry::quantity(
				1,
				"signal-strength",
				"decibel-milliwatts",
				-71.0,
			))
			.to_json(),
		)
		.await
		.unwrap();

		let quiet =
			tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
		assert!(quiet.is_err(), "a no-op is silent on the wire");

		// The stream is still usable.
		write_message(
			&mut stream,
			&Message::Hello {
				name: "c".to_owned(),
				version: "0".to_owned(),
			}
			.to_json(),
		)
		.await
		.expect("the stream stays open");
	}

	/// A client that is not speaking the protocol loses the stream it did it on, and nothing else.
	#[tokio::test]
	async fn a_protocol_fault_costs_the_stream_and_not_the_connection() {
		let mut streams = paired(&secret(0x42)).await;
		let _a = streams.accept().await.unwrap();

		let mut bad = streams.open().await.unwrap();
		write_message(&mut bad, b"this is not json").await.unwrap();
		let ended = matches!(read_message(&mut bad).await, Ok(None) | Err(_));
		assert!(ended, "a fault closes the stream it arrived on");

		// The connection is untouched: a subscribe for default is still served.
		let mut good = streams.open().await.unwrap();
		write_message(
			&mut good,
			&Message::Subscribe {
				topic: DEFAULT_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();
		// It may be quiet if the pushed feed still holds default; either way the connection lives.
		let _ = drain(&mut good).await;
	}

	#[tokio::test]
	async fn a_client_with_the_wrong_code_cannot_open_a_session() {
		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let configurator = inert().await;
		let device = tokio::spawn(async move {
			run(
				device_side.compat(),
				&secret(0x01),
				Sampler::start(),
				configurator,
			)
			.await
		});

		assert!(
			connect_initiator(client_side.compat(), &secret(0x02))
				.await
				.is_err()
		);
		assert!(matches!(
			device.await.unwrap(),
			Err(SessionError::Handshake(_))
		));
	}
	/// A stream opened with `configure` is served a configuration session, and a second one while it
	/// is open is told the device is busy (CFG).
	#[tokio::test]
	async fn configure_opens_a_session_and_a_second_is_busy() {
		let mut streams = paired(&secret(0x42)).await;
		let _a = streams.accept().await.unwrap();

		let mut first = streams.open().await.unwrap();
		write_message(&mut first, &Message::Configure.to_json())
			.await
			.unwrap();
		let opened = read::<Message>(&read_message(&mut first).await.unwrap().unwrap()).unwrap();
		let Reading::Message(Message::Configuration {
			document,
			capabilities: Some(capabilities),
		}) = opened
		else {
			panic!("a session opens with the configuration and capabilities, got {opened:?}");
		};
		assert_eq!(
			document,
			serde_json::json!({"attachments": []})
				.as_object()
				.cloned()
				.unwrap()
		);
		assert!(capabilities.is_empty(), "this build states no capabilities");

		let mut second = streams.open().await.unwrap();
		write_message(&mut second, &Message::Configure.to_json())
			.await
			.unwrap();
		let answer = read::<Message>(&read_message(&mut second).await.unwrap().unwrap()).unwrap();
		assert_eq!(answer, Reading::Message(Message::Busy));
		assert!(
			matches!(read_message(&mut second).await, Ok(None) | Err(_)),
			"the busy stream is closed"
		);
	}

	/// A session dropped from outside, as the daemon drops one whose client unsubscribed, takes the
	/// streams it was serving with it, so a configuration session it held ends and the next client is
	/// not told the device is busy (CFG). The transport is left healthy throughout, so nothing but the
	/// drop can end it.
	#[tokio::test]
	async fn dropping_a_session_ends_the_configuration_session_it_held() {
		let psk = secret(0x42);
		let configurator = inert().await;

		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let device = {
			let psk = psk.clone();
			let configurator = configurator.clone();
			tokio::spawn(async move {
				let _ = run(device_side.compat(), &psk, Sampler::start(), configurator).await;
			})
		};
		let encrypted = connect_initiator(client_side.compat(), &psk).await.unwrap();
		let (mut streams, driver) = multiplex(encrypted, Mode::Client);
		tokio::spawn(async move {
			let _ = driver.await;
		});
		let _hello = streams.accept().await.unwrap();

		let mut held = streams.open().await.unwrap();
		write_message(&mut held, &Message::Configure.to_json())
			.await
			.unwrap();
		read_message(&mut held).await.unwrap().unwrap();

		device.abort();
		let _ = device.await;

		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		tokio::spawn({
			let psk = psk.clone();
			async move {
				let _ = run(device_side.compat(), &psk, Sampler::start(), configurator).await;
			}
		});
		let encrypted = connect_initiator(client_side.compat(), &psk).await.unwrap();
		let (mut streams, driver) = multiplex(encrypted, Mode::Client);
		tokio::spawn(async move {
			let _ = driver.await;
		});
		let mut next = streams.open().await.unwrap();
		write_message(&mut next, &Message::Configure.to_json())
			.await
			.unwrap();
		let answer = tokio::time::timeout(Duration::from_secs(5), read_message(&mut next))
			.await
			.expect("the device answers")
			.unwrap()
			.unwrap();
		assert!(
			matches!(
				read::<Message>(&answer).unwrap(),
				Reading::Message(Message::Configuration { .. })
			),
			"the next client opens a session rather than being told the device is busy"
		);
	}
}
