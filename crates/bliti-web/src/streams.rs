//! The streams a client opens on a channel, and the handles the application drives them through:
//! subscriptions (MSG), and the exchanges both ends speak on, for configuration (CFG), power (CTL),
//! curves (CRV), Tailscale (TSC) and Tamanu (TMU).

use std::cell::RefCell;

use bliti_core::channel::{
	messages::{Message, Secret},
	stream::{Stream, read_message, write_message},
};
use futures::{
	AsyncRead, AsyncReadExt, AsyncWriteExt, StreamExt,
	channel::{mpsc, oneshot},
	future::{self, Either},
};
use js_sys::{Function, JSON, Promise};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

use crate::{Channel, Inner, describe};

#[wasm_bindgen]
impl Channel {
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
	/// the client's half of the exchange on the same stream, and closing it ends the session, which
	/// changes nothing the device holds.
	pub fn configure(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let exchange = Exchange::open(
				&inner,
				Message::Configure,
				"a configuration session",
				on_message,
				on_closed,
			)
			.await?;
			Ok(JsValue::from(ConfigurationHandle { exchange }))
		})
	}

	/// Open a power stream: a stream whose first message is `power` (CTL).
	///
	/// Everything the device sends on it is passed to `on_message` as a subscription's messages are,
	/// and `on_closed` is called once the stream ends. The handle this resolves to asks for acts on
	/// the same stream.
	pub fn power(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let exchange = Exchange::open(
				&inner,
				Message::Power,
				"a power stream",
				on_message,
				on_closed,
			)
			.await?;
			Ok(JsValue::from(PowerHandle { exchange }))
		})
	}

	/// Open a curve stream: a stream whose first message is `curve` (CRV).
	///
	/// Everything the device sends on it is passed to `on_message` as a subscription's messages are,
	/// and `on_closed` is called once the stream ends. The handle this resolves to asks for a load or
	/// a reset on the same stream.
	pub fn curve(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let exchange = Exchange::open(
				&inner,
				Message::Curve,
				"a curve stream",
				on_message,
				on_closed,
			)
			.await?;
			Ok(JsValue::from(CurveHandle { exchange }))
		})
	}

	/// Open a Tailscale stream: a stream whose first message is `tailscale` (TSC).
	///
	/// Everything the device sends on it is passed to `on_message` as a subscription's messages are,
	/// and `on_closed` is called once the stream ends. The handle this resolves to asks for acts on
	/// the same stream.
	pub fn tailscale(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let exchange = Exchange::open(
				&inner,
				Message::Tailscale,
				"a Tailscale stream",
				on_message,
				on_closed,
			)
			.await?;
			Ok(JsValue::from(TailscaleHandle { exchange }))
		})
	}

	/// Open a Tamanu stream: a stream whose first message is `tamanu` (TMU).
	///
	/// Everything the device sends on it is passed to `on_message` as a subscription's messages are,
	/// and `on_closed` is called once the stream ends. The handle this resolves to asks for acts on
	/// the same stream.
	pub fn tamanu(&self, on_message: Function, on_closed: Function) -> Promise {
		let inner = self.inner.clone();
		future_to_promise(async move {
			let exchange = Exchange::open(
				&inner,
				Message::Tamanu,
				"a Tamanu stream",
				on_message,
				on_closed,
			)
			.await?;
			Ok(JsValue::from(TamanuHandle { exchange }))
		})
	}
}

/// A stream the client opened and both ends then speak on: read and written at once, until either
/// end closes it.
struct Exchange {
	outbound: RefCell<Option<mpsc::UnboundedSender<Vec<u8>>>>,
	closer: RefCell<Option<oneshot::Sender<()>>>,
	/// What the stream is, for an error to name.
	what: &'static str,
}

impl Exchange {
	/// Open a stream whose first message is `first`, and report everything the device sends on it.
	async fn open(
		inner: &Inner,
		first: Message,
		what: &'static str,
		on_message: Function,
		on_closed: Function,
	) -> Result<Self, JsError> {
		let opener = inner
			.opener
			.borrow()
			.clone()
			.ok_or_else(|| JsError::new("this channel is not connected"))?;
		let mut stream = opener
			.open()
			.await
			.map_err(|err| JsError::new(&format!("opening {what}: {err}")))?;
		write_message(&mut stream, &first.to_json())
			.await
			.map_err(|err| JsError::new(&format!("opening {what}: {err}")))?;

		// The stream is read and written at once, so it is split: a read is pending essentially
		// always, and a write must not wait on it.
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
			// However the read ended, the stream has, so the write half closes too.
			ending.close_channel();
			call_closed(&on_closed, ended);
		});

		Ok(Self {
			outbound: RefCell::new(Some(outbound)),
			closer: RefCell::new(Some(closer)),
			what,
		})
	}

	fn send(&self, message: Vec<u8>) -> Result<(), JsError> {
		self.outbound
			.borrow()
			.as_ref()
			.and_then(|outbound| outbound.unbounded_send(message).ok())
			.ok_or_else(|| JsError::new(&format!("{} has ended", self.what)))
	}

	/// End the stream. Closed rather than dropped, for the reason [`SubscriptionHandle::close`] gives.
	fn close(&self) {
		self.outbound.borrow_mut().take();
		if let Some(closer) = self.closer.borrow_mut().take() {
			let _ = closer.send(());
		}
	}
}

