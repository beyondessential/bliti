//! The browser client for bliti: the protocol half of the web application (WEB).
//!
//! This crate compiles to wasm and carries everything the specs describe — reading a QR code
//! (QR), recomputing and matching the advertised handle (ADV), the `NNpsk0` handshake, the
//! stream layer, and the JSON messages (CHN). It is the same code the daemon and the
//! command-line client run, which is the point: one implementation of the key schedule and the
//! handshake rather than a Rust one and a JavaScript one that must agree forever.
//!
//! What stays in JavaScript is Web Bluetooth, the camera, and the interface. Those are browser APIs
//! with no protocol in them, and binding them through wasm would buy nothing.
//!
//! The memory-hard derivation of KEY never runs here: a client reads the presence token from the
//! payload and only computes the handle, which is a fast hash. The crate therefore takes `bliti-core`
//! without its default features, and argon2 is not in the build at all.

use std::{cell::RefCell, rc::Rc};

use bliti_core::{
	advertisement::Advertised,
	channel::{
		capabilities,
		envelope::{Reading, read},
		messages::Message,
		stream::{Mode, Opener, Stream, connect_initiator, multiplex, read_message, write_message},
	},
	qr::QrPayload,
};
use futures::{
	AsyncRead, AsyncReadExt, AsyncWriteExt, StreamExt,
	channel::{mpsc, oneshot},
	future::{self, Either},
};
use js_sys::{Function, JSON, Promise};
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

/// The service UUID a client filters its scan by (ADV), in the lowercase hyphenated form the Web
/// Bluetooth API expects. Read from the core so the browser filters on the same UUID the device
/// advertises.
#[wasm_bindgen]
pub fn service_uuid() -> String {
	bliti_core::SERVICE_UUID.to_string()
}

/// The characteristic a client writes to send bytes to the device (CHN, "Transport").
#[wasm_bindgen]
pub fn client_tx_uuid() -> String {
	bliti_core::CHARACTERISTIC_UUID_CLIENT_TX.to_string()
}

/// The characteristic the device notifies on to send bytes to the client (CHN, "Transport").
#[wasm_bindgen]
pub fn device_tx_uuid() -> String {
	bliti_core::CHARACTERISTIC_UUID_DEVICE_TX.to_string()
}

/// Whether a document stays within a device's `capabilities.document`, checked before it is proposed
/// (NSCR) with the checker the device rejects with (NET), so the two cannot disagree.
///
/// Both are plain objects. Null where the document is within them, or `{ at, reason }` naming the
/// first member they do not cover. The reason is the checker's, for logs; a screen words its own.
#[wasm_bindgen]
pub fn check_capabilities(document: JsValue, capabilities: JsValue) -> Result<JsValue, JsError> {
	let text = |value: &JsValue, what: &str| {
		JSON::stringify(value)
			.map(String::from)
			.map_err(|_| JsError::new(&format!("the {what} cannot be written as JSON")))
	};
	let fault = capability_fault(
		&text(&document, "document")?,
		&text(&capabilities, "capabilities")?,
	)
	.map_err(|why| JsError::new(&why))?;
	match fault {
		Some(fault) => JSON::parse(&fault.to_string())
			.map_err(|_| JsError::new("the fault cannot be read back as JSON")),
		None => Ok(JsValue::NULL),
	}
}

/// The fault [`check_capabilities`] reports, from the document and capabilities as JSON text.
fn capability_fault(
	document: &str,
	capabilities: &str,
) -> Result<Option<serde_json::Value>, String> {
	let object = |json: &str, what: &str| match serde_json::from_str(json) {
		Ok(serde_json::Value::Object(map)) => Ok(map),
		Ok(_) => Err(format!("the {what} is an object")),
		Err(err) => Err(format!("the {what} is not JSON: {err}")),
	};
	let document = object(document, "document")?;
	let capabilities = object(capabilities, "capabilities")?;
	Ok(capabilities::check(&document, &capabilities)
		.err()
		.map(|invalid| serde_json::json!({ "at": invalid.at, "reason": invalid.reason })))
}

/// A QR code the application has read, by either of the paths in WEB.
#[wasm_bindgen]
pub struct QrCode {
	payload: QrPayload,
}

#[wasm_bindgen]
impl QrCode {
	/// Read a QR code however it was given: the URL a code encodes, the fragment alone, or the
	/// human-readable rendering printed beneath the code. All three carry the same payload, and a
	/// payload that parses as none of them is reported as unreadable.
	#[wasm_bindgen(constructor)]
	pub fn new(text: &str) -> Result<QrCode, JsError> {
		use bliti_core::qr::QrError;
		let payload = QrPayload::read(text).map_err(|err| match err {
			QrError::UnsupportedVersion(version) => JsError::new(&format!(
				"That QR code is bliti version {version}, which this app does not read."
			)),
			QrError::Malformed => JsError::new("That is not a bliti QR code."),
		})?;
		Ok(Self { payload })
	}

