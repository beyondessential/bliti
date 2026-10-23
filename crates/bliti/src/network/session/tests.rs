//! The configuration session of CFG, driven over an in-memory stream against a fake backend.

use std::time::Duration;

use bliti_core::channel::{
	config::{Document, Failure, Invalid, Outcome, Segment, path},
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::AsyncWriteExt;
use serde_json::{Map, Value as Json, json};
use tokio::{io::DuplexStream, task::JoinHandle};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use self::fake::{Applying, Call, Log, capabilities, fake, fake_observing};
use super::{
	Configurator, Store, serve,
	store::{
		Stored,
		tests::{Scratch, object, stored},
	},
};
use crate::network::select::Stage;

mod capabilities;
mod fake;
mod holding;
mod start;
mod state;

/// The configuration the tests start from: a wall port, and a member a newer client added.
fn recorded() -> Map<String, Json> {
	object(json!({
		"attachments": [{"kind": "wired-dynamic", "label": "Wall port", "enabled": true, "verify": true, "interface": "eth0"}],
		"x-added-by-a-newer-client": {"kept": true},
	}))
}

/// A proposal: the clinic's wireless network ahead of the wall port.
fn proposal() -> Map<String, Json> {
	object(json!({
		"attachments": [
			{"kind": "wireless", "label": "Clinic", "enabled": true, "verify": true, "ssid": "Clinic",
			 "security": {"kind": "psk", "passphrase": "correct horse"}},
			{"kind": "wired-dynamic", "label": "Wall port", "enabled": true, "verify": true, "interface": "eth0"},
		],
		"regulatory-domain": "NZ",
	}))
}

/// A different proposal: the same, in another regulatory domain.
fn another() -> Map<String, Json> {
	let mut another = proposal();
	another.insert("regulatory-domain".to_owned(), json!("AU"));
	another
}

/// Where the passphrase of the proposal's wireless candidate sits.
fn passphrase() -> String {
	path(&[
		Segment::Name("attachments"),
		Segment::Index(0),
		Segment::Name("security"),
		Segment::Name("passphrase"),
	])
}

/// The failure of the proposal's wireless candidate being refused its key.
fn refused_key() -> Failure {
	Stage::Association.failed(passphrase(), "the access point refused the key (reason 15)")
}

fn parsed(raw: &Map<String, Json>) -> Document {
	Document::parse(raw).unwrap()
}

/// A held configuration whose verification ended well.
fn settled(document: Map<String, Json>) -> Stored {
	stored(Json::Object(document))
}

/// The `state` of a device observing nothing, as it opens.
fn state(outcome: Outcome) -> Message {
	Message::State {
		busy: false,
		outcome,
		failure: None,
		attachments: Vec::new(),
		capabilities: None,
	}
}

/// What a `state` carries beyond its outcome, set on the one `state` makes.
trait Reported {
	fn busy(self) -> Self;
	fn failing(self, failure: impl Into<Option<Failure>>) -> Self;
	fn entries(self, entries: Vec<Json>) -> Self;
	fn told(self, capabilities: Map<String, Json>) -> Self;
}

impl Reported for Message {
	fn busy(self) -> Self {
		match self {
			Self::State {
				outcome,
				failure,
				attachments,
				capabilities,
				..
			} => Self::State {
				busy: true,
				outcome,
				failure,
				attachments,
				capabilities,
			},
			other => other,
		}
	}

	fn failing(self, failure: impl Into<Option<Failure>>) -> Self {
		match self {
			Self::State {
				busy,
				outcome,
				attachments,
				capabilities,
				..
			} => Self::State {
				busy,
				outcome,
				failure: failure.into(),
				attachments,
				capabilities,
			},
			other => other,
		}
	}

	fn entries(self, entries: Vec<Json>) -> Self {
		match self {
			Self::State {
				busy,
				outcome,
				failure,
				capabilities,
				..
			} => Self::State {
				busy,
				outcome,
				failure,
				attachments: entries,
				capabilities,
			},
			other => other,
		}
	}

	fn told(self, capabilities: Map<String, Json>) -> Self {
		match self {
			Self::State {
				busy,
				outcome,
				failure,
				attachments,
				..
			} => Self::State {
				busy,
				outcome,
				failure,
				attachments,
				capabilities: Some(capabilities),
			},
			other => other,
		}
	}
}

type Client = Compat<DuplexStream>;

/// A device with a fake backend and a held configuration on disk.
struct Device {
	configurator: Configurator,
	log: Log,
	scratch: Scratch,
}

impl Device {
	async fn new() -> Self {
		Self::holding(settled(recorded()), false).await
	}

	/// A device whose backend publishes the state of its candidates.
	async fn observing() -> Self {
		Self::holding(settled(recorded()), true).await
	}

	/// A device that starts on what was recorded.
	async fn holding(held: Stored, observing: bool) -> Self {
		let scratch = Scratch::new();
		scratch.store().record(&held).await.unwrap();
		let (backend, log) = fake_observing(observing);
		let configurator =
			Configurator::start(backend, scratch.store(), object(json!({"attachments": []})))
				.await
				.unwrap();
		log.clear();
		Self {
			configurator,
			log,
			scratch,
		}
	}

	fn store(&self) -> Store {
		self.scratch.store()
	}

	/// What a daemon started over the same disk does, and what it was asked to.
	async fn restarted(&self) -> (Configurator, Log) {
		let (backend, log) = fake();
		let configurator =
			Configurator::start(backend, self.store(), object(json!({"attachments": []})))
				.await
				.unwrap();
		(configurator, log)
	}

	/// Open a session on this device.
	async fn open(&self) -> (Client, JoinHandle<()>) {
		open(self.configurator.clone()).await
	}

	/// Open a session and take its opening `configuration` and `state`.
	async fn opened(&self) -> (Client, JoinHandle<()>, Map<String, Json>) {
		let (mut client, task) = self.open().await;
		let Message::Configuration { document, .. } = recv(&mut client).await else {
			panic!("a session opens with the configuration");
		};
		assert!(
			matches!(recv(&mut client).await, Message::State { .. }),
			"and then its state"
		);
		(client, task, document)
	}
}

/// Open a session as a client would: a stream whose first message is `configure`, served the way
/// the dispatcher serves it once it has read that message.
async fn open(configurator: Configurator) -> (Client, JoinHandle<()>) {
	let (client, device) = tokio::io::duplex(1 << 16);
	let mut client = client.compat();
	let mut device = device.compat();
	write_message(&mut client, &Message::Configure.to_json())
		.await
		.unwrap();
	let task = tokio::spawn(async move {
		let first = read_message(&mut device).await.unwrap().unwrap();
		assert_eq!(
			read::<Message>(&first).unwrap(),
			Reading::Message(Message::Configure)
		);
		let _ = serve(&mut device, &configurator).await;
	});
	(client, task)
}

async fn send(client: &mut Client, message: Message) {
	write_message(client, &message.to_json()).await.unwrap();
}

async fn propose(client: &mut Client, document: Map<String, Json>) {
	send(
		client,
		Message::Configuration {
			document,
			capabilities: None,
		},
	)
	.await;
}

async fn recv(client: &mut Client) -> Message {
	let raw = tokio::time::timeout(Duration::from_secs(5), read_message(client))
		.await
		.expect("the device answered")
		.unwrap()
		.expect("the stream is open");
	match read::<Message>(&raw).unwrap() {
		Reading::Message(message) => message,
		other => panic!("the device sent something unreadable: {other:?}"),
	}
}

/// The `configuration` the device sends next, and the `state` after it.
async fn recv_held(client: &mut Client) -> (Map<String, Json>, Message) {
	let Message::Configuration { document, .. } = recv(client).await else {
		panic!("expected the configuration held");
	};
	(document, recv(client).await)
}

/// Whether the device sends nothing for a moment.
async fn quiet(client: &mut Client) -> bool {
	tokio::time::timeout(Duration::from_millis(100), read_message(client))
		.await
		.is_err()
}

/// The `state` the device sends once it carries `outcome`, passing over any before it. A verification
/// that ends as it is saved may be told only by its outcome.
async fn until_outcome(client: &mut Client, outcome: Outcome) -> Message {
	loop {
		match recv(client).await {
			Message::State { outcome: now, .. } if now != outcome => {}
			other => return other,
		}
	}
}

/// Propose and take what the device saving it sends: the proposal back, then `state` verifying.
async fn proposed(client: &mut Client, document: Map<String, Json>) {
	propose(client, document.clone()).await;
	let (echoed, verifying) = recv_held(client).await;
	assert_eq!(echoed, document, "saved: sent back unchanged");
	assert_eq!(verifying, state(Outcome::Verifying));
}

#[tokio::test]
async fn opening_a_session_sends_the_configuration_held_and_the_capabilities_then_state() {
	let device = Device::new().await;
	let (mut client, _task) = device.open().await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: recorded(),
			capabilities: Some(capabilities()),
		},
		"the held document is echoed raw, with the member this build does not know"
	);
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
	assert!(quiet(&mut client).await);
}

