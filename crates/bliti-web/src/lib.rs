//! The browser client for bliti: the protocol half of the web application (BLI-WEB).
//!
//! This crate compiles to wasm and carries everything the specs describe — reading a sticker
//! (BLI-STK), recomputing and matching the advertised handle (BLI-ADV), the `NNpsk0` handshake, the
//! stream layer, and the JSON messages (BLI-CHN). It is the same code the daemon and the
//! command-line client run, which is the point: one implementation of the key schedule and the
//! handshake rather than a Rust one and a JavaScript one that must agree forever.
//!
//! What stays in JavaScript is Web Bluetooth, the camera, and the interface. Those are browser APIs
//! with no protocol in them, and binding them through wasm would buy nothing.
//!
//! The memory-hard derivation of BLI-KEY never runs here: a client reads the sticker secret from the
//! payload and only computes the handle, which is a fast hash. The crate therefore takes `bliti-core`
//! without its default features, and argon2 is not in the build at all.

use std::{cell::RefCell, rc::Rc};

use bliti_core::{
	advertisement::Advertised,
	channel::{
		envelope::{Reading, read},
		messages::{ClientMessage, DeviceMessage},
		stream::{
			Mode, Stream, Streams, connect_initiator, multiplex, read_message, write_message,
		},
	},
	sticker::StickerPayload,
};
use futures::{
	AsyncWriteExt,
	channel::{mpsc, oneshot},
	future::{self, Either},
	lock::Mutex,
};
use js_sys::{Function, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

mod transport;

use transport::WebTransport;

/// Report panics to the console rather than as an unexplained trap. Called once by the application
/// as it starts.
#[wasm_bindgen]
pub fn start() {
	console_error_panic_hook::set_once();
}

/// The service UUID a client filters its scan by (BLI-ADV), in the lowercase hyphenated form the Web
/// Bluetooth API expects. Read from the core so the browser filters on the same UUID the device
/// advertises.
#[wasm_bindgen]
pub fn service_uuid() -> String {
	bliti_core::SERVICE_UUID.to_string()
}

/// The characteristic a client writes to send bytes to the device (BLI-CHN, "Transport").
#[wasm_bindgen]
pub fn client_tx_uuid() -> String {
	bliti_core::CHARACTERISTIC_UUID_CLIENT_TX.to_string()
}

/// The characteristic the device notifies on to send bytes to the client (BLI-CHN, "Transport").
#[wasm_bindgen]
pub fn device_tx_uuid() -> String {
	bliti_core::CHARACTERISTIC_UUID_DEVICE_TX.to_string()
}

/// A sticker the application has read, by either of the paths in BLI-WEB.
#[wasm_bindgen]
pub struct Sticker {
	payload: StickerPayload,
}

#[wasm_bindgen]
impl Sticker {
	/// Read a sticker however it was given: the URL a code encodes, the fragment alone, or the
	/// human-readable rendering printed beneath the code. All three carry the same payload, and a
	/// payload that parses as none of them is reported as unreadable.
	#[wasm_bindgen(constructor)]
	pub fn new(text: &str) -> Result<Sticker, JsError> {
		use bliti_core::sticker::StickerError;
		let payload = StickerPayload::read(text).map_err(|err| match err {
			StickerError::UnsupportedVersion(version) => JsError::new(&format!(
				"That sticker is bliti version {version}, which this app does not read."
			)),
			StickerError::Malformed => JsError::new("That is not a bliti sticker."),
		})?;
		Ok(Self { payload })
	}

	/// The version of the sticker in hand. A device advertising a different one is reported as being
	/// at a version this client does not support, rather than as not matching.
	#[wasm_bindgen(getter)]
	pub fn version(&self) -> u8 {
		self.payload.version()
	}

	/// The sticker's URL, as its code encodes it.
	#[wasm_bindgen(getter)]
	pub fn url(&self) -> String {
		self.payload.to_url()
	}

	/// The human-readable rendering printed beneath the code.
	#[wasm_bindgen(getter)]
	pub fn human(&self) -> String {
		self.payload.to_human()
	}

	/// Read a local name heard over the air against this sticker (BLI-ADV, "Matching").
	///
	/// `undefined` where the name is not a bliti payload at all, which is the ordinary case for every
	/// other device in range and is passed over rather than reported.
	pub fn read_local_name(&self, name: &str) -> Option<Advertisement> {
		Advertised::from_local_name(name).map(|advertised| Advertisement {
			version: advertised.version,
			// The version is checked by the caller before the match is believed: no two versions
			// produce a matching handle, so a mismatch there is not a different device.
			matches: advertised.version == self.payload.version()
				&& advertised.matches(self.payload.secret()),
		})
	}
}

/// What a client made of one device's advertisement.
#[wasm_bindgen]
pub struct Advertisement {
	version: u8,
	matches: bool,
}

#[wasm_bindgen]
impl Advertisement {
	/// The version the device advertises.
	#[wasm_bindgen(getter)]
	pub fn version(&self) -> u8 {
		self.version
	}

	/// Whether this device is the one the sticker belongs to.
	#[wasm_bindgen(getter)]
	pub fn matches(&self) -> bool {
		self.matches
	}
}

/// The state a channel holds once it is open. Kept behind a shared handle so the exported methods can
/// hand work to a task without borrowing across an await.
struct Inner {
	payload: StickerPayload,
	transport: RefCell<Option<WebTransport>>,
	inbound: RefCell<mpsc::Sender<Vec<u8>>>,
	// An async lock rather than a cell: it is held across opening a stream, and two sends in
	// flight queue behind each other rather than colliding over the handle.
	streams: Mutex<Option<Streams>>,
	// The stream this client named itself on, held open for whatever a feature gives a client to send.
	control: Mutex<Option<Stream>>,
	// Closes the device's reporting stream, the same way a subscription is closed.
	reporting_closer: RefCell<Option<oneshot::Sender<()>>>,
}

/// A channel to a device: the handshake of BLI-CHN and the streams above it.
///
/// Built before the connection is driven, so the application can start feeding it the device's
/// notifications before the handshake runs.
#[wasm_bindgen]
pub struct Channel {
	inner: Rc<Inner>,
}

#[wasm_bindgen]
impl Channel {
	/// Prepare a channel for a device, given the function that writes a chunk to the device's write
	/// characteristic. The handshake does not run until [`Channel::connect`] is called.
	#[wasm_bindgen(constructor)]
	pub fn new(sticker: &Sticker, write: Function) -> Channel {
		let (transport, inbound) = WebTransport::new(write);
		Channel {
			inner: Rc::new(Inner {
				payload: sticker.payload.clone(),
				transport: RefCell::new(Some(transport)),
				inbound: RefCell::new(inbound),
				streams: Mutex::new(None),
				control: Mutex::new(None),
				reporting_closer: RefCell::new(None),
			}),
		}
	}

	/// Feed bytes arriving on the device's notify characteristic.
	pub fn receive(&self, bytes: &[u8]) {
		let _ = self.inner.inbound.borrow_mut().try_send(bytes.to_vec());
	}

	/// Run the handshake, name this client to the device, and start reading what the device reports.
	///
	/// The client opens a control stream and sends its hello without waiting for the device's, and the
	/// device opens its reporting stream without being asked; neither blocks on the other (BLI-MSG).
	/// Every message the device sends is passed to `on_message` as one of the outcomes described in
	/// [`describe`], and `on_closed` is called once the reporting stream ends.
	pub fn connect(
		&self,
		name: String,
		version: String,
		on_message: Function,
		on_closed: Function,
	) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let transport = inner
				.transport
				.borrow_mut()
				.take()
				.ok_or_else(|| JsError::new("this channel has already been connected"))?;

			let encrypted = connect_initiator(transport, inner.payload.secret())
				.await
				.map_err(|err| JsError::new(&format!("handshake failed: {err}")))?;

			let (mut streams, driver) = multiplex(encrypted, Mode::Client);
			spawn_local(async move {
				let _ = driver.await;
			});

			// This client names itself, on a control stream of its own. The device logs it and never
			// acts on it, and nothing here waits for an answer because there is none.
			let mut control = streams
				.open()
				.await
				.map_err(|err| JsError::new(&format!("opening the control stream: {err}")))?;
			let hello = ClientMessage::Hello { name, version };
			write_message(&mut control, &hello.to_json())
				.await
				.map_err(|err| JsError::new(&format!("naming this client: {err}")))?;

			let reporting = streams
				.accept()
				.await
				.ok_or_else(|| JsError::new("the device closed the channel before reporting"))?;
			// The reporting stream is closed the same way a subscription is, so that dropping the
			// channel does not leave a reader holding it.
			let (closer, closing) = oneshot::channel();
			spawn_local(report_until_closed(
				reporting, closing, on_message, on_closed,
			));
			*inner.reporting_closer.borrow_mut() = Some(closer);

			*inner.control.lock().await = Some(control);
			*inner.streams.lock().await = Some(streams);
			Ok(JsValue::UNDEFINED)
		})
	}

	/// Subscribe to what the device sends continuously on a topic.
	///
	/// The subscription is the stream: it begins with this message and ends when the stream is closed,
	/// so [`Subscription::close`] is the unsubscribe and there is no message for it (BLI-MSG). A topic
	/// the device does not know yields no data and no error, which is what an older device looks like.
	pub fn subscribe(&self, topic: String, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let mut streams = inner.streams.lock().await;
			let streams = streams
				.as_mut()
				.ok_or_else(|| JsError::new("this channel is not connected"))?;
			let mut stream = streams
				.open()
				.await
				.map_err(|err| JsError::new(&format!("opening a subscription: {err}")))?;
			write_message(&mut stream, &ClientMessage::Subscribe { topic }.to_json())
				.await
				.map_err(|err| JsError::new(&format!("subscribing: {err}")))?;

			let (closer, closing) = oneshot::channel();
			spawn_local(report_until_closed(stream, closing, on_message, on_closed));
			Ok(JsValue::from(SubscriptionHandle {
				closer: RefCell::new(Some(closer)),
			}))
		})
	}
}