/// One open configuration session, which lasts exactly as long as its stream (CFG).
#[wasm_bindgen]
pub struct ConfigurationHandle {
	exchange: Exchange,
}

#[wasm_bindgen]
impl ConfigurationHandle {
	/// Propose a document: the whole configuration the client wants in force, as a plain object.
	pub fn propose(&self, document: JsValue) -> Result<(), JsError> {
		let json = JSON::stringify(&document)
			.map_err(|_| JsError::new("the document cannot be written as JSON"))?;
		let message = proposal(&String::from(json)).map_err(|why| JsError::new(&why))?;
		self.send(message)
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

	/// End the session, which changes nothing the device holds (CFG).
	pub fn close(&self) {
		self.exchange.close();
	}

	fn send(&self, message: Vec<u8>) -> Result<(), JsError> {
		self.exchange.send(message)
	}
}

/// One open power stream, which lasts exactly as long as its stream (CTL).
#[wasm_bindgen]
pub struct PowerHandle {
	exchange: Exchange,
}

#[wasm_bindgen]
impl PowerHandle {
	/// Ask the device to carry out an act, by its wire name.
	pub fn act(&self, act: String) -> Result<(), JsError> {
		self.exchange.send(Message::Act { act }.to_json())
	}

	/// End the power stream.
	pub fn close(&self) {
		self.exchange.close();
	}
}

/// One open curve stream, which lasts exactly as long as its stream (CRV).
#[wasm_bindgen]
pub struct CurveHandle {
	exchange: Exchange,
}

#[wasm_bindgen]
impl CurveHandle {
	/// Ask the device to load a curve document, given as it was read from a file.
	pub fn load(&self, document: JsValue) -> Result<(), JsError> {
		let json = JSON::stringify(&document)
			.map_err(|_| JsError::new("the document cannot be written as JSON"))?;
		let message = loading(&String::from(json)).map_err(|why| JsError::new(&why))?;
		self.exchange.send(message)
	}

	/// Ask the device to return to the curve its build carries.
	pub fn reset(&self) -> Result<(), JsError> {
		self.exchange.send(Message::Reset.to_json())
	}

	/// End the curve stream.
	pub fn close(&self) {
		self.exchange.close();
	}
}

/// One open Tailscale stream, which lasts exactly as long as its stream (TSC).
#[wasm_bindgen]
pub struct TailscaleHandle {
	exchange: Exchange,
}

#[wasm_bindgen]
impl TailscaleHandle {
	/// Ask the device to carry out an act, by its wire name, with the members the act takes: how a
	/// `sign-in` is done and the auth key and name it uses, or the machine a `ping` tests. A member
	/// left out or empty is not sent.
	pub fn act(
		&self,
		act: String,
		method: Option<String>,
		key: Option<String>,
		name: Option<String>,
		peer: Option<String>,
	) -> Result<(), JsError> {
		self.exchange
			.send(tailscale_act(act, method, key, name, peer).to_json())
	}

	/// End the Tailscale stream.
	pub fn close(&self) {
		self.exchange.close();
	}
}

/// The `tailscale-act` asking for `act`, leaving out every member not given or given empty.
pub(crate) fn tailscale_act(
	act: String,
	method: Option<String>,
	key: Option<String>,
	name: Option<String>,
	peer: Option<String>,
) -> Message {
	let given = |member: Option<String>| member.filter(|value| !value.is_empty());
	Message::TailscaleAct {
		act,
		method: given(method),
		key: given(key).map(Secret::new),
		name: given(name),
		peer: given(peer),
	}
}

/// One open Tamanu stream, which lasts exactly as long as its stream (TMU).
#[wasm_bindgen]
pub struct TamanuHandle {
	exchange: Exchange,
}

#[wasm_bindgen]
impl TamanuHandle {
	/// Ask the device to carry out an act, by its wire name.
	pub fn act(&self, act: String) -> Result<(), JsError> {
		self.exchange.send(Message::TamanuAct { act }.to_json())
	}

	/// End the Tamanu stream.
	pub fn close(&self) {
		self.exchange.close();
	}
}

/// The `load` message carrying a curve document, from the document as JSON text. The document is
/// sent as it stands: the device is the one to validate it, and refuses with a reason (CRV).
pub(crate) fn loading(json: &str) -> Result<Vec<u8>, String> {
	serde_json::from_str(json)
		.map(|document| Message::Load { document }.to_json())
		.map_err(|err| format!("the document is not JSON: {err}"))
}

/// The `configuration` message proposing a document, from the document as JSON text.
pub(crate) fn proposal(json: &str) -> Result<Vec<u8>, String> {
	match serde_json::from_str(json) {
		Ok(serde_json::Value::Object(document)) => Ok(Message::Configuration {
			document,
			capabilities: None,
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
pub(crate) async fn report_until_closed(
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