#[tokio::test]
async fn configure_asked_again_is_answered_with_the_configuration_and_the_capabilities_whole() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, Message::Configure).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: recorded(),
			capabilities: Some(capabilities()),
		}
	);
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
}

#[tokio::test]
async fn a_saved_proposal_is_echoed_then_verifying_then_its_outcome() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	let mut sent = proposal();
	sent.insert("x-added-by-this-client".to_owned(), json!(1));
	proposed(&mut client, sent.clone()).await;

	// Recorded before any of it was applied, with what it replaced, so a restart can judge it again.
	assert_eq!(
		device.store().load().unwrap(),
		Some(Stored {
			outcome: Outcome::Verifying,
			replaced: Some(recorded()),
			..settled(sent.clone())
		}),
		"recorded raw, as the client sent it"
	);
	device
		.log
		.until(|calls| matches!(calls, [Call::Apply(..)]))
		.await;
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&sent), parsed(&recorded()))]
	);

	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
	assert_eq!(
		device.store().load().unwrap(),
		Some(settled(sent)),
		"settled: nothing replaced is kept"
	);
}

#[tokio::test]
async fn writing_the_configuration_back_unmodified_is_saved_and_verified_again() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, document) = device.opened().await;
	proposed(&mut client, document).await;
	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&recorded()), parsed(&recorded()))],
		"nothing differs, so nothing is judged"
	);
}