	/// The version of the QR code in hand. A device advertising a different one is reported as being
	/// at a version this client does not support, rather than as not matching.
	#[wasm_bindgen(getter)]
	pub fn version(&self) -> u8 {
		self.payload.version()
	}

	/// The QR code's URL, as its code encodes it.
	#[wasm_bindgen(getter)]
	pub fn url(&self) -> String {
		self.payload.to_url()
	}

	/// The human-readable rendering printed beneath the code.
	#[wasm_bindgen(getter)]
	pub fn human(&self) -> String {
		self.payload.to_human()
	}

	/// Read a local name heard over the air against this QR code (ADV, "Matching").
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

	/// Whether this device is the one the QR code belongs to.
	#[wasm_bindgen(getter)]
	pub fn matches(&self) -> bool {
		self.matches
	}
}

/// The state a channel holds once it is open. Kept behind a shared handle so the exported methods can
/// hand work to a task without borrowing across an await.
struct Inner {
	payload: QrPayload,
	transport: RefCell<Option<WebTransport>>,
	inbound: RefCell<mpsc::Sender<Vec<u8>>>,
	// Opens subscription streams. Independent of the accept loop, so a subscription can be opened while
	// the pushed streams are being read.
	opener: RefCell<Option<Opener>>,
	// Closes the streams the device pushed (its hello and the default feed), which is how the client
	// declines the feed (MSG).
	feed_closers: RefCell<Vec<oneshot::Sender<()>>>,
}

/// A channel to a device: the handshake of CHN and the streams above it.
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
	pub fn new(code: &QrCode, write: Function) -> Channel {
		let (transport, inbound) = WebTransport::new(write);
		Channel {
			inner: Rc::new(Inner {
				payload: code.payload.clone(),
				transport: RefCell::new(Some(transport)),
				inbound: RefCell::new(inbound),
				opener: RefCell::new(None),
				feed_closers: RefCell::new(Vec::new()),
			}),
		}
	}

	/// Feed bytes arriving on the device's notify characteristic.
	pub fn receive(&self, bytes: &[u8]) {
		let _ = self.inner.inbound.borrow_mut().try_send(bytes.to_vec());
	}

	/// Run the handshake, name this client to the device, and read the streams the device pushes.
	///
	/// The client opens its hello stream and sends its hello without waiting for the device's, and the
	/// device pushes its own hello and the `default` feed without being asked; neither blocks on the
	/// other (MSG). Every message on a pushed stream is passed to `on_message` as one of the outcomes
	/// described in [`describe`]. `on_closed` is called when a pushed stream ends. `on_channel_closed`
	/// is called once the whole channel closes, for any reason: the device going out of range or
	/// restarting, or a fault the connection could not survive (CHN).
	pub fn connect(
		&self,
		name: String,
		version: String,
		on_message: Function,
		on_closed: Function,
		on_channel_closed: Function,
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
				let why = driver.await.err().map(|err| err.to_string());
				let _ = on_channel_closed.call1(
					&JsValue::NULL,
					&match why {
						Some(why) => JsValue::from_str(&why),
						None => JsValue::NULL,
					},
				);
			});

			// This client names itself, on its own hello stream. The device logs it and never acts on
			// it, and nothing here waits for an answer because there is none.
			let opener = streams.opener();
			let mut hello_stream = opener
				.open()
				.await
				.map_err(|err| JsError::new(&format!("opening the hello stream: {err}")))?;
			write_message(
				&mut hello_stream,
				&Message::Hello { name, version }.to_json(),
			)
			.await
			.map_err(|err| JsError::new(&format!("naming this client: {err}")))?;
			*inner.opener.borrow_mut() = Some(opener);

			// Read whatever the device pushes: its hello, and the default feed. Each is delivered to
			// `on_message`, and each carries a closer so declining the feed closes it (MSG).
			let inner_loop = inner.clone();
			spawn_local(async move {
				// Keep the hello stream we opened alive for the life of the loop.
				let _hello_stream = hello_stream;
				while let Some(stream) = streams.accept().await {
					let (closer, closing) = oneshot::channel();
					inner_loop.feed_closers.borrow_mut().push(closer);
					spawn_local(report_until_closed(
						stream,
						closing,
						on_message.clone(),
						on_closed.clone(),
					));
				}
			});

			Ok(JsValue::UNDEFINED)
		})
	}

	/// Close the streams the device pushed, declining the feed. Sampling continues on the device, so a
	/// later [`Channel::subscribe`] for `default` is served what is current (MSG).
	pub fn close_feed(&self) {
		for closer in self.inner.feed_closers.borrow_mut().drain(..) {
			let _ = closer.send(());
		}
	}

	/// Subscribe to what the device sends continuously on a topic.
	///
	/// The subscription is the stream: it begins with this message and ends when the stream is closed,
	/// so [`SubscriptionHandle::close`] is the unsubscribe and there is no message for it (MSG). A
	/// topic the device does not know yields no data and no error, which is what an older device looks
	/// like.
	pub fn subscribe(&self, topic: String, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let opener = inner
				.opener
				.borrow()
				.clone()
				.ok_or_else(|| JsError::new("this channel is not connected"))?;
			let mut stream = opener
				.open()
				.await
				.map_err(|err| JsError::new(&format!("opening a subscription: {err}")))?;
			write_message(&mut stream, &Message::Subscribe { topic }.to_json())
				.await
				.map_err(|err| JsError::new(&format!("subscribing: {err}")))?;

			let (closer, closing) = oneshot::channel();
			spawn_local(report_until_closed(stream, closing, on_message, on_closed));
			Ok(JsValue::from(SubscriptionHandle {
				closer: RefCell::new(Some(closer)),
			}))
		})
	}

	/// Open a configuration session: a stream whose first message is `configure` (CFG).
	///
	/// Everything the device sends on the session is passed to `on_message` exactly as a subscription's
	/// messages are, and `on_closed` is called once the stream ends. The handle this resolves to sends
	/// the client's half of the exchange on the same stream, and closing it ends the session, which a
	/// device reads as abandoning any proposal not confirmed.
	pub fn configure(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let opener = inner
				.opener
				.borrow()
				.clone()
				.ok_or_else(|| JsError::new("this channel is not connected"))?;
			let mut stream = opener
				.open()
				.await
				.map_err(|err| JsError::new(&format!("opening a configuration session: {err}")))?;
			write_message(&mut stream, &Message::Configure.to_json())
				.await
				.map_err(|err| JsError::new(&format!("opening a configuration session: {err}")))?;

			// The session is read and written at once, so the stream is split: a read is pending
			// essentially always, and a write must not wait on it.
			let (mut reader, mut writer) = AsyncReadExt::split(stream);
			let (outbound, mut queued) = mpsc::unbounded::<Vec<u8>>();
			spawn_local(async move {
				while let Some(message) = queued.next().await {
					if write_message(&mut writer, &message).await.is_err() {
						break;
					}
				}
				let _ = writer.close().await;
			});

			let (closer, closing) = oneshot::channel();
			let ending = outbound.clone();
			spawn_local(async move {
				let ended = report(&mut reader, closing, &on_message).await;
				// However the read ended, the session has, so the write half closes too.
				ending.close_channel();
				call_closed(&on_closed, ended);
			});

			Ok(JsValue::from(ConfigurationHandle {
				outbound: RefCell::new(Some(outbound)),
				closer: RefCell::new(Some(closer)),
			}))
		})
	}
}