/// One open subscription, which lasts exactly as long as its stream.
#[wasm_bindgen]
pub struct SubscriptionHandle {
	closer: RefCell<Option<oneshot::Sender<()>>>,
}

#[wasm_bindgen]
impl SubscriptionHandle {
	/// Unsubscribe, by asking the reader to close the stream.
	///
	/// The reader owns the stream and selects on this alongside its read, rather than the two
	/// contending for one lock: a read is pending essentially always, including the ordinary case of a
	/// device that does not know the topic and sends nothing, so a closer that had to take the stream
	/// from under the reader would wait forever and the unsubscribe would never happen.
	///
	/// Closed rather than dropped: closing sends the graceful end of stream the device reads to stop
	/// sending, where dropping would reach it as a reset. Either ends the subscription, but the
	/// ordinary path should be the graceful one.
	pub fn close(&self) {
		if let Some(closer) = self.closer.borrow_mut().take() {
			let _ = closer.send(());
		}
	}
}

/// Pass everything a stream carries to the application, until it ends or the application asks to
/// close it.
///
/// Each message is described rather than handed over raw, so the application is told which of the
/// outcomes of BLI-MSG it is looking at and can render accordingly. A fault is the exception: the
/// receiver closes the stream a fault arrived on (BLI-MSG), so it ends the read here rather than
/// being reported and read past, which would let a peer that has completed the handshake stream
/// malformed messages indefinitely.
async fn report_until_closed(
	mut stream: Stream,
	closing: oneshot::Receiver<()>,
	on_message: Function,
	on_closed: Function,
) {
	let mut closing = closing;
	let ended = loop {
		let read = read_message(&mut stream);
		futures::pin_mut!(read);
		match future::select(read, &mut closing).await {
			Either::Left((Ok(Some(raw)), _)) => {
				let (described, fault) = describe(&raw);
				let _ = on_message.call1(&JsValue::NULL, &JsValue::from_str(&described));
				if let Some(fault) = fault {
					break Some(fault);
				}
			}
			Either::Left((Ok(None), _)) => break None,
			Either::Left((Err(err), _)) => break Some(err.to_string()),
			// The application has unsubscribed, or is going away.
			Either::Right(_) => break None,
		}
	};

	let _ = stream.close().await;
	let _ = on_closed.call1(
		&JsValue::NULL,
		&match ended {
			Some(why) => JsValue::from_str(&why),
			None => JsValue::NULL,
		},
	);
}

/// Describe one message to the application as JSON, and say whether it was a fault.
///
/// The three outcomes of BLI-MSG are kept apart here rather than in the application, so every client
/// surface inherits the same reading of the wire. `message` is what this build understood, `skipped`
/// is a device newer than this build saying something safe to pass over, `refused` is one saying
/// something that must not be half read, and `fault` is a device not speaking the protocol.
fn describe(raw: &[u8]) -> (String, Option<String>) {
	match read::<DeviceMessage>(raw) {
		Ok(Reading::Message(message)) => (
			serde_json::json!({ "kind": "message", "message": message }).to_string(),
			None,
		),
		Ok(Reading::Skipped(skip)) => (
			serde_json::json!({ "kind": "skipped", "detail": skip.to_string() }).to_string(),
			None,
		),
		Ok(Reading::Refused(refusal)) => (
			serde_json::json!({ "kind": "refused", "detail": refusal.to_string() }).to_string(),
			None,
		),
		Err(fault) => (
			serde_json::json!({ "kind": "fault", "detail": fault.to_string() }).to_string(),
			Some(fault.to_string()),
		),
	}
}
