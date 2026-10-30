//! The browser client for bliti: the protocol half of the web application (WEB).
//!
//! This crate compiles to wasm and carries everything the specs describe — reading a QR code
//! (QR), computing and matching the advertised handle (ADV), the `NXpsk0` handshake, the
//! stream layer, and the JSON messages (CHN). It is the same code the daemon and the
//! command-line client run, which is the point: one implementation of the key schedule and the
//! handshake rather than a Rust one and a JavaScript one that must agree forever.
//!
//! What stays in JavaScript is Web Bluetooth, the camera, and the interface. Those are browser APIs
//! with no protocol in them, and binding them through wasm would buy nothing. Decoding what the
//! camera sees is here, for a browser with no QR detector of its own.
//!
//! The memory-hard derivation of KEY never runs here: a client reads the presence token and the key
//! fingerprint from the payload and computes only the pre-shared key, the handle and the
//! fingerprint, which are fast derivations. The crate therefore takes `bliti-core` without its
//! default features, and argon2 is not in the build at all.

use std::{cell::RefCell, rc::Rc};

use bliti_core::{
	advertisement::{Advertised, Heard, local_names},
	channel::{
		capabilities,
		envelope::{Reading, Refusal, read},
		messages::Message,
		stream::{
			Mode, Opener, connect_initiator, connect_initiator_unauthenticated, multiplex,
			write_message,
		},
	},
	provisioning,
	qr::QrPayload,
};
use futures::channel::{mpsc, oneshot};
use js_sys::{Function, JSON, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

use streams::report_until_closed;
use transport::WebTransport;

mod provision;
mod scan;
mod sheet;
mod streams;
#[cfg(test)]
mod tests;
mod transport;

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

/// The characteristic a client reads to be given its slot (CHN, "Transport").
#[wasm_bindgen]
pub fn allocation_uuid() -> String {
	bliti_core::slot::CHARACTERISTIC_UUID_ALLOCATION.to_string()
}

/// What the allocation characteristic answered, read: the characteristic the device notifies this
/// client on and the most one write may carry, or null where the device has no slot free (CHN,
/// "Transport").
#[wasm_bindgen]
pub fn read_allocation(allocation: &[u8]) -> Result<Option<Allocation>, JsError> {
	use bliti_core::slot::Allocated;
	match Allocated::from_value(allocation).map_err(|why| JsError::new(&why.to_string()))? {
		Allocated::Given { slot, mtu } => Ok(Some(Allocation {
			slot: slot.uuid().to_string(),
			chunk: write_chunk(mtu),
		})),
		Allocated::Full => Ok(None),
	}
}

/// The largest value Web Bluetooth writes to a characteristic.
const WEB_BLUETOOTH_MAX_WRITE: usize = 512;

/// The most one write carries on a link whose ATT_MTU is `mtu`, as far as Web Bluetooth allows.
fn write_chunk(mtu: u16) -> usize {
	bliti_core::slot::payload(mtu.into()).min(WEB_BLUETOOTH_MAX_WRITE)
}

/// A slot the device gave this client.
#[wasm_bindgen]
pub struct Allocation {
	slot: String,
	chunk: usize,
}

#[wasm_bindgen]
impl Allocation {
	/// The characteristic the device notifies this client on.
	#[wasm_bindgen(getter)]
	pub fn slot(&self) -> String {
		self.slot.clone()
	}

	/// The most one write carries on this client's link, for [`Channel::new`].
	#[wasm_bindgen(getter)]
	pub fn chunk(&self) -> usize {
		self.chunk
	}
}

/// The QR code a phone joins a device's hotspot by, as an SVG image for inlining in the page (VIEW).
#[wasm_bindgen]
pub fn hotspot_qr(ssid: &str, passphrase: &str) -> Result<String, JsError> {
	bliti_core::wifi_qr::svg(ssid, passphrase).map_err(|why| JsError::new(&why.to_string()))
}

/// The QR code for any text, as an SVG image for inlining in the page: a Tailscale login link's
/// (TSCR).
#[wasm_bindgen]
pub fn text_qr(text: &str) -> Result<String, JsError> {
	bliti_core::text_qr::svg(text).map_err(|why| JsError::new(&why.to_string()))
}

/// The PDF sheet of labels for `labels`, a JSON array of `{ payload, version }` in the order they are
/// to print, as the bytes of the file (PSHT).
#[wasm_bindgen]
pub fn sheet_pdf(labels: &str) -> Result<Vec<u8>, JsError> {
	let labels = sheet::parse(labels).map_err(|why| JsError::new(&why))?;
	sheet::pdf(&labels).map_err(|why| JsError::new(&why))
}

/// How full the sheet of `count` labels is, worded for the action that downloads it (PSHT).
#[wasm_bindgen]
pub fn sheet_fill(count: usize) -> String {
	sheet::fill(count)
}

/// Whether a document is within a device's capabilities, checked before it is proposed (NSCR) with
/// the checker the device rejects with (NET), so the two cannot disagree.
///
/// Both are plain objects, the capabilities whole. Null where they admit the document, or
/// `{ at, reason, rule }` naming the first part they do not: `rule` is `mirror` where
/// `capabilities.document` does not cover it, or the rule of HOT it breaks. The reason is the
/// checker's, for logs; a screen words its own.
#[wasm_bindgen]
pub fn check_capabilities(document: JsValue, capabilities: JsValue) -> Result<JsValue, JsError> {
	across(&document, &capabilities, capability_fault)
}

/// Whether a document's hotspot and a wireless candidate could be carried only by one radio running
/// one at a time (HOT), alone, so a screen can ask it of a document it is considering. Null where
/// not, or `{ at, reason }`.
#[wasm_bindgen]
pub fn check_placement(document: JsValue, capabilities: JsValue) -> Result<JsValue, JsError> {
	across(&document, &capabilities, placement_fault)
}

/// Where a document's hotspot chooses its own band, channel and width, and where it follows a
/// wireless client's channel (HOT), by the rule the device holds a document to: `{ own, follows }`,
/// each a list of interfaces.
#[wasm_bindgen]
pub fn hotspot_channels(document: JsValue, capabilities: JsValue) -> Result<JsValue, JsError> {
	across(&document, &capabilities, channel_choice)
}

/// Call `answer` on a document and capabilities given as plain objects, and hand back what it
/// answers as one, or null for nothing.
fn across(
	document: &JsValue,
	capabilities: &JsValue,
	answer: fn(&str, &str) -> Result<Option<serde_json::Value>, String>,
) -> Result<JsValue, JsError> {
	let text = |value: &JsValue, what: &str| {
		JSON::stringify(value)
			.map(String::from)
			.map_err(|_| JsError::new(&format!("the {what} cannot be written as JSON")))
	};
	let answer = answer(
		&text(document, "document")?,
		&text(capabilities, "capabilities")?,
	)
	.map_err(|why| JsError::new(&why))?;
	match answer {
		Some(answer) => JSON::parse(&answer.to_string())
			.map_err(|_| JsError::new("the answer cannot be read back as JSON")),
		None => Ok(JsValue::NULL),
	}
}

type Object = serde_json::Map<String, serde_json::Value>;

/// The document and capabilities, each a JSON object.
fn objects(document: &str, capabilities: &str) -> Result<(Object, Object), String> {
	let object = |json: &str, what: &str| match serde_json::from_str(json) {
		Ok(serde_json::Value::Object(map)) => Ok(map),
		Ok(_) => Err(format!("the {what} is not an object")),
		Err(err) => Err(format!("the {what} is not JSON: {err}")),
	};
	Ok((
		object(document, "document")?,
		object(capabilities, "capabilities")?,
	))
}

/// The fault [`check_capabilities`] reports, from the document and capabilities as JSON text.
fn capability_fault(
	document: &str,
	capabilities: &str,
) -> Result<Option<serde_json::Value>, String> {
	let (document, capabilities) = objects(document, capabilities)?;
	Ok(capabilities::admits(&document, &capabilities)
		.err()
		.map(|refused| {
			serde_json::json!({
				"at": refused.invalid.at,
				"reason": refused.invalid.reason,
				"rule": refused.rule.as_str(),
			})
		}))
}

/// What [`hotspot_channels`] answers, from the document and capabilities as JSON text.
fn channel_choice(document: &str, capabilities: &str) -> Result<Option<serde_json::Value>, String> {
	let (document, capabilities) = objects(document, capabilities)?;
	let choice = capabilities::channel_choice(&document, &capabilities);
	Ok(Some(
		serde_json::json!({ "own": choice.own, "follows": choice.follows }),
	))
}

/// The fault [`check_placement`] reports, from the document and capabilities as JSON text.
fn placement_fault(
	document: &str,
	capabilities: &str,
) -> Result<Option<serde_json::Value>, String> {
	let (document, capabilities) = objects(document, capabilities)?;
	Ok(capabilities::placement(&document, &capabilities)
		.err()
		.map(|invalid| serde_json::json!({ "at": invalid.at, "reason": invalid.reason })))
}

/// A QR code the application has read, from the camera or typed in (WEB).
#[wasm_bindgen]
pub struct QrCode {
	payload: QrPayload,
}

#[wasm_bindgen]
impl QrCode {
	/// Read a QR code from its text, as captured from the code or entered by the operator (QR,
	/// "Reading").
	#[wasm_bindgen(constructor)]
	pub fn new(text: &str) -> Result<QrCode, JsError> {
		use bliti_core::qr::QrError;
		let payload = QrPayload::read(text).map_err(|err| match err {
			QrError::UnsupportedVersion(version) => JsError::new(&format!(
				"That code is Tamanu Iti payload version {version}, which this app does not read."
			)),
			QrError::Malformed => JsError::new("This isn't a Tamanu Iti code."),
		})?;
		Ok(Self { payload })
	}

	/// The payload version of the QR code in hand.
	#[wasm_bindgen(getter)]
	pub fn version(&self) -> u8 {
		self.payload.version()
	}

	/// The text the QR code encodes.
	#[wasm_bindgen(getter)]
	pub fn text(&self) -> String {
		self.payload.to_text()
	}

	/// The last characters of the encoded payload, which name the device without giving away
	/// anything secret (WEB, "Exporting the QR code").
	#[wasm_bindgen(getter)]
	pub fn suffix(&self) -> String {
		self.payload.suffix()
	}

	/// The QR code as the SVG image a generator produces for it, for printing a replacement.
	#[wasm_bindgen(getter)]
	pub fn svg(&self) -> String {
		self.payload.to_svg()
	}

	/// The local names the device this QR code belongs to may advertise, one for each version marker
	/// this build considers for the code, highest first. Known before anything is heard, so the
	/// chooser is filtered on them and the operator told the first (WEB, "Finding the device").
	#[wasm_bindgen(getter)]
	pub fn local_names(&self) -> Vec<String> {
		local_names(&self.payload)
	}

	/// Read a local name heard over the air against this QR code (ADV, "Matching").
	///
	/// `undefined` where the name is not a bliti payload at all, which is the ordinary case for every
	/// other device in range and is passed over rather than reported.
	pub fn read_local_name(&self, name: &str) -> Option<Advertisement> {
		Advertised::from_local_name(name).map(|advertised| Advertisement {
			version: advertised.version,
			heard: advertised.heard_by(&self.payload),
		})
	}
}

/// What a client made of one device's advertisement.
#[wasm_bindgen]
pub struct Advertisement {
	version: u8,
	heard: Heard,
}

#[wasm_bindgen]
impl Advertisement {
	/// The version marker the device advertises.
	#[wasm_bindgen(getter)]
	pub fn version(&self) -> u8 {
		self.version
	}

	/// Whether this client implements the version marker the device advertises. Where it does not,
	/// whether the device matches cannot be said (VER).
	#[wasm_bindgen(getter)]
	pub fn supported(&self) -> bool {
		!matches!(self.heard, Heard::Unsupported(_))
	}

	/// Whether this device is the one the QR code belongs to.
	#[wasm_bindgen(getter)]
	pub fn matches(&self) -> bool {
		self.heard == Heard::Matches
	}
}

/// The state a channel holds once it is open. Kept behind a shared handle so the exported methods can
/// hand work to a task without borrowing across an await.
struct Inner {
	// The QR code of the device this channel is for, or nothing for a channel to a device in
	// provisioning mode, which has no key fingerprint to check (PRV).
	payload: Option<QrPayload>,
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

impl Channel {
	fn with(payload: Option<QrPayload>, write: Function, chunk: usize) -> Channel {
		let (transport, inbound) = WebTransport::new(write, chunk);
		Channel {
			inner: Rc::new(Inner {
				payload,
				transport: RefCell::new(Some(transport)),
				inbound: RefCell::new(inbound),
				opener: RefCell::new(None),
				feed_closers: RefCell::new(Vec::new()),
			}),
		}
	}
}

#[wasm_bindgen]
impl Channel {
	/// Prepare a channel for a device, given the function that writes a chunk to the device's write
	/// characteristic and the most one chunk may carry, as the allocation gave it. The handshake does
	/// not run until [`Channel::connect`] is called.
	#[wasm_bindgen(constructor)]
	pub fn new(code: &QrCode, write: Function, chunk: usize) -> Channel {
		Channel::with(Some(code.payload.clone()), write, chunk)
	}

	/// Prepare a channel for a device in provisioning mode, which no QR code anchors: the handshake
	/// is keyed by the provisioning token and compares no fingerprint (PRV, "The provisioning token").
	/// Otherwise as [`Channel::new`].
	pub fn for_provisioning(write: Function, chunk: usize) -> Channel {
		Channel::with(None, write, chunk)
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

			let encrypted = match &inner.payload {
				Some(payload) => {
					connect_initiator(
						transport,
						&payload.presence_token().pre_shared_key(),
						payload.fingerprint(),
					)
					.await
				}
				None => {
					connect_initiator_unauthenticated(transport, &provisioning::pre_shared_key())
						.await
				}
			}
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
}

/// Describe one message to the application as JSON, and say whether it was a fault.
///
/// The three outcomes of MSG are kept apart here rather than in the application, so every client
/// surface inherits the same reading of the wire. `message` is what this build understood, `skipped`
/// is a device newer than this build saying something safe to pass over, `refused` is one saying
/// something that must not be half read, and `fault` is a device not speaking the protocol. A
/// refusal names the message type it refused, so the application can say so once per type (STAT).
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
		Ok(Reading::Refused(refusal)) => {
			let type_name = match &refusal {
				Refusal::CriticalType(type_name) | Refusal::CriticalMembers { type_name, .. } => {
					type_name
				}
			};
			(
				serde_json::json!({
					"kind": "refused",
					"type": type_name,
					"detail": refusal.to_string(),
				})
				.to_string(),
				None,
			)
		}
		Err(fault) => (
			serde_json::json!({ "kind": "fault", "detail": fault.to_string() }).to_string(),
			Some(fault.to_string()),
		),
	}
}
