//! The client half of the channel: connect to a device over GATT and run a session against it.
//!
//! This exists so the whole of CHN can be exercised from the command line, against a real device
//! over real BLE, without a browser. The web application does the same thing through Web Bluetooth,
//! and both sit on the same [`bliti_core::channel`] stack, so what this proves the browser inherits.

use std::{
	io,
	pin::Pin,
	task::{Context, Poll},
};

use anyhow::{Context as _, Result, anyhow};
use bliti_core::{
	CHARACTERISTIC_UUID_CLIENT_TX, CHARACTERISTIC_UUID_DEVICE_TX, SERVICE_UUID,
	advertisement::Advertised,
	channel::{
		envelope::{Reading, read},
		messages::Message,
		readings::Entry,
		stream::{Mode, Streams, connect_initiator, multiplex, read_message, write_message},
	},
	key_schedule::PresenceToken,
};
use bluer::gatt::remote::Characteristic;
use futures::{
	AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt as _, SinkExt, StreamExt, channel::mpsc,
};

/// How long to wait for the host to discover what the peer offers, after the link is up.
const SERVICE_RESOLUTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// How many bytes to put in one write. Messages are framed and reassembled above this, so a
/// conservative chunk costs only extra writes and works against any negotiated attribute size.
const WRITE_CHUNK: usize = 20;

/// A byte stream over a device's two characteristics: notifications in, writes out.
///
/// The mirror image of the device's own transport, and the same shape as far as everything above is
/// concerned.
struct GattClientTransport {
	inbound: mpsc::Receiver<Vec<u8>>,
	outbound: mpsc::Sender<Vec<u8>>,
	pending: Vec<u8>,
	consumed: usize,
}

impl AsyncRead for GattClientTransport {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		loop {
			if this.consumed < this.pending.len() {
				let available = &this.pending[this.consumed..];
				let n = available.len().min(buf.len());
				buf[..n].copy_from_slice(&available[..n]);
				this.consumed += n;
				if this.consumed == this.pending.len() {
					this.pending.clear();
					this.consumed = 0;
				}
				return Poll::Ready(Ok(n));
			}
			match this.inbound.poll_next_unpin(cx) {
				Poll::Ready(Some(chunk)) => {
					this.pending = chunk;
					this.consumed = 0;
				}
				Poll::Ready(None) => return Poll::Ready(Ok(0)),
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

impl AsyncWrite for GattClientTransport {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		if buf.is_empty() {
			return Poll::Ready(Ok(0));
		}
		match this.outbound.poll_ready(cx) {
			Poll::Ready(Ok(())) => {}
			Poll::Ready(Err(_)) => return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into())),
			Poll::Pending => return Poll::Pending,
		}
		let chunk = &buf[..buf.len().min(WRITE_CHUNK)];
		this.outbound
			.start_send(chunk.to_vec())
			.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))?;
		Poll::Ready(Ok(chunk.len()))
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		self.get_mut()
			.outbound
			.poll_flush_unpin(cx)
			.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
	}

	fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		self.get_mut()
			.outbound
			.poll_close_unpin(cx)
			.map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
	}
}

/// Find the bliti service's two characteristics on a connected device.
async fn characteristics(device: &bluer::Device) -> Result<(Characteristic, Characteristic)> {
	for service in device.services().await? {
		if service.uuid().await? != SERVICE_UUID {
			continue;
		}
		let (mut to_device, mut from_device) = (None, None);
		for characteristic in service.characteristics().await? {
			match characteristic.uuid().await? {
				u if u == CHARACTERISTIC_UUID_CLIENT_TX => to_device = Some(characteristic),
				u if u == CHARACTERISTIC_UUID_DEVICE_TX => from_device = Some(characteristic),
				_ => {}
			}
		}
		if let (Some(to_device), Some(from_device)) = (to_device, from_device) {
			return Ok((to_device, from_device));
		}
	}
	Err(anyhow!("the device does not carry the bliti service"))
}

/// Find the device a QR code belongs to, by the matching of ADV.
///
/// A client cannot reach a device it has not heard: a peer has to be discovered before it can be
/// connected to. So finding it is part of connecting, and this is the same scan-then-match the web
/// application performs before it opens a channel.
async fn find(
	adapter: &bluer::Adapter,
	secret: &PresenceToken,
	seconds: u64,
) -> Result<bluer::Address> {
	// Discovery has to be running for names to be refreshed, but the events it emits are not enough
	// on their own: a device the host already knows is announced once, carrying whatever name it was
	// last seen with, which after a salt roll is a handle that no longer matches. So the names of
	// every device known are re-read while the scan runs, rather than read once when it is announced.
	let _discovery = adapter.discover_devices().await?;
	let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(seconds);

	while tokio::time::Instant::now() < deadline {
		for address in adapter.device_addresses().await? {
			let device = adapter.device(address)?;
			let Some(name) = device.name().await.ok().flatten() else {
				continue;
			};
			let Some(advertised) = Advertised::from_local_name(&name) else {
				continue;
			};
			if advertised.version != bliti_core::key_schedule::VERSION {
				tracing::warn!(
					%address,
					version = advertised.version,
					"a bliti device at an unsupported version"
				);
				continue;
			}
			if advertised.matches(secret) {
				tracing::info!(%address, "matched the QR code");
				return Ok(address);
			}
		}
		tokio::time::sleep(std::time::Duration::from_secs(3)).await;
	}
	Err(anyhow!("no device matching that QR code was heard"))
}