#[tokio::test]
async fn a_refused_proposal_is_answered_cannot_configure_then_the_configuration_unchanged() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// Structurally invalid: a static candidate with no gateway (LINK).
	propose(
		&mut client,
		object(json!({"attachments": [
			{"kind": "wired-static", "label": "Site A", "enabled": true, "verify": true, "interface": "eth0",
			 "addresses": ["10.0.0.5/24"]},
		]})),
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::CannotConfigure {
			at: "$['attachments'][0]['gateway']".to_owned(),
			reason: "a static candidate carries a gateway".to_owned(),
		}
	);
	let (held, reported) = recv_held(&mut client).await;
	assert_eq!(held, recorded(), "the configuration held, unchanged");
	assert_eq!(reported, state(Outcome::Applied));

	// Outside the capabilities, found by the backend's check (NET).
	device.log.refuse(Invalid::at(
		path(&[Segment::Name("hotspot")]),
		"this radio cannot run a hotspot",
	));
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::CannotConfigure {
			at: path(&[Segment::Name("hotspot")]),
			reason: "this radio cannot run a hotspot".to_owned(),
		}
	);
	let (held, _) = recv_held(&mut client).await;
	assert_eq!(held, recorded());

	assert_eq!(device.log.calls(), [], "nothing of either was applied");
	assert_eq!(
		device.store().load().unwrap(),
		Some(settled(recorded())),
		"and nothing was recorded"
	);
}

