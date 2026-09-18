//! One authenticated session: the handshake, the streams above it, and the messages they carry.
//!
//! This is written against any byte transport rather than against GATT, so the whole session can be
//! exercised over an in-memory duplex with no adapter involved, and so the same code serves a
//! different transport later without changing.
//!
//! The device opens a reporting stream as soon as the handshake completes, naming itself and then
//! reporting what it is, and serves whatever streams a client opens (MSG). It never answers a
//! message on the wire: one it does not recognise is passed over, one carrying something critical it
//! does not know is refused, and one that breaks the protocol closes the stream it arrived on.

use std::time::Duration;

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::{ClientMessage, DeviceMessage},
		stream::{
			Mode, Streams, accept_responder, is_peer_fault, multiplex, read_message, write_message,
		},
	},
	key_schedule::PresenceToken,
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{facts::Facts, sampler::Sampler};

/// How often to look for a change in what the device reports about itself.
const IDENTITY_POLL: Duration = Duration::from_secs(2);

/// The topic carrying the device's live readings (SYS).
const SYSTEM_TOPIC: &str = "system";

/// What the device calls itself to a client, and the version it is at.
///
/// Both are opaque to the client, which displays them and never acts on them (MSG). They are the
/// package's own name and version, so a device reports what was actually built and installed.
const DEVICE_NAME: &str = env!("CARGO_PKG_NAME");
const DEVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How long a deliberate teardown waits for the connection to close before dropping it.
///
/// Closing is a term frame and the tail of the compression stream, a few hundred bytes, which a client
/// still in range takes within a connection interval or two. A client that has gone never takes them,
/// and the device asks to go back on the air only once this returns, so the bound is what keeps a
/// vanished client from holding the device off the air (ADV, "Advertising continuously"). Falling
/// through it costs only the clean ending, which the client reports as an ending either way.
const CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