/// An open channel to a device, its hello already sent.
struct Channel {
	// Held so the connection to BlueZ outlives every use of the device.
	_session: bluer::Session,
	device: bluer::Device,
	streams: Streams,
}

/// Connect to a device, run the handshake, and name this client on its own hello stream.
async fn open(
	address: Option<bluer::Address>,
	secret: &PresenceToken,
	adapter_name: Option<&str>,
) -> Result<Channel> {
	let session = bluer::Session::new().await?;
	let adapter = match adapter_name {
		Some(name) => session.adapter(name)?,
		None => session.default_adapter().await?,
	};
	adapter.set_powered(true).await?;

	// A host keeps what it learned about a peer, and what it kept can be stale: a device it has
	// connected to before may never resolve its services again. Forgetting it first costs one
	// discovery and makes a connection attempt behave the same every time.
	let mut device = None;
	for attempt in 0..2 {
		let found = match address {
			Some(address) => address,
			None => find(&adapter, secret, 20).await?,
		};
		let candidate = adapter.device(found)?;
		if !candidate.is_connected().await? {
			candidate
				.connect()
				.await
				.context("connecting to the device")?;
		}
		tracing::info!(address = %found, "connected");

		// Connecting is not the same as knowing what the peer offers: the host discovers the peer's
		// attributes after the link is up, and until it has there are no services to look through.
		let deadline = tokio::time::Instant::now() + SERVICE_RESOLUTION_TIMEOUT;
		let mut resolved = false;
		while tokio::time::Instant::now() < deadline {
			if candidate.is_services_resolved().await? {
				resolved = true;
				break;
			}
			tokio::time::sleep(std::time::Duration::from_millis(200)).await;
		}

		if resolved {
			device = Some(candidate);
			break;
		}
		if attempt == 0 {
			tracing::warn!("services did not resolve; forgetting the device and trying once more");
			let _ = candidate.disconnect().await;
			let _ = adapter.remove_device(found).await;
			tokio::time::sleep(std::time::Duration::from_secs(1)).await;
		}
	}
	let device = device.ok_or_else(|| anyhow!("the device's services were never resolved"))?;

	let (to_device, from_device) = characteristics(&device).await?;
	let notifications = from_device.notify().await?;

	// Pump notifications in and writes out, so the transport sees an ordinary byte stream.
	let (mut inbound_tx, inbound_rx) = mpsc::channel(64);
	let (outbound_tx, mut outbound_rx) = mpsc::channel::<Vec<u8>>(64);
	tokio::spawn(async move {
		let mut notifications = std::pin::pin!(notifications);
		while let Some(chunk) = notifications.next().await {
			if inbound_tx.try_send(chunk).is_err() {
				break;
			}
		}
	});
	tokio::spawn(async move {
		while let Some(chunk) = outbound_rx.next().await {
			if to_device.write(&chunk).await.is_err() {
				break;
			}
		}
	});

	let transport = GattClientTransport {
		inbound: inbound_rx,
		outbound: outbound_tx,
		pending: Vec::new(),
		consumed: 0,
	};

	// Everything from here is the same stack the browser will run.
	let encrypted = connect_initiator(transport, secret)
		.await
		.map_err(|err| anyhow!("handshake failed: {err}"))?;
	tracing::info!("handshake complete");

	let (mut streams, driver) = multiplex(encrypted, Mode::Client);
	tokio::spawn(async move {
		let _ = driver.await;
	});

	// The client names itself on its own hello stream, without waiting to be asked and without waiting
	// for the device's hello. The device logs it and never acts on it (MSG).
	let mut hello_stream = streams.open().await?;
	let hello = Message::Hello {
		name: env!("CARGO_PKG_NAME").to_owned(),
		version: env!("BLITI_VERSION").to_owned(),
	};
	write_message(&mut hello_stream, &hello.to_json()).await?;

	Ok(Channel {
		_session: session,
		device,
		streams,
	})
}