#[tokio::test]
async fn a_proposal_that_cannot_be_recorded_is_refused_and_applied_nowhere() {
	let scratch = Scratch::new();
	std::fs::create_dir_all(&scratch.0).unwrap();
	let blocked = scratch.0.join("file");
	std::fs::write(&blocked, b"").unwrap();
	let (backend, log) = fake();
	let configurator = Configurator::start(
		backend,
		Store::new(blocked.join("network.json")),
		recorded(),
	)
	.await
	.unwrap();
	log.clear();
	let device = Device {
		configurator,
		log,
		scratch,
	};
	let (mut client, _task, _) = device.opened().await;

	propose(&mut client, proposal()).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, reason }
			if at == "$" && reason.contains("could not record the configuration")
	));
	let (held, _) = recv_held(&mut client).await;
	assert_eq!(held, recorded());
	assert_eq!(device.log.calls(), []);
}

#[tokio::test]
async fn a_failed_proposal_stays_held_and_the_next_session_opens_on_it_with_the_failure() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;

	device.log.release(Err(refused_key()));
	assert_eq!(
		recv(&mut client).await,
		state(Outcome::Failed).failing(refused_key()),
		"carries the part at fault, the reason, and the stage it stopped at"
	);
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))],
		"nothing restores what the proposal replaced"
	);

	client.close().await.unwrap();
	task.await.unwrap();
	let (mut next, _task) = device.open().await;
	let (held, reported) = recv_held(&mut next).await;
	assert_eq!(held, proposal());
	assert_eq!(reported, state(Outcome::Failed).failing(refused_key()));
	assert_eq!(
		device.store().load().unwrap(),
		Some(Stored {
			outcome: Outcome::Failed,
			failure: Some(refused_key()),
			..settled(proposal())
		})
	);
}

#[tokio::test]
async fn a_failure_at_a_wired_candidate_carries_where_the_attempt_stopped() {
	let device = Device::new().await;
	device.log.set(Applying::Fail(Stage::Gateway.failed(
		path(&[Segment::Name("attachments"), Segment::Index(1)]),
		"192.168.1.1 did not answer",
	)));
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Configuration { document, .. } if document == proposal()
	));
	assert_eq!(
		until_outcome(&mut client, Outcome::Failed).await,
		state(Outcome::Failed).failing(Failure {
			at: "$['attachments'][1]".to_owned(),
			reason: "192.168.1.1 did not answer".to_owned(),
			reached: Some("gateway".to_owned()),
		})
	);
}

#[tokio::test]
async fn a_session_closed_mid_verification_leaves_it_verifying_and_the_next_sees_it_finish() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;

	client.close().await.unwrap();
	task.await.unwrap();
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))],
		"ending the session abandons nothing and restores nothing"
	);

	let (mut next, _task) = device.open().await;
	let (held, reported) = recv_held(&mut next).await;
	assert_eq!(held, proposal());
	assert_eq!(reported, state(Outcome::Verifying));

	device.log.release(Ok(()));
	assert_eq!(recv(&mut next).await, state(Outcome::Applied));
}

#[tokio::test]
async fn a_session_dropped_with_its_connection_leaves_the_configuration_held_and_running() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;

	// The connection's task is dropped with the session in it.
	task.abort();
	let _ = task.await;
	drop(client);
	device.log.release(Ok(()));

	let (mut next, _task) = device.open().await;
	let (held, _) = recv_held(&mut next).await;
	assert_eq!(held, proposal());
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))]
	);
}