/// One open configuration session, which lasts exactly as long as its stream (CFG).
#[wasm_bindgen]
pub struct ConfigurationHandle {
	outbound: RefCell<Option<mpsc::UnboundedSender<Vec<u8>>>>,
	closer: RefCell<Option<oneshot::Sender<()>>>,
}

#[wasm_bindgen]
impl ConfigurationHandle {
	/// Propose a document: the whole configuration the client wants in force, as a plain object, and
	/// whether the device is to verify it.
	pub fn propose(&self, document: JsValue, verify: bool) -> Result<(), JsError> {
		let json = JSON::stringify(&document)
			.map_err(|_| JsError::new("the document cannot be written as JSON"))?;
		let message = proposal(&String::from(json), verify).map_err(|why| JsError::new(&why))?;
		self.send(message)
	}

	/// Make the applied proposal durable.
	pub fn confirm(&self) -> Result<(), JsError> {
		self.send(Message::Confirm.to_json())
	}

	/// Abandon the proposal, whether it is still being verified or already applied.
	pub fn discard(&self) -> Result<(), JsError> {
		self.send(Message::Discard.to_json())
	}

	/// Ask for the wireless networks the device can see, on one wireless interface or, where none is
	/// named, on every one able to.
	pub fn scan(&self, interface: Option<String>) -> Result<(), JsError> {
		self.send(Message::Scan { interface }.to_json())
	}

	/// Ask for what the device's radios can see of the spectrum, on one wireless interface or, where
	/// none is named, on every one able to.
	pub fn survey(&self, interface: Option<String>) -> Result<(), JsError> {
		self.send(Message::Survey { interface }.to_json())
	}

	/// Ask the device to join by WPS, by `push-button` or `pin` (WLAN), on one wireless interface or,
	/// where none is named, on one the device chooses.
	pub fn wps(&self, method: String, interface: Option<String>) -> Result<(), JsError> {
		self.send(Message::Wps { method, interface }.to_json())
	}

