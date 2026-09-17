//! One authenticated session: the handshake, the streams above it, and the messages they carry.
//!
//! This is written against any byte transport rather than against GATT, so the whole session can be
//! exercised over an in-memory duplex with no adapter involved, and so the same code serves a
//! different transport later without changing.
//!
//! The device opens a reporting stream as soon as the handshake completes, naming itself and then
//! reporting what it is, and serves whatever streams a client opens (BLI-MSG). It never answers a
//! message on the wire: one it does not recognise is passed over, one carrying something critical it
//! does not know is refused, and one that breaks the protocol closes the stream it arrived on.

use std::time::Duration;

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::{ClientMessage, DeviceMessage},
		stream::{Mode, Streams, accept_responder, multiplex, read_message, write_message},
	},
	key_schedule::StickerSecret,
};
use futures::{AsyncRead, AsyncWrite};

use crate::{facts::Facts, sampler::Sampler};

/// How often to look for a change in what the device reports about itself.
const IDENTITY_POLL: Duration = Duration::from_secs(2);

/// The topic carrying the device's live readings (BLI-SYS).
const SYSTEM_TOPIC: &str = "system";

/// What the device calls itself to a client, and the version it is at.
///
/// Both are opaque to the client, which displays them and never acts on them (BLI-MSG). They are the
/// package's own name and version, so a device reports what was actually built and installed.
const DEVICE_NAME: &str = env!("CARGO_PKG_NAME");
const DEVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Run a session to completion over a transport, as the device.
///
/// Returns once the client goes away or the channel fails. A failed handshake is an ordinary outcome
/// rather than an error worth stopping the daemon for: anyone in range can connect and try, and the
/// device stays reachable by a legitimate operator afterwards (BLI-ADV, "Advertising continuously").
pub async fn run<S>(
	transport: S,
	secret: &StickerSecret,
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
	let driving = tokio::spawn(async move {
		if let Err(err) = driver.await {
			tracing::debug!(%err, "connection closed");
		}
	});

	// Holds sampling open for as long as this session lasts, so a device that had gone quiet starts
	// filling its window again the moment somebody connects (BLI-SYS).
	let _session = sampler.session();

	let result = converse(&mut streams, &sampler).await;
	driving.abort();
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
/// empty while an operator waits (BLI-SYS).
async fn serve_system<S>(stream: &mut S, sampler: &Sampler) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let history = DeviceMessage::SystemHistory {
		samples: sampler.window(),
	};
	write_message(stream, &history.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	let mut live = sampler.live();
	loop {
		match live.recv().await {
			Ok(sample) => {
				let message = DeviceMessage::SystemSample {
					at: sample.at,
					readings: sample.readings,
				};
				// A write failing is the client having gone away, which is the unsubscribe.
				if write_message(stream, &message.to_json()).await.is_err() {
					return Ok(());
				}
			}
			// A client too slow to keep up misses samples rather than stalling the sampler. The next
			// one it receives is current, which is what a live view wants.
			Err(tokio::sync::broadcast::error::RecvError::Lagged(missed)) => {
				tracing::debug!(missed, "subscriber fell behind");
			}
			Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
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
			// hangs its topic on (BLI-MSG, "Subscribing").
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
				// branches on it (BLI-MSG).
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Ok(Reading::Message(ClientMessage::Subscribe { topic })) if topic == SYSTEM_TOPIC => {
				tracing::info!(%topic, "serving a subscription");
				return serve_system(stream, sampler).await;
			}
			Ok(Reading::Message(ClientMessage::Subscribe { topic })) => {
				// A topic this device does not recognise is skipped as anything else is: it sends
				// nothing and leaves the stream open until the client closes it. That is what a device
				// older than its client looks like, and it fails nothing (BLI-MSG).
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
	/// The handshake did not complete: a client that has not scanned this device's sticker, or noise
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

	fn secret(byte: u8) -> StickerSecret {
		StickerSecret::from_bytes([byte; 32])
	}

	/// Open a client against a device session over an in-memory duplex, with no BLE involved.
	async fn paired(psk: &StickerSecret) -> Streams {
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

	/// The device names itself first and reports second, both without being asked (BLI-MSG).
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
	/// stays open and the session carries on (BLI-MSG).
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
	/// into the fault path (BLI-MSG).
	///
	/// The vehicle is a message type this device does not know, marked critical by an upper case
	/// `TYPE`. It has to be: every type BLI-MSG defines forbids a critical member, so a refusal on one
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
	/// (BLI-MSG, "What the base protocol guarantees").
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
			Reading::Message(DeviceMessage::SystemHistory { .. })
		));
	}

	/// A topic this device does not serve is skipped like anything else it does not recognise: it
	/// sends nothing and leaves the stream open for the client to close. That is what a device older
	/// than its client looks like, and it fails nothing (BLI-MSG).
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
	/// than filling from empty while an operator waits (BLI-SYS).
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

		let raw = tokio::time::timeout(Duration::from_secs(2), read_message(&mut subscription))
			.await
			.expect("the window arrives")
			.unwrap()
			.unwrap();
		let Reading::Message(DeviceMessage::SystemHistory { samples }) =
			read::<DeviceMessage>(&raw).unwrap()
		else {
			panic!("the first message on a subscription is the window");
		};
		// It may be empty on a device that has only just started, which is a valid window.
		for pair in samples.windows(2) {
			assert!(pair[0].at <= pair[1].at, "the window is oldest first");
		}
	}

	#[tokio::test]
	async fn a_client_with_the_wrong_sticker_cannot_open_a_session() {
		let (client_side, device_side) = tokio::io::duplex(1 << 16);
		let device = tokio::spawn(async move {
			run(device_side.compat(), &secret(0x01), Sampler::start()).await
		});

		// A client holding a different sticker fails the handshake, in both directions.
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
