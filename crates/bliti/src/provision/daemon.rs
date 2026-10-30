//! The provisioning mode daemon: the USER button, the advertising window, the GATT server, the LED
//! and the console's socket, around one [`Provisioner`](super::Provisioner) (PRV).

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use async_trait::async_trait;
use bliti_core::key_schedule::DeviceKeys;
use tokio::sync::mpsc;

use super::{
	Provisioner, button, led, name, session, socket,
	step::registry,
	store::Store,
	window::{self, Air},
};
use crate::{
	device::{
		server::{
			ADVERTISE_SETTLE, Event, Events, advertisement, application, end_connections,
			serve_writes, shutdown, stop,
		},
		sessions::{Link, serve_sessions},
		slots::Slots,
	},
	gatt::InboundSink,
	identity,
	session::AbortOnDrop,
};

/// What the provisioning daemon is told by means outside the protocol (PRV).
#[derive(Debug, Clone)]
pub struct Options {
	/// Bluetooth adapter to use. The system's first where absent.
	pub adapter: Option<String>,
	/// Where the provisioning medium is mounted, which a run's state is kept on.
	pub medium: PathBuf,
	/// Which kind of source of the board ID the device's own is to come from.
	pub board_id_source: bliti_core::board_id::SourceKind,
	/// Where the daemon listens for the console.
	pub socket: PathBuf,
	/// The date the image on the provisioning medium was built (PRS, "The image's date").
	pub image_built: jiff::civil::Date,
}

/// Run provisioning mode until interrupted.
pub async fn run(options: Options) -> Result<()> {
	// The run first, and the LED with it, so the device shows its state from the start rather than
	// after the board ID has been derived (PLED).
	let serial = match identity::platform_serial(&identity::sources()) {
		Ok(Some(serial)) => Some(serial),
		Ok(None) => {
			tracing::warn!("this board has no platform serial; a run will not survive a reboot");
			None
		}
		Err(err) => {
			tracing::warn!(%err, "cannot read this board's platform serial; a run will not survive a reboot");
			None
		}
	};
	let store = serial
		.as_deref()
		.map(|serial| Store::new(&options.medium, serial));
	let provisioner = Provisioner::start(registry(), store, options.image_built).await;

	let _led = match led::Sysfs::open() {
		Ok(sysfs) => Some(AbortOnDrop(
			tokio::spawn(led::drive(sysfs, provisioner.watch())).abort_handle(),
		)),
		Err(err) => {
			tracing::warn!(%err, "cannot use the status LED; the state will not be shown on it");
			None
		}
	};

	let socket_path = options.socket.as_path();
	let _console = socket::listen(socket_path, provisioner.clone())
		.inspect_err(|err| {
			tracing::warn!(%err, path = %socket_path.display(), "cannot listen for the console; it cannot attach to this daemon");
		})
		.ok();

	// Once, ahead of the first press, which could otherwise find the derivation still running. The
	// name is the same for as long as the process lives (PRV).
	let name = name::establish(options.board_id_source).await.render();
	tracing::info!(local_name = %name, "provisioning mode");

	let keys = Arc::new(DeviceKeys::provisioning(rand::random(), rand::random()));

	let bluez = bluer::Session::new().await?;
	let adapter = match options.adapter.as_deref() {
		Some(name) => bluez.adapter(name)?,
		None => bluez.default_adapter().await?,
	};
	adapter.set_powered(true).await?;
	tracing::info!(adapter = %adapter.name(), address = %adapter.address().await?, "adapter ready");
	// BlueZ serves the alias as the GAP Device Name, which a connecting host keeps in place of the
	// advertised name (PRV).
	adapter
		.set_alias(name.clone())
		.await
		.context("naming the adapter after the local name")?;

	let (to_window, window_events) = mpsc::unbounded_channel();
	let events: Events = Arc::new(move |event| {
		let _ = match event {
			Event::Opened => to_window.send(window::Event::Connect),
			Event::Closed => to_window.send(window::Event::Disconnect),
			Event::Refused => Ok(()),
		};
	});

	let sink = InboundSink::default();
	// One slot, so a second client reads the allocation of a device with none free (PRV).
	let slots = Arc::new(Slots::new(1));
	let (gatt, controls) = application(adapter.clone(), slots.clone(), events.clone());
	let _application = adapter
		.serve_gatt_application(gatt)
		.await
		.context("registering the GATT application")?;
	end_connections(&adapter, "ending a connection made before this start").await?;
	let _writes =
		AbortOnDrop(tokio::spawn(serve_writes(controls.writes, sink.clone())).abort_handle());
	let _sessions = AbortOnDrop(
		tokio::spawn(serve_sessions(
			controls.subscriptions,
			Link::new(adapter.clone(), slots, sink, events),
			move |transport| {
				let (keys, provisioner) = (keys.clone(), provisioner.clone());
				async move { session::run(transport, &keys, provisioner).await }
			},
		))
		.abort_handle(),
	);

	// Silent until the first press.
	let air = Radio {
		adapter: adapter.clone(),
		local_name: name,
		registered: None,
	};
	let result = tokio::select! {
		_ = window::keep(button::user(), air, window_events) => Ok(()),
		result = shutdown() => result,
	};
	// Returning drops the GATT application, which unregisters it from BlueZ. Leaving by any other
	// route leaves it and the advertisement registered against a process that is gone.
	tokio::time::sleep(ADVERTISE_SETTLE).await;
	stop(&adapter, None).await;
	result
}

/// The adapter, as the window puts it on the air.
struct Radio {
	adapter: bluer::Adapter,
	local_name: String,
	registered: Option<bluer::adv::AdvertisementHandle>,
}

#[async_trait]
impl Air for Radio {
	async fn on(&mut self) -> Result<(), String> {
		let handle = self
			.adapter
			.advertise(advertisement(self.local_name.clone()))
			.await
			.map_err(|err| err.to_string())?;
		self.registered = Some(handle);
		tracing::info!(local_name = %self.local_name, "advertising");
		Ok(())
	}

	async fn off(&mut self) {
		if self.registered.take().is_some() {
			// Unregistering reaches BlueZ asynchronously, and the controller has only a few advertising
			// slots, which registering the next advertisement while this one withdraws exhausts.
			tokio::time::sleep(ADVERTISE_SETTLE).await;
			tracing::info!("off the air");
		}
	}
}