	/// End the session. Closed rather than dropped, for the reason [`SubscriptionHandle::close`] gives.
	pub fn close(&self) {
		self.outbound.borrow_mut().take();
		if let Some(closer) = self.closer.borrow_mut().take() {
			let _ = closer.send(());
		}
	}

	fn send(&self, message: Vec<u8>) -> Result<(), JsError> {
		self.outbound
			.borrow()
			.as_ref()
			.and_then(|outbound| outbound.unbounded_send(message).ok())
			.ok_or_else(|| JsError::new("this configuration session has ended"))
	}
}

/// The `configuration` message proposing a document, from the document as JSON text.
fn proposal(json: &str, verify: bool) -> Result<Vec<u8>, String> {
	match serde_json::from_str(json) {
		Ok(serde_json::Value::Object(document)) => Ok(Message::Configuration {
			document,
			capabilities: None,
			verify: Some(verify),
		}
		.to_json()),
		Ok(_) => Err("a document is an object".to_owned()),
		Err(err) => Err(format!("the document is not JSON: {err}")),
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
/// close it, then close it.
async fn report_until_closed(
	mut stream: Stream,
	closing: oneshot::Receiver<()>,
	on_message: Function,
	on_closed: Function,
) {
	let ended = report(&mut stream, closing, &on_message).await;
	let _ = stream.close().await;
	call_closed(&on_closed, ended);
}

/// Pass everything a stream carries to the application, until it ends or the application asks to
/// stop, returning why it ended where that was not the ordinary way.
///
/// Each message is described rather than handed over raw, so the application is told which of the
/// outcomes of MSG it is looking at and can render accordingly. A fault is the exception: the
/// receiver closes the stream a fault arrived on (MSG), so it ends the read here rather than
/// being reported and read past, which would let a peer that has completed the handshake stream
/// malformed messages indefinitely.
async fn report<R: AsyncRead + Unpin>(
	reader: &mut R,
	mut closing: oneshot::Receiver<()>,
	on_message: &Function,
) -> Option<String> {
	loop {
		let read = read_message(reader);
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
	}
}

fn call_closed(on_closed: &Function, ended: Option<String>) {
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
/// The three outcomes of MSG are kept apart here rather than in the application, so every client
/// surface inherits the same reading of the wire. `message` is what this build understood, `skipped`
/// is a device newer than this build saying something safe to pass over, `refused` is one saying
/// something that must not be half read, and `fault` is a device not speaking the protocol.
fn describe(raw: &[u8]) -> (String, Option<String>) {
	match read::<Message>(raw) {
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

#[cfg(test)]
mod tests {
	use bliti_core::channel::envelope::{Reading, read};

	use super::*;

	/// A proposal is a `configuration` carrying the document whole, `verify`, and no capabilities,
	/// with the document critical on the wire so a device that cannot read it does not act on it
	/// (CFG).
	#[test]
	fn a_proposal_carries_the_document_critical() {
		let bytes = proposal(
			r#"{"attachments":[],"regulatory-domain":"VU","later":{"kept":1}}"#,
			true,
		)
		.unwrap();
		let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
		assert_eq!(wire["type"], "configuration");
		assert_eq!(wire["DOCUMENT"]["regulatory-domain"], "VU");
		assert_eq!(wire["verify"], true);
		assert!(wire.get("capabilities").is_none());

		let Ok(Reading::Message(Message::Configuration {
			document,
			capabilities,
			verify,
		})) = read(&bytes)
		else {
			panic!("a proposal reads back as a configuration");
		};
		assert_eq!(capabilities, None);
		assert_eq!(verify, Some(true));
		// A member this build does not know is sent as the operator's document carried it.
		assert_eq!(document["later"]["kept"], 1);
	}

	/// The pre-proposal check is the core's, naming the first member capabilities do not cover.
	#[test]
	fn the_capability_check_names_what_is_not_covered() {
		let capabilities = r#"{"attachments":{"kind":{"wired-dynamic":{"interface":["eth0"]}}}}"#;
		assert_eq!(
			capability_fault(
				r#"{"attachments":[{"kind":"wired-dynamic","label":"a","verify":true,"interface":"eth0"}]}"#,
				capabilities
			),
			Ok(None)
		);
		let fault = capability_fault(
			r#"{"attachments":[{"kind":"wired-dynamic","label":"a","verify":true,"interface":"eth1"}]}"#,
			capabilities,
		)
		.unwrap()
		.unwrap();
		assert_eq!(fault["at"], "$['attachments'][0]['interface']");
		assert!(fault["reason"].is_string());
		assert!(capability_fault("[]", capabilities).is_err());
	}

	#[test]
	fn a_proposal_is_an_object() {
		assert!(proposal("[]", true).is_err());
		assert!(proposal("not json", true).is_err());
	}
}