/// Connect to a device, run the handshake, and exchange the milestone's two messages.
pub async fn connect(
	address: Option<bluer::Address>,
	secret: &PresenceToken,
	adapter_name: Option<&str>,
) -> Result<()> {
	let Channel {
		_session,
		device,
		mut streams,
	} = open(address, secret, adapter_name).await?;

	// The device pushes its own hello and the default feed unprompted, each on its own stream (MSG).
	// Read whatever it pushes for a few seconds and print each message from its own description.
	let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
	while std::time::Instant::now() < deadline {
		let Ok(Some(mut stream)) =
			tokio::time::timeout(std::time::Duration::from_secs(1), streams.accept()).await
		else {
			continue;
		};
		tokio::spawn(async move {
			while let Ok(Some(raw)) = read_message(&mut stream).await {
				match read::<Message>(&raw) {
					Ok(Reading::Message(Message::Hello { name, version })) => {
						println!("device:   {name} {version}");
					}
					// Rendered from what each entry says about itself, with no list of names to match
					// against: a device that has gained an entry shows it here without this client
					// changing (NFO).
					Ok(Reading::Message(Message::Fact(entry) | Message::Reading(entry))) => {
						println!("{}", render(&entry));
					}
					Ok(Reading::Message(Message::Subscribe { topic })) => {
						println!("subscribe: {topic}");
					}
					// The configuration session of CFG. This diagnostic client opens none, so these are
					// noted but not acted on.
					Ok(Reading::Message(
						Message::Configure
						| Message::Configuration { .. }
						| Message::Applied { .. }
						| Message::State { .. }
						| Message::Pin { .. }
						| Message::Invalid { .. }
						| Message::Confirm
						| Message::Discard
						| Message::Busy
						| Message::Scan { .. }
						| Message::Survey { .. }
						| Message::Wps { .. }
						| Message::Networks { .. }
						| Message::Spectrum { .. },
					)) => {
						println!("config:   a configuration-session message");
					}
					// A device newer than this build: passed over, or not acted on, but never fatal.
					Ok(Reading::Skipped(skip)) => println!("skipped:  {skip}"),
					Ok(Reading::Refused(refusal)) => println!("refused:  {refusal}"),
					// A device not speaking the protocol. The stream goes; the connection does not.
					Err(fault) => {
						println!("fault:    {fault}");
						break;
					}
				}
			}
		});
	}

	let _ = device.disconnect().await;
	Ok(())
}

/// Open a configuration session on a device (CFG) and relay it.
///
/// Each line of standard input is sent on the session stream as one message, verbatim, and each
/// message the device sends there is printed as it arrived, one per line. The session ends when
/// standard input does, which leaves the recorded configuration in force unless a `confirm` was sent.
/// Everything the device pushes on streams of its own is read and dropped.
pub async fn configure(
	address: Option<bluer::Address>,
	secret: &PresenceToken,
	adapter_name: Option<&str>,
) -> Result<()> {
	let Channel {
		_session,
		device,
		mut streams,
	} = open(address, secret, adapter_name).await?;

	let (mut from_device, mut to_device) = AsyncReadExt::split(streams.open().await?);
	write_message(&mut to_device, &Message::Configure.to_json()).await?;

	tokio::spawn(async move {
		while let Some(mut pushed) = streams.accept().await {
			tokio::spawn(async move { while let Ok(Some(_)) = read_message(&mut pushed).await {} });
		}
	});
	let printer = tokio::spawn(async move {
		while let Ok(Some(raw)) = read_message(&mut from_device).await {
			println!("{}", String::from_utf8_lossy(&raw));
		}
	});

	let (lines_tx, mut lines) = mpsc::channel::<String>(8);
	std::thread::spawn(move || {
		let mut lines_tx = lines_tx;
		for line in io::stdin().lines().map_while(Result::ok) {
			if futures::executor::block_on(lines_tx.send(line)).is_err() {
				break;
			}
		}
	});
	while let Some(line) = lines.next().await {
		let line = line.trim();
		if !line.is_empty() {
			write_message(&mut to_device, line.as_bytes()).await?;
		}
	}

	to_device.close().await?;
	printer.abort();
	let _ = device.disconnect().await;
	Ok(())
}

/// One fact or reading, rendered from its own description.
///
/// Nothing here matches on an entry's name: a client that did could only show what it already knew
/// about, which is the property NFO exists to avoid.
fn render(entry: &Entry) -> String {
	let mut line = format!("{}: ", entry.name);
	match &entry.value {
		Some(value) => line.push_str(&show(value, &entry.kind, entry.unit.as_deref())),
		None => match entry.reason() {
			Some(why) => line.push_str(&format!(
				"{} ({why})",
				entry.status().unwrap_or("unavailable")
			)),
			None => line.push_str(entry.status().unwrap_or("unavailable")),
		},
	}
	if matches!(entry.status(), Some("warning" | "failed" | "broken")) {
		line.push_str("  [!]");
	}
	line
}

/// A value, drawn by its kind, with the unit the entry named for itself.
fn show(value: &serde_json::Value, kind: &str, unit: Option<&str>) -> String {
	use bliti_core::channel::readings::kind as k;
	match kind {
		k::FRACTION => value.as_f64().map_or_else(
			|| value.to_string(),
			|number| format!("{:.0}%", number * 100.0),
		),
		k::DURATION => value.as_f64().map_or_else(
			|| value.to_string(),
			|seconds| {
				let seconds = seconds as u64;
				format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60)
			},
		),
		k::TEXT | k::DATETIME | k::IPV4 | k::IPV6 => value
			.as_str()
			.map_or_else(|| value.to_string(), ToOwned::to_owned),
		// A quantity, or a kind this build does not know: the value stringified, with the unit where
		// there is one (VIEW).
		_ => {
			let number = value
				.as_f64()
				.map_or_else(|| value.to_string(), |n| n.to_string());
			match unit {
				Some(unit) => format!("{number} {unit}"),
				None => number,
			}
		}
	}
}
