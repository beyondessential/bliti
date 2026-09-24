//! The daemon: advertising, the GATT server, and the sessions they let a client open.
//!
//! This is the only module that talks to BlueZ. Behaviour is specified in ADV and CHN.

use std::{
	path::Path,
	sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use bliti_core::{
	CHARACTERISTIC_UUID_CLIENT_TX, CHARACTERISTIC_UUID_DEVICE_TX, SERVICE_UUID,
	advertisement::Advertised,
	key_schedule::{DeviceKeys, Handle, RotationSalt},
	qr::QrPayload,
};
use bluer::{
	adv::Advertisement,
	gatt::{
		CharacteristicWriter,
		local::{
			Application, Characteristic, CharacteristicControl, CharacteristicControlEvent,
			CharacteristicControlHandle, CharacteristicNotify, CharacteristicNotifyMethod,
			CharacteristicWrite, CharacteristicWriteMethod, Service, characteristic_control,
		},
	},
};
use futures::StreamExt;
use tracing::Instrument;

use crate::{
	NetworkBackend, SALT_ROTATION,
	gatt::{GattTransport, InboundSink},
	identity,
	network::{
		session::{Configurator, Inert, Store},
		stack::{Chosen, Stack},
		wired,
	},
	session::{self, AbortOnDrop},
};

/// Run the daemon until interrupted.
pub async fn run(
	cache: &Path,
	network: &Path,
	backend: NetworkBackend,
	adapter_name: Option<&str>,
) -> Result<()> {
	// Establish identity before touching Bluetooth: a board whose QR code is dead, or that this build
	// cannot derive for, must say so rather than advertise a handle nobody can match.
	let identity = identity::establish(cache).context("establishing this board's identity")?;
	if identity.derived {
		tracing::info!(source = %identity.kind, "derived this board's root");
	} else {
		tracing::info!(source = %identity.kind, "root is cached");
	}
	let keys = Arc::new(identity.keys);

	// The recorded network configuration goes in force before anything else, since nothing provisional
	// survives a restart (CFG). One configurator serves every connection, so at most one configuration
	// session is open device-wide.
	let sys_class_net = Path::new(wired::SYS_CLASS_NET);
	let backend = match backend {
		NetworkBackend::Inert => Chosen::Inert(Inert),
		NetworkBackend::Stack => Chosen::Stack(Box::new(
			Stack::linux(wired::interfaces(sys_class_net))
				.await
				.context("starting the network backend")?,
		)),
	};
	let wireless = backend.report();
	let configurator = Configurator::start(
		backend,
		Store::new(network),
		wired::unconfigured(sys_class_net),
	)
	.await
	.context("putting the recorded network configuration in force")?;

	let session = bluer::Session::new().await?;
	let adapter = match adapter_name {
		Some(name) => session.adapter(name)?,
		None => session.default_adapter().await?,
	};
	adapter.set_powered(true).await?;
	tracing::info!(adapter = %adapter.name(), address = %adapter.address().await?, "adapter ready");
	end_earlier_connections(&adapter).await?;

	let sink = InboundSink::default();
	// A legacy controller stops advertising the instant a client connects and does not resume by
	// itself: the advertisement stays registered with BlueZ, so nothing reports an error, but nothing
	// goes out and the device is undiscoverable to the next client. A session fires this when it opens
	// and when it ends, and the loop re-registers the advertisement in response, which is what puts the
	// device back on the air for the next client (ADV, CHN).
	let readvertise = Arc::new(tokio::sync::Notify::new());

	// Sampling starts with the daemon rather than with the first session, so a client that connects
	// to a device that has been up a while finds a populated window (NFO).
	let sampler = crate::sampler::Sampler::start(wireless);
	let (writes, writes_handle) = characteristic_control();
	let (subscriptions, subscriptions_handle) = characteristic_control();
	let _application = adapter
		.serve_gatt_application(application(writes_handle, subscriptions_handle))
		.await
		.context("registering the GATT application")?;
	let _writes = AbortOnDrop(tokio::spawn(serve_writes(writes, sink.clone())).abort_handle());
	let _sessions = AbortOnDrop(
		tokio::spawn(serve_sessions(
			subscriptions,
			sink,
			keys.clone(),
			readvertise.clone(),
			sampler,
			configurator,
		))
		.abort_handle(),
	);

	// A device advertises whenever it is running, re-registering the advertisement each time the salt
	// rolls and each time a session ends. Anyone in range can connect and begin a handshake that will
	// fail; that is expected, and there is no lockout, because someone in range could otherwise deny an
	// operator their own device.
	let mut rotation = tokio::time::interval(SALT_ROTATION);
	// An interval yields its first tick immediately; take it here so the first salt lasts a full
	// period rather than being replaced the instant it is advertised.
	rotation.tick().await;
	let mut shutdown = std::pin::pin!(shutdown());
	let mut backoff = ADVERTISE_RETRY;
	loop {
		let salt = random_salt();
		let advertised = Advertised::new(keys.presence_token.handle(salt), salt);
		let registered = match adapter.advertise(advertisement(advertised)).await {
			Ok(registered) => {
				backoff = ADVERTISE_RETRY;
				registered
			}
			// Registration failing is not worth exiting for. A device that has stopped advertising
			// cannot be reached at all, and an operator standing in front of one cannot tell it from a
			// dead one. Worse, exiting leaves the advertisement registered against a process that is
			// gone, and BlueZ holds it until it restarts: each exit costs one of the controller's few
			// advertising slots and makes the next registration likelier to fail the same way.
			Err(err) => {
				tracing::error!(%err, ?backoff, "could not register the advertisement; retrying");
				tokio::select! {
					_ = tokio::time::sleep(backoff) => {}
					result = &mut shutdown => {
						result?;
						tracing::info!("stopping");
						return Ok(());
					}
				}
				backoff = (backoff * 2).min(ADVERTISE_RETRY_MAX);
				continue;
			}
		};
		tracing::info!(local_name = %advertised.to_local_name(), "advertising");

		tokio::select! {
			_ = rotation.tick() => {}
			// A client connected or left, so the controller has stopped advertising: drop this
			// advertisement and register a fresh one, which resumes it. A fresh salt comes with it,
			// which is harmless.
			_ = readvertise.notified() => {
				tracing::info!("a session opened or ended; resuming advertising");
			}
			result = &mut shutdown => {
				result?;
				// Returning drops the advertisement and the GATT application, which unregisters both
				// from BlueZ. Leaving by any other route leaves them registered against a process that
				// is gone, and BlueZ only forgets them when it restarts.
				tracing::info!("stopping");
				return Ok(());
			}
		}

		// Unregistering reaches BlueZ asynchronously, and the controller has only a few advertising
		// slots. Registering the next advertisement while this one is still being withdrawn is what
		// exhausts them, and the registration then times out on D-Bus.
		drop(registered);
		tokio::time::sleep(ADVERTISE_SETTLE).await;
	}
}

/// Resolve when the daemon is asked to stop, by either of the signals that mean it.
///
/// systemd stops a unit with `SIGTERM`, so waiting on ctrl-c alone would mean the ordinary way of
/// stopping the daemon is the one way that skips unregistering from BlueZ.
async fn shutdown() -> Result<()> {
	use tokio::signal::unix::{SignalKind, signal};
	let mut terminate = signal(SignalKind::terminate())?;
	let mut interrupt = signal(SignalKind::interrupt())?;
	tokio::select! {
		_ = terminate.recv() => {}
		_ = interrupt.recv() => {}
	}
	Ok(())
}

/// How long to leave BlueZ to withdraw an advertisement before registering the next.
const ADVERTISE_SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

/// How long to wait before trying a failed advertisement registration again, and the ceiling that
/// wait backs off to.
const ADVERTISE_RETRY: std::time::Duration = std::time::Duration::from_secs(1);
const ADVERTISE_RETRY_MAX: std::time::Duration = std::time::Duration::from_secs(30);

/// What a device may put on the air in any one second, across every session (CHN, "Send rate").
///
/// Payload bytes rather than a count of notifications: a peer may coalesce several of them into one
/// ATT protocol data unit, so the same count occupies the link for very different lengths of time
/// depending on how large each one is and on whether the peer coalesces at all, neither of which the
/// device is told. A device with a backlog takes longer to clear it rather than taking the connection
/// down, which is the outcome worth having: a slow reading beats a dropped session.
const NOTIFY_BYTES_A_SECOND: usize = 40 * 1024;

/// How far ahead of the pacing schedule a send may run before it waits.
///
/// One small notification is worth a few hundred microseconds of the allowance, and sleeping for that
/// rounds up to the timer's granularity and throttles far below the ceiling. Letting sends bunch this
/// far ahead and then waiting off the whole debt at once keeps the long-run rate exact. It costs a
/// burst of `NOTIFY_BYTES_A_SECOND` times this, a couple of hundred bytes.
const NOTIFY_PACING_SLACK: std::time::Duration = std::time::Duration::from_millis(5);

/// How much of the second's allowance a payload of this size spends.
fn notify_pacing(len: usize) -> std::time::Duration {
	std::time::Duration::from_nanos((len as u64 * 1_000_000_000) / NOTIFY_BYTES_A_SECOND as u64)
}

/// Holds the device to the send rate of CHN, by payload rather than by counting notifications.
///
/// Paced rather than windowed: clearing a counter each second lets one second's allowance land at its
/// end and the next at its start, putting twice the ceiling on the air across the boundary, which is
/// the region the ceiling exists to stay out of.
struct Pacer {
	next_send: std::time::Instant,
}

impl Pacer {
	fn new(now: std::time::Instant) -> Self {
		Self { next_send: now }
	}

	/// How long to hold `len` payload bytes back before putting them on the air, if at all.
	fn wait_for(&mut self, now: std::time::Instant, len: usize) -> Option<std::time::Duration> {
		let wait = match self.next_send.checked_duration_since(now) {
			Some(wait) if wait >= NOTIFY_PACING_SLACK => Some(wait),
			Some(_) => None,
			None => {
				// A quiet stretch is not credit towards a later burst.
				self.next_send = now;
				None
			}
		};
		self.next_send += notify_pacing(len);
		wait
	}
}

/// A handle rendered for a person to read in a log line.
fn hex(handle: Handle) -> String {
	handle
		.as_bytes()
		.iter()
		.map(|b| format!("{b:02x}"))
		.collect()
}

/// A fresh rotation salt. Advertised in the clear; what it buys is that a passive observer cannot
/// follow a device by its handle across a change.
fn random_salt() -> RotationSalt {
	RotationSalt::from_bytes(rand::random())
}

/// The advertisement a device registers.
///
/// The service UUID goes in the advertisement, because filtering a scan by service UUID is the only
/// filtering some client platforms offer and it is applied to the advertisement. The handle, salt and
/// version ride in the local name, which is the one element a host will place in the scan response,
/// and so the only way the whole thing fits a controller that does only legacy advertising.
fn advertisement(advertised: Advertised) -> Advertisement {
	Advertisement {
		advertisement_type: bluer::adv::Type::Peripheral,
		service_uuids: [SERVICE_UUID].into_iter().collect(),
		local_name: Some(advertised.to_local_name()),
		discoverable: Some(true),
		// A short interval so a scanning client finds the device quickly and reliably. The default is
		// over a second, which on a legacy controller leaves a client's scan window catching an
		// advertisement only now and then, so the first attempt after a device comes on the air often
		// hears nothing. These are hints the controller rounds to what it supports.
		min_interval: Some(std::time::Duration::from_millis(100)),
		max_interval: Some(std::time::Duration::from_millis(150)),
		..Default::default()
	}
}

/// The GATT application: one service with the characteristic a client writes and the one the device
/// notifies on.
///
/// Both run over sockets BlueZ acquires for each client: one carrying that client's writes in the
/// order it made them, the other what the device notifies to that client alone. Each arrives on the
/// characteristic's control, `writes` and `subscriptions` (CHN, "Several clients at once").
fn application(
	writes: CharacteristicControlHandle,
	subscriptions: CharacteristicControlHandle,
) -> Application {
	Application {
		services: vec![Service {
			uuid: SERVICE_UUID,
			primary: true,
			characteristics: vec![
				Characteristic {
					uuid: CHARACTERISTIC_UUID_CLIENT_TX,
					write: Some(CharacteristicWrite {
						write: true,
						write_without_response: true,
						method: CharacteristicWriteMethod::Io,
						..Default::default()
					}),
					control_handle: writes,
					..Default::default()
				},
				Characteristic {
					uuid: CHARACTERISTIC_UUID_DEVICE_TX,
					notify: Some(CharacteristicNotify {
						notify: true,
						method: CharacteristicNotifyMethod::Io,
						..Default::default()
					}),
					control_handle: subscriptions,
					..Default::default()
				},
			],
			..Default::default()
		}],
		..Default::default()
	}
}

/// Hand each client's writes to its session, in the order the client made them.
///
/// One socket per client, read by one task, is what keeps them in order: handled one call at a time,
/// writes a client makes without waiting for a response can be taken up out of order.
async fn serve_writes(mut writes: CharacteristicControl, sink: InboundSink) {
	while let Some(event) = writes.next().await {
		let CharacteristicControlEvent::Write(request) = event else {
			continue;
		};
		let client = request.device_address();
		let reader = match request.accept() {
			Ok(reader) => reader,
			Err(err) => {
				tracing::warn!(%client, %err, "could not take a client's writes");
				continue;
			}
		};
		let sink = sink.clone();
		tokio::spawn(async move {
			// An empty read is the socket closing, as the client disconnects.
			while let Ok(bytes) = reader.recv().await {
				if bytes.is_empty() {
					break;
				}
				sink.deliver(client, bytes);
			}
		});
	}
	tracing::error!("BlueZ stopped reporting writes; clients can no longer reach a session");
}

/// Open a session for each client that subscribes, and run them side by side.
async fn serve_sessions(
	mut subscriptions: CharacteristicControl,
	sink: InboundSink,
	keys: Arc<DeviceKeys>,
	readvertise: Arc<tokio::sync::Notify>,
	sampler: crate::sampler::Sampler,
	configurator: Configurator<Chosen>,
) {
	// One allowance for the whole device, since every session shares the one radio.
	let pacer = Arc::new(Mutex::new(Pacer::new(std::time::Instant::now())));
	while let Some(event) = subscriptions.next().await {
		let CharacteristicControlEvent::Notify(notifier) = event else {
			continue;
		};
		// Every line a session logs names its client, so sessions running side by side read apart.
		let span = tracing::info_span!("session", client = %notifier.device_address());
		tokio::spawn(
			serve_session(
				notifier,
				sink.clone(),
				keys.clone(),
				readvertise.clone(),
				sampler.clone(),
				configurator.clone(),
				pacer.clone(),
			)
			.instrument(span),
		);
	}
	tracing::error!("BlueZ stopped reporting subscriptions; no further sessions can open");
}

/// Serve one client's session, from its subscribing to its leaving or the session ending.
///
/// `readvertise` is fired as the session opens and as it ends, so the daemon can resume advertising:
/// a client connecting stops the controller advertising, and only re-registering the advertisement
/// brings it back.
async fn serve_session(
	notifier: CharacteristicWriter,
	sink: InboundSink,
	keys: Arc<DeviceKeys>,
	readvertise: Arc<tokio::sync::Notify>,
	sampler: crate::sampler::Sampler,
	configurator: Configurator<Chosen>,
	pacer: Arc<Mutex<Pacer>>,
) {
	// A client subscribing is what opens a session: it is the point at which the device can send, so
	// it is the point at which a handshake can run.
	let client = notifier.device_address();
	tracing::info!("client subscribed; opening a session");
	readvertise.notify_one();
	let (transport, mut outbound) = GattTransport::open(&sink, client);

	// Pump the device's bytes out as notifications for as long as the client is subscribed, and notice
	// when it stops being subscribed.
	//
	// Noticing matters: nothing else tells the device the client has gone. The session reads until its
	// transport ends, and the transport only ends when the session drops it, so without this the two
	// wait on each other and the device stays busy with a client that left.
	let (left, gone) = tokio::sync::oneshot::channel();
	let pump = tokio::spawn(async move {
		loop {
			tokio::select! {
				chunk = outbound.next() => {
					let Some(chunk) = chunk else { break };

					// Out of allowance: hold the rest back rather than pushing on and swamping the
					// link. Nothing is dropped, only delayed (CHN, "Send rate").
					let wait = pacer
						.lock()
						.expect("the pacer is never held across a panic")
						.wait_for(std::time::Instant::now(), chunk.len());
					if let Some(wait) = wait {
						tokio::time::sleep(wait).await;
					}

					if notifier.send(&chunk).await.is_err() {
						break;
					}
				}
				_ = notifier.closed() => break,
			}
		}
		let _ = left.send(());
	});

	// A failed handshake is an ordinary outcome: anyone in range can connect and try, and the device
	// stays reachable afterwards.
	tokio::select! {
		result = session::run(transport, &keys, sampler, configurator) => match result {
			Ok(()) => tracing::info!("session ended"),
			Err(err) => tracing::info!(%err, "session ended"),
		},
		_ = gone => tracing::info!("client unsubscribed; session ended"),
	}
	pump.abort();
	readvertise.notify_one();
}

/// Scan for bliti devices and report which one the QR code in hand belongs to.
///
/// This is the client half of ADV: recompute the handle from the QR code against whatever salt
/// each device advertises, and compare. It exists so discovery and matching can be exercised without
/// a browser; the web application does the same thing.
pub async fn scan(payload: &QrPayload, seconds: u64, adapter_name: Option<&str>) -> Result<()> {
	let session = bluer::Session::new().await?;
	let adapter = match adapter_name {
		Some(name) => session.adapter(name)?,
		None => session.default_adapter().await?,
	};
	adapter.set_powered(true).await?;

	let mut events = adapter.discover_devices().await?;
	let deadline = tokio::time::sleep(std::time::Duration::from_secs(seconds));
	let mut deadline = std::pin::pin!(deadline);
	let mut matched = 0usize;
	let mut seen = std::collections::BTreeSet::new();

	tracing::info!(seconds, "scanning");
	loop {
		let event = tokio::select! {
			_ = &mut deadline => break,
			event = events.next() => match event {
				Some(event) => event,
				None => break,
			},
		};
		let bluer::AdapterEvent::DeviceAdded(address) = event else {
			continue;
		};
		if !seen.insert(address) {
			continue;
		}
		let device = adapter.device(address)?;
		let name = device.name().await.ok().flatten();
		let carries_bliti = device
			.uuids()
			.await
			.ok()
			.flatten()
			.is_some_and(|uuids| uuids.contains(&SERVICE_UUID));
		tracing::debug!(%address, ?name, bliti = carries_bliti, "heard");

		// A name that is not a bliti payload belongs to a device that is not one.
		let Some(advertised) = name.as_deref().and_then(Advertised::from_local_name) else {
			if carries_bliti {
				println!(
					"{address}  a bliti device whose name is not a payload ({})",
					name.unwrap_or_else(|| "-".to_owned())
				);
			}
			continue;
		};

		// The version is read before recomputing, so a device speaking a version this client does not
		// hold is reported as exactly that rather than as a device that simply did not match.
		if advertised.version != payload.version() {
			println!(
				"{address}  a bliti device at unsupported version {}",
				advertised.version
			);
			continue;
		}

		if advertised.matches(payload.presence_token()) {
			matched += 1;
			println!(
				"{address}  MATCHES the QR code (handle {})",
				hex(advertised.handle)
			);
		} else {
			println!("{address}  another bliti device");
		}
	}

	if matched == 0 {
		tracing::warn!("no device matching that QR code was heard");
	} else if matched > 1 {
		// Two devices answering one QR code is a handle collision, or a device being impersonated;
		// either way it is reported rather than silently picking one.
		tracing::warn!(matched, "more than one device matched that QR code");
	}
	Ok(())
}

/// End every connection made before this start: the channel its client held was served by an
/// earlier daemon, and the client would otherwise wait on it with nothing to tell it the channel is
/// gone (CHN).
async fn end_earlier_connections(adapter: &bluer::Adapter) -> Result<()> {
	for address in adapter.device_addresses().await? {
		let device = adapter.device(address)?;
		if !device.is_connected().await.unwrap_or(false) {
			continue;
		}
		tracing::info!(%address, "ending a connection made before this start");
		if let Err(error) = device.disconnect().await {
			tracing::warn!(%address, %error, "could not end a connection made before this start");
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use super::*;

	/// The chunk the transport actually hands the pump (`NOTIFY_CHUNK` in `gatt.rs`).
	const CHUNK: usize = 20;

	/// What the slack buys a sender on top of the second's allowance.
	fn burst_allowance() -> usize {
		(NOTIFY_BYTES_A_SECOND as f64 * NOTIFY_PACING_SLACK.as_secs_f64()) as usize + CHUNK
	}

	/// Run `chunks` payloads through the pacer, advancing a simulated clock by whatever it asks
	/// for, and return the time each one went out.
	fn schedule(chunks: usize, len: usize) -> (Instant, Vec<Instant>) {
		let start = Instant::now();
		let mut pacer = Pacer::new(start);
		let mut now = start;
		let mut sent = Vec::with_capacity(chunks);
		for _ in 0..chunks {
			if let Some(wait) = pacer.wait_for(now, len) {
				now += wait;
			}
			sent.push(now);
		}
		(start, sent)
	}

	#[test]
	fn a_seconds_allowance_takes_a_second() {
		let (start, sent) = schedule(NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let elapsed = sent.last().expect("sent something").duration_since(start);
		assert!(
			elapsed >= Duration::from_millis(980),
			"a second of payload drained in {elapsed:?}"
		);
	}

	#[test]
	fn no_one_second_window_exceeds_the_ceiling() {
		// Three seconds of continuous offering, so a window straddles every boundary.
		let (_, sent) = schedule(3 * NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let ceiling = NOTIFY_BYTES_A_SECOND + burst_allowance();
		for (i, from) in sent.iter().enumerate() {
			let in_window = sent[i..]
				.iter()
				.take_while(|at| at.duration_since(*from) < Duration::from_secs(1))
				.count() * CHUNK;
			assert!(
				in_window <= ceiling,
				"{in_window} bytes went out in one second, over the {ceiling} allowed"
			);
		}
	}

	#[test]
	fn a_quiet_stretch_is_not_saved_up() {
		let start = Instant::now();
		let mut pacer = Pacer::new(start);
		let idle = start + Duration::from_secs(5);

		// The first payload after the lull goes straight out, however long the lull was.
		assert_eq!(pacer.wait_for(idle, NOTIFY_BYTES_A_SECOND), None);

		// Having just spent the whole allowance, the next one waits about a second, rather than
		// drawing on five seconds of silence.
		let wait = pacer.wait_for(idle, CHUNK).expect("must be held back");
		assert!(
			wait >= Duration::from_millis(980),
			"held back only {wait:?}"
		);
	}

	#[test]
	fn the_ceiling_is_the_same_in_bytes_whatever_the_payload_size() {
		// A count-based ceiling would let the larger payload put far more on the air.
		let small = schedule(NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let large = schedule(NOTIFY_BYTES_A_SECOND / 500, 500);
		let small_span = small.1.last().unwrap().duration_since(small.0);
		let large_span = large.1.last().unwrap().duration_since(large.0);
		let difference = small_span.abs_diff(large_span);
		assert!(
			difference < Duration::from_millis(50),
			"same bytes took {small_span:?} at {CHUNK}B but {large_span:?} at 500B"
		);
	}
}
