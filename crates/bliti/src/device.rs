//! The daemon: advertising, the GATT server, and the sessions they let a client open.
//!
//! This and provisioning mode are the only modules that talk to BlueZ. Behaviour is specified in
//! ADV and CHN.

use std::{path::Path, sync::Arc};

use anyhow::{Context, Result};
use bliti_core::{
	SERVICE_UUID,
	advertisement::{Advertised, Heard},
	key_schedule::Handle,
	qr::QrPayload,
	slot,
};
use futures::StreamExt;

use self::{
	server::{
		ADVERTISE_RETRY, ADVERTISE_RETRY_MAX, ADVERTISE_SETTLE, Events, advertisement, application,
		end_connections, serve_writes, shutdown, stop,
	},
	sessions::{Link, serve_sessions},
	slots::Slots,
};
use crate::{
	NetworkBackend,
	facts::{Supply, case::Case, curve},
	gatt::InboundSink,
	identity,
	network::{
		session::{Configurator, Inert, Store},
		stack::{Chosen, Stack},
		wired,
	},
	power::{Controller, systemd::Systemd},
	session::{self, AbortOnDrop},
	tailscale::{self, Tailscale},
	tamanu::Tamanu,
};

pub mod pacing;
pub mod server;
pub mod sessions;
pub mod slots;