#[tokio::test]
async fn a_newer_proposal_abandons_the_verification_of_the_one_before_it() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;
	device
		.log
		.until(|calls| matches!(calls, [Call::Apply(..)]))
		.await;

	proposed(&mut client, another()).await;
	assert_eq!(
		device.log.until(|calls| calls.len() == 3).await,
		[
			Call::Apply(parsed(&proposal()), parsed(&recorded())),
			Call::Aborted,
			Call::Apply(parsed(&another()), parsed(&recorded())),
		],
		"judged against what was last judged, as what the first added never was"
	);

	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
	assert_eq!(device.store().load().unwrap(), Some(settled(another())));
}

#[tokio::test]
async fn a_proposal_after_one_that_ended_is_judged_against_it() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;
	device.log.release(Err(refused_key()));
	assert!(matches!(
		recv(&mut client).await,
		Message::State {
			outcome: Outcome::Failed,
			..
		}
	));

	proposed(&mut client, another()).await;
	device.log.until(|calls| calls.len() == 2).await;
	assert_eq!(
		device.log.calls()[1],
		Call::Apply(parsed(&another()), parsed(&proposal())),
		"it replaces the failed one, whose candidates it leaves as they were"
	);
}

#[tokio::test]
async fn a_scan_is_served_while_a_verification_runs() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;

	send(&mut client, Message::Scan { interface: None }).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Networks {
			access_points: vec![json!({"interface": "wlan0", "ssid": "Clinic", "signal": -61})],
		}
	);
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))],
		"the verification was not abandoned"
	);
	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
}

#[tokio::test]
async fn scan_is_answered_with_networks() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, Message::Scan { interface: None }).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Networks {
			access_points: vec![json!({"interface": "wlan0", "ssid": "Clinic", "signal": -61})],
		}
	);

	// Naming an interface reaches the backend, which scans that radio alone.
	send(
		&mut client,
		Message::Scan {
			interface: Some("wlx00c0caa1b2c3".to_owned()),
		},
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Networks {
			access_points: vec![
				json!({"interface": "wlx00c0caa1b2c3", "ssid": "Clinic", "signal": -61})
			],
		}
	);
}

#[tokio::test]
async fn survey_is_answered_with_the_spectrum_or_cannot_act_where_unsupported() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, Message::Survey { interface: None }).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotAct { act, at, .. } if act == "survey" && at == "$"
	));

	let spectrum = object(json!({"channels": [{"number": 6, "busy": 0.4}]}));
	device.log.surveys(spectrum.clone());
	send(&mut client, Message::Survey { interface: None }).await;
	assert_eq!(recv(&mut client).await, Message::Spectrum { spectrum });
}

#[tokio::test]
async fn messages_only_a_device_sends_are_no_ops() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	for message in [
		state(Outcome::Failed),
		Message::CannotConfigure {
			at: path(&[]),
			reason: "echo".to_owned(),
		},
		Message::CannotAct {
			act: "scan".to_owned(),
			at: path(&[]),
			reason: "echo".to_owned(),
		},
		Message::Networks {
			access_points: Vec::new(),
		},
		Message::Spectrum {
			spectrum: Map::new(),
		},
	] {
		send(&mut client, message).await;
	}
	assert!(quiet(&mut client).await, "none is answered");
	send(&mut client, Message::Scan { interface: None }).await;
	assert!(
		matches!(recv(&mut client).await, Message::Networks { .. }),
		"the session carries on"
	);
	assert_eq!(device.log.calls(), []);
}

#[tokio::test]
async fn a_protocol_fault_ends_the_session_and_leaves_the_verification_running() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;

	write_message(&mut client, b"not json").await.unwrap();
	task.await.unwrap();
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))]
	);
	device.log.release(Ok(()));
	let (mut next, _task) = device.open().await;
	let (_, reported) = recv_held(&mut next).await;
	assert!(matches!(
		reported,
		Message::State {
			outcome: Outcome::Applied | Outcome::Verifying,
			..
		}
	));
}
