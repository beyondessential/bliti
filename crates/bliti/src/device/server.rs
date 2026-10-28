//! The GATT server a device offers a client, and what it takes to go on and come off the air.
//!
//! Shared by the daemon and by provisioning mode, which differ in how many clients they serve, who
//! they serve them as, and when they advertise. Behaviour is specified in ADV and CHN.

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use bliti_core::{
	CHARACTERISTIC_UUID_CLIENT_TX, SERVICE_UUID,
	slot::{Allocation, CHARACTERISTIC_UUID_ALLOCATION, Slot},
};
use bluer::gatt::local::{
	Application, Characteristic, CharacteristicControl, CharacteristicControlEvent,
	CharacteristicNotify, CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
	CharacteristicWriteMethod, Service, characteristic_control,
};
use futures::StreamExt;

use super::slots::Slots;
use crate::gatt::InboundSink;

/// How long to leave BlueZ to withdraw an advertisement before registering the next.
pub const ADVERTISE_SETTLE: Duration = Duration::from_millis(250);

/// How long to wait before trying a failed advertisement registration again, and the ceiling that
/// wait backs off to.
pub const ADVERTISE_RETRY: Duration = Duration::from_secs(1);
pub const ADVERTISE_RETRY_MAX: Duration = Duration::from_secs(30);

/// Something that happened to a client's channel, which the daemon decides what to do about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
	/// A client subscribed to its slot, so a session opened.
	Opened,
	/// A session ended.
	Closed,
	/// A client read the allocation and was given no slot.
	Refused,
}

/// Told of each [`Event`].
pub type Events = Arc<dyn Fn(Event) + Send + Sync>;

/// What BlueZ reports of the application's characteristics, as clients use them.
pub struct Controls {
	/// Where each client's writes arrive, for [`serve_writes`].
	pub writes: CharacteristicControl,
	/// Where each client's subscription to its slot arrives, for
	/// [`serve_sessions`](super::sessions::serve_sessions).
	pub subscriptions: Vec<(Slot, CharacteristicControl)>,
}

/// The advertisement a device registers, carrying `local_name`.
///
/// The service UUID goes in the advertisement, because filtering a scan by service UUID is the only
/// filtering some client platforms offer and it is applied to the advertisement. The name is the one
/// element a host will place in the scan response, and so the only way a device can say anything
/// more and still fit a controller that does only legacy advertising.
pub fn advertisement(local_name: String) -> bluer::adv::Advertisement {
	bluer::adv::Advertisement {
		advertisement_type: bluer::adv::Type::Peripheral,
		service_uuids: [SERVICE_UUID].into_iter().collect(),
		local_name: Some(local_name),
		discoverable: Some(true),
		// A short interval so a scanning client finds the device quickly and reliably. The default is
		// over a second, which on a legacy controller leaves a client's scan window catching an
		// advertisement only now and then, so the first attempt after a device comes on the air often
		// hears nothing. These are hints the controller rounds to what it supports.
		min_interval: Some(Duration::from_millis(100)),
		max_interval: Some(Duration::from_millis(150)),
		..Default::default()
	}
}

/// The GATT application: one service with the characteristic a client writes, the one it reads for
/// its slot, and the slots the device notifies on.
///
/// Every slot a device can offer is registered, whether or not `slots` gives each out, because a device
/// MUST offer them all (CHN, "Transport"); a client given no slot subscribes to none.
///
/// Writes and notifications run over sockets BlueZ acquires for each client: one carrying that
/// client's writes in the order it made them, the other what the device notifies to that client
/// alone. Each arrives on the characteristic's control, in the [`Controls`] returned (CHN, "Several
/// clients at once").
pub fn application(
	adapter: bluer::Adapter,
	slots: Arc<Slots>,
	events: Events,
) -> (Application, Controls) {
	let (writes, writes_handle) = characteristic_control();
	let (subscriptions, subscription_handles): (Vec<_>, Vec<_>) = Slot::all()
		.map(|slot| {
			let (control, handle) = characteristic_control();
			((slot, control), (slot, handle))
		})
		.unzip();

	let client_tx = Characteristic {
		uuid: CHARACTERISTIC_UUID_CLIENT_TX,
		write: Some(CharacteristicWrite {
			write: true,
			write_without_response: true,
			method: CharacteristicWriteMethod::Io,
			..Default::default()
		}),
		control_handle: writes_handle,
		..Default::default()
	};
	let allocation = Characteristic {
		uuid: CHARACTERISTIC_UUID_ALLOCATION,
		read: Some(allocation(adapter, slots, events)),
		..Default::default()
	};
	let device_tx = subscription_handles
		.into_iter()
		.map(|(slot, subscriptions)| Characteristic {
			uuid: slot.uuid(),
			notify: Some(CharacteristicNotify {
				notify: true,
				method: CharacteristicNotifyMethod::Io,
				..Default::default()
			}),
			control_handle: subscriptions,
			..Default::default()
		});
	let application = Application {
		services: vec![Service {
			uuid: SERVICE_UUID,
			primary: true,
			characteristics: [client_tx, allocation]
				.into_iter()
				.chain(device_tx)
				.collect(),
			..Default::default()
		}],
		..Default::default()
	};
	(
		application,
		Controls {
			writes,
			subscriptions,
		},
	)
}