/// Run the daemon until interrupted.
pub async fn run(
	cache: &Path,
	network: &Path,
	backend: NetworkBackend,
	adapter_name: Option<&str>,
	tailscale: tailscale::Options,
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

	// Before the adapter is touched, so a device signed in to a tailnet it does not accept stops here
	// and does not advertise (TSC, "Signing in").
	let tailscale = Tailscale::connect(tailscale)
		.await
		.context("checking the tailnet this device is in")?;

	// The configuration the device holds goes in force before anything else (CFG). One configurator
	// serves every connection, so the configuration is the device's, whichever connection reads it.
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
	.context("putting the held network configuration in force")?;

	let session = bluer::Session::new().await?;
	let adapter = match adapter_name {
		Some(name) => session.adapter(name)?,
		None => session.default_adapter().await?,
	};
	adapter.set_powered(true).await?;
	tracing::info!(adapter = %adapter.name(), address = %adapter.address().await?, "adapter ready");
	// The same name every time: a client computes it from the QR code before it listens (ADV).
	let advertised = Advertised::new(&keys.presence_token);
	// BlueZ serves the adapter's alias as the GAP Device Name, which a connecting host keeps as the
	// device's name in place of the advertised one (ADV).
	adapter
		.set_alias(advertised.to_local_name())
		.await
		.context("naming the adapter after the local name")?;

	let controller = controller(&adapter).await;

	let sink = InboundSink::default();
	// A legacy controller stops advertising the instant a client connects and does not resume by
	// itself: the advertisement stays registered with BlueZ, so nothing reports an error, but nothing
	// goes out and the device is undiscoverable to the next client. A session fires this when it opens
	// and when it ends, and the loop re-registers the advertisement in response, which is what puts the
	// device back on the air for the next client (ADV, CHN).
	let readvertise = Arc::new(tokio::sync::Notify::new());

	// The backup supply's curves are read from the curve file the first time its gauge answers, which
	// is at once where there is one; on a machine with none, it manages nothing (CHG).
	let supply = Supply::new(curve::store::Store::new(curve::store::DEFAULT_PATH));
	// Unlike sampling, this never stops: a power cut rarely happens with anyone connected (DEV), and
	// the low-battery shutdown is watched for whether or not anyone samples (LOW).
	if let Err(err) = crate::facts::record_supply(supply.clone(), controller.clone()) {
		tracing::warn!(%err, "cannot watch the backup supply; power changes go unrecorded and no low battery is watched for");
	}
	// Watched from the start and never stopped, for the same reason: a case is seldom opened with
	// anyone connected (NFO).
	let case = Case::watch().unwrap_or_else(|err| {
		tracing::warn!(%err, "cannot watch the case switch; openings go unrecorded");
		Case::none()
	});
	// Sampling starts with the daemon rather than with the first session, so a client that connects
	// to a device that has been up a while finds a populated window (NFO).
	let sampler = crate::sampler::Sampler::start(wireless, supply.clone());
	// Looked at while the device is being talked to, which the sampler knows.
	let tamanu = Tamanu::connect(sampler.clone());
	// The command line changes the curves of a running daemon through this, since the daemon holds
	// them in memory and would otherwise overwrite what it wrote to the curve file (CRV).
	let socket = Path::new(crate::battery::socket::PATH);
	let _socket = crate::battery::socket::listen(socket, supply.clone())
		.inspect_err(|err| {
			tracing::warn!(%err, path = %socket.display(), "cannot listen for the command line; `bliti battery-curve` will change the curve file behind this daemon's back");
		})
		.ok();
	let slots = Arc::new(Slots::new(slot::COUNT));
	let events: Events = {
		let readvertise = readvertise.clone();
		Arc::new(move |_| readvertise.notify_one())
	};
	let (gatt, controls) = application(adapter.clone(), slots.clone(), events.clone());
	let _application = adapter
		.serve_gatt_application(gatt)
		.await
		.context("registering the GATT application")?;
	// Only once the service is back: a client still connected from before was told it had gone, and
	// registering it tells that client it has returned before the connection ends (CHN).
	end_connections(&adapter, "ending a connection made before this start").await?;
	let _writes =
		AbortOnDrop(tokio::spawn(serve_writes(controls.writes, sink.clone())).abort_handle());
	let _sessions = AbortOnDrop(
		tokio::spawn(serve_sessions(
			controls.subscriptions,
			Link::new(adapter.clone(), slots, sink, events),
			move |transport| {
				let keys = keys.clone();
				let sampler = sampler.clone();
				let configurator = configurator.clone();
				let controller = controller.clone();
				let supply = supply.clone();
				let tailscale = tailscale.clone();
				let tamanu = tamanu.clone();
				let case = case.clone();
				async move {
					session::run(
						transport,
						&keys,
						sampler,
						configurator,
						controller,
						supply,
						tailscale,
						tamanu,
						case,
					)
					.await
				}
			},
		))
		.abort_handle(),
	);

	// A device advertises whenever it is running, re-registering the advertisement each time a session
	// opens or ends. Anyone in range can connect and begin a handshake that will fail; that is expected,
	// and there is no lockout, because someone in range could otherwise deny an operator their own
	// device.
	let mut shutdown = std::pin::pin!(shutdown());
	let mut backoff = ADVERTISE_RETRY;
	loop {
		let registered = match adapter
			.advertise(advertisement(advertised.to_local_name()))
			.await
		{
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
						stop(&adapter, None).await;
						return Ok(());
					}
				}
				backoff = (backoff * 2).min(ADVERTISE_RETRY_MAX);
				continue;
			}
		};
		tracing::info!(local_name = %advertised.to_local_name(), "advertising");

		tokio::select! {
			// A client connected or left, so the controller has stopped advertising: drop this
			// advertisement and register it afresh, which resumes it.
			_ = readvertise.notified() => {
				tracing::info!("a session opened or ended; resuming advertising");
			}
			result = &mut shutdown => {
				result?;
				// Returning drops the GATT application, which unregisters it from BlueZ. Leaving by any
				// other route leaves it and the advertisement registered against a process that is
				// gone, and BlueZ only forgets them when it restarts.
				stop(&adapter, Some(registered)).await;
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

/// A handle rendered for a person to read in a log line.
fn hex(handle: Handle) -> String {
	handle
		.as_bytes()
		.iter()
		.map(|b| format!("{b:02x}"))
		.collect()
}

/// Scan for bliti devices and report which one the QR code in hand belongs to.
///
/// This is the client half of ADV: compute the handle from the QR code and compare it against what
/// each device advertises. It exists so discovery and matching can be exercised without
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

		// The version marker is read before comparing, so a device speaking a version this client
		// does not hold is reported as exactly that rather than as a device that simply did not match.
		match advertised.heard_by(payload) {
			Heard::Matches => {
				matched += 1;
				println!(
					"{address}  MATCHES the QR code (handle {})",
					hex(advertised.handle)
				);
			}
			Heard::Other => println!("{address}  another bliti device"),
			Heard::Unsupported(marker) => {
				println!("{address}  a bliti device at unsupported version marker {marker}")
			}
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

/// The acts this device offers, and what drops its connections before it carries one out (CTL).
///
/// A device whose systemd cannot be reached offers none, and says why.
async fn controller(adapter: &bluer::Adapter) -> Controller {
	let probed = tokio::task::spawn_blocking(Systemd::probe)
		.await
		.unwrap_or_else(|err| Err(anyhow::anyhow!("probing systemd panicked: {err}")));
	let systemd = match probed {
		Ok(systemd) => systemd,
		Err(err) => {
			tracing::warn!(
				err = format!("{err:#}"),
				"cannot reach systemd; offering no act"
			);
			return Controller::none();
		}
	};
	let acts = systemd.acts();
	tracing::info!(acts = ?acts.iter().map(|act| act.name()).collect::<Vec<_>>(), "acts offered");
	let adapter = adapter.clone();
	Controller::new(
		acts,
		Arc::new(systemd),
		Box::new(move || {
			let adapter = adapter.clone();
			Box::pin(async move {
				if let Err(err) =
					end_connections(&adapter, "ending a connection before going away").await
				{
					tracing::warn!(%err, "could not list the connections to end");
				}
			})
		}),
	)
}
