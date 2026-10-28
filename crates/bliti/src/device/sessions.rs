//! The sessions a client opens by subscribing to its slot, over the GATT transport.
//!
//! What a session says is the caller's: this carries a client's bytes in and out, paces them, and
//! notices when the client has gone.

use std::{
	future::Future,
	sync::{Arc, Mutex},
	time::Duration,
};

use bliti_core::slot::{self, Slot};
use bluer::gatt::{
	CharacteristicWriter,
	local::{CharacteristicControl, CharacteristicControlEvent},
};
use futures::{StreamExt, stream};
use tracing::Instrument;

use super::{
	pacing::Pacer,
	server::{Event, Events, is_connected},
	slots::Slots,
};
use crate::{
	gatt::{GattTransport, InboundSink},
	session::SessionError,
};

/// How long a session's last notifications may take to go out once it has ended.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// What every session on the device shares.
///
/// `events` is told as a session opens and as it ends, so the daemon can resume advertising: a
/// client connecting stops the controller advertising, and only re-registering the advertisement
/// brings it back.
#[derive(Clone)]
pub struct Link {
	adapter: bluer::Adapter,
	slots: Arc<Slots>,
	sink: InboundSink,
	events: Events,
	/// One allowance for the whole device, since every session shares the one radio.
	pacer: Arc<Mutex<Pacer>>,
}

impl Link {
	pub fn new(
		adapter: bluer::Adapter,
		slots: Arc<Slots>,
		sink: InboundSink,
		events: Events,
	) -> Self {
		Self {
			adapter,
			slots,
			sink,
			events,
			pacer: Arc::new(Mutex::new(Pacer::new(std::time::Instant::now()))),
		}
	}
}

/// Open a session for each client that subscribes to its slot, and run them side by side.
///
/// `session` runs one over the transport it is given, and ends it by returning.
pub async fn serve_sessions<F, Fut>(
	subscriptions: Vec<(Slot, CharacteristicControl)>,
	link: Link,
	session: F,
) where
	F: Fn(GattTransport) -> Fut + Send + Sync + 'static,
	Fut: Future<Output = Result<(), SessionError>> + Send + 'static,
{
	let session = Arc::new(session);
	let mut subscriptions = stream::select_all(
		subscriptions
			.into_iter()
			.map(|(slot, control)| control.map(move |event| (slot, event))),
	);
	while let Some((slot, event)) = subscriptions.next().await {
		let CharacteristicControlEvent::Notify(notifier) = event else {
			continue;
		};
		// Every line a session logs names its client, so sessions running side by side read apart.
		let span = tracing::info_span!(
			"session",
			client = %notifier.device_address(),
			slot = slot.number()
		);
		tokio::spawn(serve_session(slot, notifier, link.clone(), session.clone()).instrument(span));
	}
	tracing::error!("BlueZ stopped reporting subscriptions; no further sessions can open");
}

/// Serve one client's session, from its subscribing to its leaving or the session ending.
async fn serve_session<F, Fut>(
	slot: Slot,
	notifier: CharacteristicWriter,
	link: Link,
	session: Arc<F>,
) where
	F: Fn(GattTransport) -> Fut + Send + Sync + 'static,
	Fut: Future<Output = Result<(), SessionError>> + Send + 'static,
{
	let client = notifier.device_address();
	// Dropping the notifier closes only this client's socket, so the slot's holder is untouched.
	if !link.slots.holds(slot, client).await {
		tracing::warn!("client subscribed to a slot it was not given; opening no session");
		return;
	}

	let Link {
		adapter,
		slots: _,
		sink,
		events,
		pacer,
	} = link;
	// A client subscribing is what opens a session: it is the point at which the device can send, so
	// it is the point at which a handshake can run.
	tracing::info!(mtu = notifier.mtu(), "client subscribed; opening a session");
	events(Event::Opened);
	let (transport, mut outbound) =
		GattTransport::open(&sink, client, slot::payload(notifier.mtu()));

	// Pump the device's bytes out as notifications for as long as the client is subscribed, and notice
	// when it stops being subscribed.
	//
	// Noticing matters: nothing else tells the device the client has gone. The session reads until its
	// transport ends, and the transport only ends when the session drops it, so without this the two
	// wait on each other and the device stays busy with a client that left.
	let (left, gone) = tokio::sync::oneshot::channel();
	let mut pump = tokio::spawn(async move {
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
		result = session(transport) => {
			match result {
				Ok(()) => tracing::info!("session ended"),
				Err(err) => tracing::info!(%err, "session ended"),
			}
			// What the session last wrote may still be held back by the pacer, and it is often what
			// the client most needs, as `going-away` is (CTL). The pump ends once it has sent it all.
			if tokio::time::timeout(DRAIN_TIMEOUT, &mut pump).await.is_err() {
				tracing::debug!("the last notifications did not go out in time; dropping them");
			}
		},
		_ = gone => tracing::info!("client unsubscribed; session ended"),
	}
	pump.abort();
	// A client left connected with no channel would wait on one nothing answers, and would go on
	// holding its slot (CHN).
	if is_connected(&adapter, client).await {
		tracing::info!("ending the connection of a client whose channel has ended");
		if let Ok(device) = adapter.device(client) {
			if let Err(error) = device.disconnect().await {
				tracing::debug!(%error, "could not end the connection; it may already be going");
			}
		}
	}
	events(Event::Closed);
}