/// Answer a read of the allocation characteristic with the reading client's slot (CHN, "Transport").
fn allocation(adapter: bluer::Adapter, slots: Arc<Slots>, events: Events) -> CharacteristicRead {
	CharacteristicRead {
		read: true,
		fun: Box::new(move |request| {
			let (adapter, slots, events) = (adapter.clone(), slots.clone(), events.clone());
			Box::pin(async move {
				let client = request.device_address;
				let allocation = slots
					.allocate(client, |holder| is_connected(&adapter, holder))
					.await;
				match allocation {
					Allocation::Given(slot) => {
						tracing::info!(%client, slot = slot.number(), mtu = request.mtu, "gave a client its slot");
					}
					Allocation::Full => {
						tracing::warn!(%client, "every slot is held; turning a client away");
						// Its connecting stopped the controller advertising, and it opens no session
						// whose opening would resume it.
						events(Event::Refused);
					}
				}
				Ok(allocation.to_value(request.mtu))
			})
		}),
		..Default::default()
	}
}

/// Whether a client is connected, as far as BlueZ knows. One BlueZ cannot name has gone.
pub async fn is_connected(adapter: &bluer::Adapter, client: bluer::Address) -> bool {
	match adapter.device(client) {
		Ok(device) => device.is_connected().await.unwrap_or(false),
		Err(_) => false,
	}
}

/// Hand each client's writes to its session, in the order the client made them.
///
/// One socket per client, read by one task, is what keeps them in order: handled one call at a time,
/// writes a client makes without waiting for a response can be taken up out of order.
pub async fn serve_writes(mut writes: CharacteristicControl, sink: InboundSink) {
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

/// End every connection to the adapter.
///
/// As a device starts, a connection made before it was served by an earlier daemon, and its client
/// would otherwise wait on it with nothing to tell it the channel is gone (CHN). As a device goes away,
/// a connection is ended so its client reads the channel closing rather than the link dying (CTL).
pub async fn end_connections(adapter: &bluer::Adapter, why: &str) -> Result<()> {
	for address in adapter.device_addresses().await? {
		let device = adapter.device(address)?;
		if !device.is_connected().await.unwrap_or(false) {
			continue;
		}
		tracing::info!(%address, "{why}");
		if let Err(error) = device.disconnect().await {
			tracing::warn!(%address, %error, "could not end a connection");
		}
	}
	Ok(())
}

/// Go off the air and end every connection, ahead of the caller dropping the GATT application.
///
/// A client still connected as the service is withdrawn is told it has gone, and keeps a record of the
/// device without it, which some hosts then match in place of what the device advertises (CHN).
pub async fn stop(
	adapter: &bluer::Adapter,
	advertisement: Option<bluer::adv::AdvertisementHandle>,
) {
	if let Some(advertisement) = advertisement {
		drop(advertisement);
		// Unregistering is asynchronous, and a client could connect again until it lands.
		tokio::time::sleep(ADVERTISE_SETTLE).await;
	}
	if let Err(err) = end_connections(adapter, "ending a connection before stopping").await {
		tracing::warn!(%err, "could not list the connections to end");
	}
	tracing::info!("stopping");
}

/// Resolve when the daemon is asked to stop, by either of the signals that mean it.
///
/// systemd stops a unit with `SIGTERM`, so waiting on ctrl-c alone would mean the ordinary way of
/// stopping the daemon is the one way that skips unregistering from BlueZ.
pub async fn shutdown() -> Result<()> {
	use tokio::signal::unix::{SignalKind, signal};
	let mut terminate = signal(SignalKind::terminate())?;
	let mut interrupt = signal(SignalKind::interrupt())?;
	tokio::select! {
		_ = terminate.recv() => {}
		_ = interrupt.recv() => {}
	}
	Ok(())
}