/// Run a session to completion over a transport, as the device.
///
/// Returns once the client goes away or the channel fails. A failed handshake is an ordinary outcome
/// rather than an error worth stopping the daemon for: anyone in range can connect and try, and the
/// device stays reachable by a legitimate operator afterwards (ADV, "Advertising continuously").
pub async fn run<S>(
	transport: S,
	secret: &PresenceToken,
	sampler: Sampler,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	let encrypted = accept_responder(transport, secret)
		.await
		.map_err(|err| SessionError::Handshake(err.to_string()))?;
	tracing::info!("handshake complete");

	let (mut streams, driver) = multiplex(encrypted, Mode::Server);
	let mut driving = tokio::spawn(async move {
		if let Err(err) = driver.await {
			// A peer that cannot be decompressed or decrypted is a fault the device reports (CHN), and
			// it costs the whole connection because the compression context is shared by every stream and
			// is unrecoverable once it has diverged. An ordinary ending is not worth a warning.
			if is_peer_fault(&err) {
				tracing::warn!(%err, "the peer is not speaking the protocol; closing the connection");
			} else {
				tracing::debug!(%err, "connection closed");
			}
		}
	});

	// Holds sampling open for as long as this session lasts, so a device that had gone quiet starts
	// filling its window again the moment somebody connects (SYS).
	let _session = sampler.session();

	let result = converse(&mut streams, &sampler).await;

	// Hand the connection to the driver to close rather than abort it: closing finishes the compression
	// stream, which the client reads as the clean ending this is, where an aborted driver leaves the
	// stream unterminated and the client reports a fault (CHN). Bounded, because a client that has
	// already walked out of range will never take the closing frames.
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

/// The device's half of the conversation: name itself and report unsolicited, and serve whatever the
/// client opens.
async fn converse(streams: &mut Streams, sampler: &Sampler) -> Result<(), SessionError> {
	// The device speaks first, without being asked. This is the property the stream layer exists for.
	let mut reporting = streams
		.open()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	// Naming itself comes first on the reporting stream, so a client knows what it is talking to
	// before anything else arrives.
	let hello = DeviceMessage::Hello {
		name: DEVICE_NAME.to_owned(),
		version: DEVICE_VERSION.to_owned(),
	};
	write_message(&mut reporting, &hello.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	let facts = Facts::new();
	let mut last = facts.statics();
	write_message(
		&mut reporting,
		&DeviceMessage::SystemIdentity {
			readings: last.clone(),
		}
		.to_json(),
	)
	.await
	.map_err(|err| SessionError::Stream(err.to_string()))?;

	let mut ticker = tokio::time::interval(IDENTITY_POLL);
	ticker.tick().await;

	loop {
		tokio::select! {
			// What the device is changes rarely, but an address appearing is among the first things an
			// installer is waiting for, so it is sent as it happens rather than waited for.
			_ = ticker.tick() => {
				let current = facts.statics();
				if current != last {
					tracing::info!("what the device reports about itself changed");
					write_message(
						&mut reporting,
						&DeviceMessage::SystemIdentity { readings: current.clone() }.to_json(),
					)
					.await
					.map_err(|err| SessionError::Stream(err.to_string()))?;
					last = current;
				}
			}

			inbound = streams.accept() => {
				let Some(mut stream) = inbound else {
					tracing::info!("client disconnected");
					return Ok(());
				};
				let sampler = sampler.clone();
				tokio::spawn(async move {
					if let Err(err) = serve_stream(&mut stream, &sampler).await {
						tracing::debug!(%err, "stream ended");
					}
				});
			}
		}
	}
}

/// Serve a subscription to the device's live readings, until the client closes the stream.
///
/// The window goes first, so a graph is populated the moment it appears rather than filling from
/// empty while an operator waits (SYS).
async fn serve_system<S>(stream: &mut S, sampler: &Sampler) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	// The readings first, so every tile an operator is waiting for renders at once, and the window
	// after, so the graphs fill in behind them. The other way round the view sits empty for as long as
	// the window takes to cross the link, which is the part of it nobody is waiting on.
	if let Some(latest) = sampler.latest() {
		let now = DeviceMessage::SystemSample {
			at: latest.at,
			readings: latest.readings,
		};
		write_message(stream, &now.to_json())
			.await
			.map_err(|err| SessionError::Stream(err.to_string()))?;
	}

	let history = DeviceMessage::SystemHistory {
		series: sampler.series(),
	};
	write_message(stream, &history.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	let mut live = sampler.live();

	// The client closing its sending side is the unsubscribe, and reading end of stream is how the
	// device learns of it (MSG, "Subscribing"). It has to be read for: a half-close leaves this
	// end's write side open, so a device watching only for a failed write would go on sending to a
	// client that has said it is done, which is the case the mechanism exists to prevent.
	let (reader, mut writer) = stream.split();
	let mut ended = std::pin::pin!(until_end_of_stream(reader));

	loop {
		tokio::select! {
			// Biased, so an unsubscribe arriving alongside a sample ends the subscription rather than
			// racing one more write against it.
			biased;

			() = &mut ended => {
				tracing::debug!("client closed its side of a subscription; unsubscribing");
				// Closed in turn, so the client reads end of stream rather than a stream left hanging.
				let _ = writer.close().await;
				return Ok(());
			}

			received = live.recv() => match received {
				Ok(sample) => {
					let message = DeviceMessage::SystemSample {
						at: sample.at,
						readings: sample.readings,
					};
					// A write failing is the client having gone away by a route that is not a graceful
					// close: a reset, a dropped link, a closed connection. Each is equally the unsubscribe.
					if write_message(&mut writer, &message.to_json()).await.is_err() {
						return Ok(());
					}
				}
				// A client too slow to keep up misses samples rather than stalling the sampler. The next
				// one it receives is current, which is what a live view wants.
				Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
					tracing::debug!(missed, "subscriber fell behind");
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
			},
		}
	}
}

/// Resolve when the peer closes its sending side, or the stream ends any other way.
///
/// A subscription lasts exactly as long as its stream, so a reset or a dropped link ends it just as a
/// graceful close does, and none of them is a fault either end reports (MSG). Nothing is defined
/// on a subscription stream after the `subscribe`, so anything that arrives before the end is passed
/// over rather than acted on.
async fn until_end_of_stream<R: AsyncRead + Unpin>(mut reader: R) {
	let mut scratch = [0u8; 64];
	loop {
		match reader.read(&mut scratch).await {
			Ok(0) | Err(_) => return,
			Ok(_) => continue,
		}
	}
}

/// Serve one stream a client opened, until it ends. Whatever happens here leaves the other streams
/// and the connection alive.
async fn serve_stream<S>(stream: &mut S, sampler: &Sampler) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	loop {
		let raw = match read_message(stream).await {
			Ok(Some(raw)) => raw,
			// The end of the stream. Where it was a subscription, this is the unsubscribe: there is
			// nothing to stop yet because no topic is defined, but the shape is the one a feature
			// hangs its topic on (MSG, "Subscribing").
			Ok(None) => {
				tracing::debug!("client closed a stream");
				return Ok(());
			}
			// A length beyond what a message may be is a fault of the same kind as a malformed one.
			Err(err) => return Err(SessionError::Fault(err.to_string())),
		};

		match read::<ClientMessage>(&raw) {
			Ok(Reading::Message(ClientMessage::Hello { name, version })) => {
				// Recorded so that what is in the field talking to these devices can be known. Nothing
				// branches on it (MSG).
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Ok(Reading::Message(ClientMessage::Subscribe { topic })) if topic == SYSTEM_TOPIC => {
				tracing::info!(%topic, "serving a subscription");
				return serve_system(stream, sampler).await;
			}
			Ok(Reading::Message(ClientMessage::Subscribe { topic })) => {
				// A topic this device does not recognise is skipped as anything else is: it sends
				// nothing and leaves the stream open until the client closes it. That is what a device
				// older than its client looks like, and it fails nothing (MSG).
				tracing::info!(%topic, "subscription to a topic this device does not serve");
			}
			Ok(Reading::Skipped(skip)) => {
				tracing::debug!(%skip, "message passed over");
			}
			Ok(Reading::Refused(refusal)) => {
				// A client newer than this device, saying something that must not be half read. Not a
				// fault: the stream carries on.
				tracing::warn!(%refusal, "message refused");
			}
			Err(fault) => {
				// A client that is not speaking the protocol. Reported, and the stream goes.
				tracing::warn!(%fault, "protocol fault; closing the stream");
				return Err(SessionError::Fault(fault.to_string()));
			}
		}
	}
}

/// A failure within one session. None of these stops the daemon.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
	/// The handshake did not complete: a client that has not scanned this device's QR code, or noise
	/// on the link.
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
	use bliti_core::channel::stream::connect_initiator;
	use tokio_util::compat::TokioAsyncReadCompatExt;

	use super::*;

	fn secret(byte: u8) -> PresenceToken {
		PresenceToken::from_bytes([byte; 32])
	}

	/// Open a client against a device session over an in-memory duplex, with no BLE involved.
	async fn paired(psk: &PresenceToken) -> Streams {
		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let device_psk = psk.clone();
		tokio::spawn(async move {
			let _ = run(device_side.compat(), &device_psk, Sampler::start()).await;
		});

		let encrypted = connect_initiator(client_side.compat(), psk).await.unwrap();
		let (streams, driver) = multiplex(encrypted, Mode::Client);
		tokio::spawn(async move {
			let _ = driver.await;
		});
		streams
	}

	/// The device names itself first and reports second, both without being asked (MSG).
	#[tokio::test]
	async fn the_device_names_itself_then_reports_unsolicited() {
		let mut streams = paired(&secret(0x42)).await;

		let mut reporting = streams.accept().await.expect("device reports unsolicited");

		let raw = read_message(&mut reporting).await.unwrap().unwrap();
		let Reading::Message(DeviceMessage::Hello { name, version }) = read(&raw).unwrap() else {
			panic!("the first message on the reporting stream is the device hello");
		};
		assert_eq!(name, DEVICE_NAME);
		assert_eq!(version, DEVICE_VERSION);

		let raw = read_message(&mut reporting).await.unwrap().unwrap();
		let Reading::Message(DeviceMessage::SystemIdentity { readings }) = read(&raw).unwrap()
		else {
			panic!("the device says what it is after naming itself");
		};
		// Rendered from what each reading says about itself, so the test names no reading either.
		assert!(!readings.is_empty());
		assert!(readings.iter().all(|reading| reading.is_coherent()));
	}

	/// A message the device does not recognise draws no reply at all, and costs nothing: the stream
	/// stays open and the session carries on (MSG).
	#[tokio::test]
	async fn an_unrecognised_message_is_passed_over_in_silence() {
		let mut streams = paired(&secret(0x42)).await;
		let _reporting = streams.accept().await.unwrap();

		let mut stream = streams.open().await.unwrap();
		write_message(&mut stream, br#"{"type":"reboot","when":"now"}"#)
			.await
			.unwrap();

		let quiet =
			tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
		assert!(quiet.is_err(), "nothing is answered on the wire");

		// And the stream is still usable, rather than having been torn down.
		write_message(
			&mut stream,
			&ClientMessage::Hello {
				name: "test-client".to_owned(),
				version: "0.0.0".to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();
	}

	/// A client newer than this device, saying something that must not be half read, is refused and
	/// costs nothing: the stream stays open. This is the distinction most at risk of being collapsed
	/// into the fault path (MSG).
	///
	/// The vehicle is a message type this device does not know, marked critical by an upper case
	/// `TYPE`. It has to be: every type MSG defines forbids a critical member, so a refusal on one
	/// of those is impossible and a critical member there would be a fault instead.
	#[tokio::test]
	async fn a_refused_message_leaves_the_stream_open() {
		let mut streams = paired(&secret(0x42)).await;
		let _reporting = streams.accept().await.unwrap();

		let mut stream = streams.open().await.unwrap();
		write_message(&mut stream, br#"{"TYPE":"wipe","confirm":true}"#)
			.await
			.unwrap();

		// Nothing is answered, and the stream is not torn down: a further message is still served.
		let quiet =
			tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
		assert!(quiet.is_err(), "a refusal is silent on the wire");

		write_message(
			&mut stream,
			&ClientMessage::Hello {
				name: "test-client".to_owned(),
				version: "0.0.0".to_owned(),
			}
			.to_json(),
		)
		.await
		.expect("the stream a refusal arrived on stays open");
	}

	/// A client that is not speaking the protocol loses the stream it did it on, and nothing else
	/// (MSG, "What the base protocol guarantees").
	#[tokio::test]
	async fn a_protocol_fault_costs_the_stream_and_not_the_connection() {
		let mut streams = paired(&secret(0x42)).await;
		let mut reporting = streams.accept().await.unwrap();
		let _hello = read_message(&mut reporting).await.unwrap().unwrap();

		let mut bad = streams.open().await.unwrap();
		write_message(&mut bad, b"this is not json").await.unwrap();

		// The stream the fault arrived on ends.
		let ended = matches!(read_message(&mut bad).await, Ok(None) | Err(_));
		assert!(ended, "a fault closes the stream it arrived on");

		// The connection is untouched: another stream is still served.
		let mut good = streams.open().await.unwrap();
		write_message(
			&mut good,
			&ClientMessage::Subscribe {
				topic: SYSTEM_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();
		let raw = tokio::time::timeout(Duration::from_secs(2), read_message(&mut good))
			.await
			.expect("a served topic answers")
			.unwrap()
			.unwrap();
		assert!(matches!(
			read::<DeviceMessage>(&raw).unwrap(),
			// The readings come first, then the window.
			Reading::Message(
				DeviceMessage::SystemSample { .. } | DeviceMessage::SystemHistory { .. }
			)
		));
	}

	/// A topic this device does not serve is skipped like anything else it does not recognise: it
	/// sends nothing and leaves the stream open for the client to close. That is what a device older
	/// than its client looks like, and it fails nothing (MSG).
	#[tokio::test]
	async fn an_unknown_topic_is_quiet_and_leaves_the_stream_open() {
		let mut streams = paired(&secret(0x42)).await;
		let mut subscription = streams.open().await.unwrap();
		write_message(
			&mut subscription,
			&ClientMessage::Subscribe {
				topic: "weather".to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();

		let quiet =
			tokio::time::timeout(Duration::from_millis(250), read_message(&mut subscription)).await;
		assert!(quiet.is_err(), "nothing is sent for a topic not served");
	}

	/// The window comes before anything live, so a graph is populated the moment it appears rather
	/// than filling from empty while an operator waits (SYS).
	#[tokio::test]
	async fn a_subscription_receives_the_window_before_anything_live() {
		let mut streams = paired(&secret(0x42)).await;
		let mut subscription = streams.open().await.unwrap();
		write_message(
			&mut subscription,
			&ClientMessage::Subscribe {
				topic: SYSTEM_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();

		// The readings come first so the view fills at once; the window follows for the graphs.
		let mut seen = Vec::new();
		for _ in 0..2 {
			let raw = tokio::time::timeout(Duration::from_secs(2), read_message(&mut subscription))
				.await
				.expect("a message arrives")
				.unwrap()
				.unwrap();
			seen.push(read::<DeviceMessage>(&raw).unwrap());
		}
		let Reading::Message(DeviceMessage::SystemHistory { series }) = seen.pop().unwrap() else {
			panic!("the window follows the readings");
		};
		// It may be empty on a device that has only just started, which is a valid window.
		for each in &series {
			for pair in each.points.windows(2) {
				assert!(pair[0].0 <= pair[1].0, "a series is oldest first");
			}
		}
	}

	/// Closing the client's sending side is the unsubscribe, and the device learns of it by reading
	/// end of stream (MSG, "Subscribing"). The half-close leaves the device's write side open, so
	/// nothing but reading tells it: a device watching only for a failed write would go on sending to
	/// a client that has said it is done.
	#[tokio::test]
	async fn closing_the_sending_side_unsubscribes() {
		let mut streams = paired(&secret(0x42)).await;
		let mut subscription = streams.open().await.unwrap();
		write_message(
			&mut subscription,
			&ClientMessage::Subscribe {
				topic: SYSTEM_TOPIC.to_owned(),
			}
			.to_json(),
		)
		.await
		.unwrap();

		// Served, so the subscription is live before it is ended.
		tokio::time::timeout(Duration::from_secs(2), read_message(&mut subscription))
			.await
			.expect("a served topic answers")
			.unwrap()
			.unwrap();

		// The unsubscribe: end the sending side only, and carry on reading.
		subscription.close().await.unwrap();

		// The device stops and closes its side in turn. Data already in flight may still arrive, which
		// a client discards rather than treating as a fault, so a few are allowed through before the
		// end; a device that never stopped would keep them coming one a second forever.
		let mut after = 0;
		loop {
			match tokio::time::timeout(Duration::from_secs(5), read_message(&mut subscription))
				.await
			{
				Ok(Ok(None)) | Ok(Err(_)) => break,
				Ok(Ok(Some(_))) => {
					after += 1;
					assert!(
						after < 4,
						"the device kept sending after the client unsubscribed"
					);
				}
				Err(_) => panic!("the device neither closed nor sent after the unsubscribe"),
			}
		}
	}

	#[tokio::test]
	async fn a_client_with_the_wrong_code_cannot_open_a_session() {
		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let device = tokio::spawn(async move {
			run(device_side.compat(), &secret(0x01), Sampler::start()).await
		});

		// A client holding a different QR code fails the handshake, in both directions.
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
}
