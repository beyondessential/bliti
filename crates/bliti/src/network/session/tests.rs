//! The configuration session of CFG, driven over an in-memory stream against a fake backend.

use std::{
	sync::{Arc, Mutex as SyncMutex},
	time::Duration,
};

use bliti_core::channel::{
	config::{Document, Invalid, Segment, path},
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::AsyncWriteExt;
use serde_json::{Map, Value as Json, json};
use tokio::{io::DuplexStream, task::JoinHandle};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use crate::network::select::Stage;

use super::{
	Backend, Configurator, Store, serve,
	store::tests::{Scratch, object},
};

/// What the fake backend was asked to do, in order.
#[derive(Debug, Clone, PartialEq)]
enum Call {
	Apply(Document),
	/// An apply whose future was dropped before it finished.
	Aborted,
	Restore(Document),
	Wps(String, Option<String>),
}

/// How the fake answers `apply`.
#[derive(Debug, Clone)]
enum Applying {
	Succeed,
	Fail(Invalid),
	/// Never finishes, as a verification still under way.
	Hang,
}

struct Shared {
	calls: Vec<Call>,
	applying: Applying,
	refuse: Option<Invalid>,
	survey: Option<Map<String, Json>>,
	joined: Result<Map<String, Json>, Invalid>,
}

/// A backend standing in for the running system, recording what it is asked to do.
struct Fake(Arc<SyncMutex<Shared>>);

/// The test's view of a [`Fake`].
#[derive(Clone)]
struct Log(Arc<SyncMutex<Shared>>);

impl Log {
	fn calls(&self) -> Vec<Call> {
		self.0.lock().unwrap().calls.clone()
	}

	fn clear(&self) {
		self.0.lock().unwrap().calls.clear();
	}

	fn set(&self, applying: Applying) {
		self.0.lock().unwrap().applying = applying;
	}

	fn refuse(&self, invalid: Invalid) {
		self.0.lock().unwrap().refuse = Some(invalid);
	}

	fn surveys(&self, spectrum: Map<String, Json>) {
		self.0.lock().unwrap().survey = Some(spectrum);
	}

	fn joins(&self, joined: Result<Map<String, Json>, Invalid>) {
		self.0.lock().unwrap().joined = joined;
	}

	/// Wait for the backend to have been asked to do something, as a session ends on a task.
	async fn until(&self, predicate: impl Fn(&[Call]) -> bool) -> Vec<Call> {
		for _ in 0..200 {
			let calls = self.calls();
			if predicate(&calls) {
				return calls;
			}
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
		panic!("the backend was never asked; it saw {:?}", self.calls());
	}
}

fn fake() -> (Fake, Log) {
	let shared = Arc::new(SyncMutex::new(Shared {
		calls: Vec::new(),
		applying: Applying::Succeed,
		refuse: None,
		survey: None,
		joined: Err(Invalid {
			at: path(&[]),
			reason: "no access point is offering WPS".to_owned(),
			reached: None,
		}),
	}));
	(Fake(shared.clone()), Log(shared))
}

/// Records that an apply was dropped before it finished.
struct OnAbort(Arc<SyncMutex<Shared>>);

impl Drop for OnAbort {
	fn drop(&mut self) {
		self.0.lock().unwrap().calls.push(Call::Aborted);
	}
}

impl Backend for Fake {
	fn capabilities(&self) -> Map<String, Json> {
		object(json!({"wps": ["push-button"], "x-shape-under-review": true}))
	}

	fn check(&self, _document: &Document) -> Result<(), Invalid> {
		match &self.0.lock().unwrap().refuse {
			Some(invalid) => Err(invalid.clone()),
			None => Ok(()),
		}
	}

	async fn apply(&mut self, document: &Document) -> Result<(), Invalid> {
		let applying = {
			let mut shared = self.0.lock().unwrap();
			shared.calls.push(Call::Apply(document.clone()));
			shared.applying.clone()
		};
		match applying {
			Applying::Succeed => Ok(()),
			Applying::Fail(invalid) => Err(invalid),
			Applying::Hang => {
				let _abort = OnAbort(self.0.clone());
				std::future::pending().await
			}
		}
	}

	async fn restore(&mut self, document: &Document) -> anyhow::Result<()> {
		self.0
			.lock()
			.unwrap()
			.calls
			.push(Call::Restore(document.clone()));
		Ok(())
	}

	async fn scan(&mut self, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		Ok(vec![
			json!({"interface": interface.unwrap_or("wlan0"), "ssid": "Clinic", "signal": -61}),
		])
	}

	async fn survey(
		&mut self,
		_interface: Option<&str>,
	) -> Result<Option<Map<String, Json>>, Invalid> {
		Ok(self.0.lock().unwrap().survey.clone())
	}

	async fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		_base: &Map<String, Json>,
	) -> Result<Map<String, Json>, Invalid> {
		let mut shared = self.0.lock().unwrap();
		shared.calls.push(Call::Wps(
			method.to_owned(),
			interface.map(ToOwned::to_owned),
		));
		shared.joined.clone()
	}
}

/// The recorded configuration the tests start from: a wall port, and a member a newer client added.
fn recorded() -> Map<String, Json> {
	object(json!({
		"attachments": [{"kind": "wired-dynamic", "label": "Wall port", "interface": "eth0"}],
		"x-added-by-a-newer-client": {"kept": true},
	}))
}

/// A proposal: the clinic's wireless network ahead of the wall port.
fn proposal() -> Map<String, Json> {
	object(json!({
		"attachments": [
			{"kind": "wireless", "label": "Clinic", "ssid": "Clinic",
			 "security": {"kind": "psk", "passphrase": "correct horse"}},
			{"kind": "wired-dynamic", "label": "Wall port", "interface": "eth0"},
		],
		"regulatory-domain": "NZ",
	}))
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

fn parsed(raw: &Map<String, Json>) -> Document {
	Document::parse(raw).unwrap()
}

type Client = Compat<DuplexStream>;

/// A device with a fake backend and a recorded configuration on disk.
struct Device {
	configurator: Configurator<Fake>,
	log: Log,
	scratch: Scratch,
}

impl Device {
	async fn new() -> Self {
		let scratch = Scratch::new();
		scratch.store().record(recorded()).await.unwrap();
		let (backend, log) = fake();
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

	/// Open a session as a client would: a stream whose first message is `configure`, served the way
	/// the dispatcher serves it once it has read that message.
	async fn open(&self) -> (Client, JoinHandle<()>) {
		let (client, device) = tokio::io::duplex(1 << 16);
		let mut client = client.compat();
		let mut device = device.compat();
		write_message(&mut client, &Message::Configure.to_json())
			.await
			.unwrap();
		let configurator = self.configurator.clone();
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

	/// Open a session and take its opening `configuration`.
	async fn opened(&self) -> (Client, JoinHandle<()>, Map<String, Json>) {
		let (mut client, task) = self.open().await;
		let Message::Configuration { document, .. } = recv(&mut client).await else {
			panic!("a session opens with the configuration");
		};
		(client, task, document)
	}
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

/// Whether the device sends nothing for a moment.
async fn quiet(client: &mut Client) -> bool {
	tokio::time::timeout(Duration::from_millis(100), read_message(client))
		.await
		.is_err()
}

/// The configuration a `confirm` answers with, which is the recorded one.
async fn confirmed(client: &mut Client) -> Map<String, Json> {
	send(client, Message::Confirm).await;
	match recv(client).await {
		Message::Configuration {
			document,
			capabilities: None,
		} => document,
		other => panic!("confirm is answered with the configuration, got {other:?}"),
	}
}

#[tokio::test]
async fn opening_a_session_returns_the_configuration_in_force_and_the_capabilities() {
	let device = Device::new().await;
	let (mut client, _task) = device.open().await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: recorded(),
			capabilities: Some(object(
				json!({"wps": ["push-button"], "x-shape-under-review": true})
			)),
		},
		"the recorded document is echoed raw, with the member this build does not know"
	);
}

#[tokio::test]
async fn a_second_session_while_one_is_open_is_told_the_device_is_busy() {
	let device = Device::new().await;
	let (_first, _task, _) = device.opened().await;

	let (mut second, task) = device.open().await;
	assert_eq!(recv(&mut second).await, Message::Busy);
	assert!(
		matches!(read_message(&mut second).await, Ok(None) | Err(_)),
		"the busy stream is closed"
	);
	task.await.unwrap();
}

#[tokio::test]
async fn a_session_can_open_once_the_one_before_it_has_ended() {
	let device = Device::new().await;
	let (mut first, task, _) = device.opened().await;
	first.close().await.unwrap();
	task.await.unwrap();

	let (mut second, _task) = device.open().await;
	assert!(matches!(
		recv(&mut second).await,
		Message::Configuration {
			capabilities: Some(_),
			..
		}
	));
}

#[tokio::test]
async fn writing_the_configuration_back_unmodified_changes_nothing() {
	let device = Device::new().await;
	let (mut client, _task, document) = device.opened().await;
	propose(&mut client, document).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(
		device.log.calls(),
		[Call::Apply(parsed(&recorded()))],
		"the backend is asked to run exactly what it already runs"
	);
}

#[tokio::test]
async fn a_proposal_is_applied_to_the_running_system_and_written_nowhere() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(device.log.calls(), [Call::Apply(parsed(&proposal()))]);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}

#[tokio::test]
async fn confirming_a_proposal_makes_it_the_recorded_configuration() {
	let device = Device::new().await;
	let (mut client, task, _) = device.opened().await;
	let mut sent = proposal();
	sent.insert("x-added-by-this-client".to_owned(), json!(1));
	propose(&mut client, sent.clone()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	assert_eq!(confirmed(&mut client).await, sent, "what is now in force");
	assert_eq!(
		device.store().load().unwrap(),
		Some(sent.clone()),
		"recorded raw, as the client sent it"
	);

	// Recorded, so ending the session keeps it rather than reverting.
	client.close().await.unwrap();
	task.await.unwrap();
	assert_eq!(device.log.calls(), [Call::Apply(parsed(&sent))]);

	// And it is what the next session opens with, and what a restart puts in force.
	let (_client, _task, opened) = device.opened().await;
	assert_eq!(opened, sent);
	let (backend, log) = fake();
	let _restarted =
		Configurator::start(backend, device.store(), object(json!({"attachments": []})))
			.await
			.unwrap();
	assert_eq!(log.calls(), [Call::Restore(parsed(&sent))]);
}

#[tokio::test]
async fn discard_during_verification_aborts_the_attempt_and_leaves_the_recorded_configuration() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()))))
		.await;

	send(&mut client, Message::Discard).await;
	let Message::Invalid { reached, .. } = recv(&mut client).await else {
		panic!("the interrupted proposal is answered invalid");
	};
	assert_eq!(reached, None, "an aborted attempt reports no stage");
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal())),
			Call::Aborted,
			Call::Restore(parsed(&recorded())),
		]
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}

#[tokio::test]
async fn discard_after_a_proposal_is_applied_reverts_to_the_recorded_configuration() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	send(&mut client, Message::Discard).await;
	assert!(quiet(&mut client).await, "discard is not answered");
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal())),
			Call::Restore(parsed(&recorded())),
		]
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}

#[tokio::test]
async fn a_session_abandoned_by_closing_the_stream_leaves_the_recorded_configuration() {
	let device = Device::new().await;
	let (mut client, task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	client.close().await.unwrap();
	task.await.unwrap();
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal())),
			Call::Restore(parsed(&recorded())),
		]
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}

#[tokio::test]
async fn a_session_abandoned_by_the_channel_dropping_leaves_the_recorded_configuration() {
	let device = Device::new().await;
	let (mut client, task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	// The whole transport goes, with no close.
	drop(client);
	task.await.unwrap();
	assert!(
		device
			.log
			.calls()
			.ends_with(&[Call::Restore(parsed(&recorded()))])
	);
}

#[tokio::test]
async fn a_session_whose_task_is_dropped_restores_before_another_can_open() {
	let device = Device::new().await;
	let (mut client, task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	// The connection's task is dropped with the session in it.
	task.abort();
	let _ = task.await;
	device
		.log
		.until(|calls| calls.ends_with(&[Call::Restore(parsed(&recorded()))]))
		.await;

	let (mut next, _task) = device.open().await;
	assert!(matches!(
		recv(&mut next).await,
		Message::Configuration { document, .. } if document == recorded()
	));
}

#[tokio::test]
async fn a_session_dropped_during_verification_aborts_and_restores() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()))))
		.await;

	task.abort();
	let calls = device
		.log
		.until(|calls| calls.ends_with(&[Call::Restore(parsed(&recorded()))]))
		.await;
	assert_eq!(
		calls,
		[
			Call::Apply(parsed(&proposal())),
			Call::Aborted,
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn a_device_powered_off_mid_proposal_starts_on_the_recorded_configuration() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	// Power comes back: a fresh daemon over the same disk, with nothing of the session.
	let (backend, log) = fake();
	let restarted =
		Configurator::start(backend, device.store(), object(json!({"attachments": []})))
			.await
			.unwrap();
	assert_eq!(log.calls(), [Call::Restore(parsed(&recorded()))]);
	drop(restarted);
}

#[tokio::test]
async fn a_device_with_nothing_recorded_starts_on_the_fallback() {
	let scratch = Scratch::new();
	let (backend, log) = fake();
	let fallback = object(json!({"attachments": []}));
	let configurator = Configurator::start(backend, scratch.store(), fallback.clone())
		.await
		.unwrap();
	assert_eq!(log.calls(), [Call::Restore(parsed(&fallback))]);
	assert_eq!(
		scratch.store().load().unwrap(),
		None,
		"the fallback is not recorded"
	);
	drop(configurator);
}

#[tokio::test(start_paused = true)]
async fn a_proposal_is_never_timed_out_while_its_session_is_open() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	tokio::time::advance(Duration::from_secs(30 * 24 * 60 * 60)).await;
	assert_eq!(device.log.calls(), [Call::Apply(parsed(&proposal()))]);
	assert_eq!(confirmed(&mut client).await, proposal());
	assert_eq!(device.store().load().unwrap(), Some(proposal()));
}

#[tokio::test]
async fn a_device_retains_nothing_of_a_proposal_after_reverting() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	send(&mut client, Message::Discard).await;
	assert_eq!(
		confirmed(&mut client).await,
		recorded(),
		"after a discard there is nothing to confirm"
	);

	device.log.set(Applying::Fail(
		Stage::Association.failed(passphrase(), "the key was refused"),
	));
	propose(&mut client, proposal()).await;
	assert!(matches!(recv(&mut client).await, Message::Invalid { .. }));
	assert_eq!(
		confirmed(&mut client).await,
		recorded(),
		"after a failure there is nothing to confirm"
	);

	send(&mut client, Message::Configure).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Configuration { document, .. } if document == recorded()
	));
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}

#[tokio::test]
async fn a_failure_carries_the_part_at_fault_a_reason_and_the_stage_it_stopped_at() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// A wrong passphrase: carrier held, association refused.
	device.log.set(Applying::Fail(
		Stage::Association.failed(passphrase(), "the access point refused the key (reason 15)"),
	));
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: "$['attachments'][0]['security']['passphrase']".to_owned(),
			reason: "the access point refused the key (reason 15)".to_owned(),
			reached: Some("association".to_owned()),
		}
	);

	// A wired candidate that leases an address on a network that does not route.
	device.log.set(Applying::Fail(Stage::Gateway.failed(
		path(&[Segment::Name("attachments"), Segment::Index(1)]),
		"192.168.1.1 did not answer",
	)));
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: "$['attachments'][1]".to_owned(),
			reason: "192.168.1.1 did not answer".to_owned(),
			reached: Some("gateway".to_owned()),
		}
	);

	// Each failed apply is followed by the recorded configuration being restored.
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal())),
			Call::Restore(parsed(&recorded())),
			Call::Apply(parsed(&proposal())),
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn a_document_that_cannot_be_accepted_is_invalid_with_no_stage_and_nothing_applied() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// Structurally invalid: a static candidate with no gateway (LINK).
	propose(
		&mut client,
		object(json!({"attachments": [
			{"kind": "wired-static", "label": "Site A", "interface": "eth0",
			 "addresses": ["10.0.0.5/24"]},
		]})),
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: "$['attachments'][0]['gateway']".to_owned(),
			reason: "a static candidate carries a gateway".to_owned(),
			reached: None,
		}
	);

	// Outside the capabilities, found by the backend's check (NET).
	device.log.refuse(Invalid {
		at: path(&[Segment::Name("hotspot")]),
		reason: "this radio cannot run a hotspot".to_owned(),
		reached: None,
	});
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: path(&[Segment::Name("hotspot")]),
			reason: "this radio cannot run a hotspot".to_owned(),
			reached: None,
		}
	);

	assert_eq!(device.log.calls(), [], "nothing of either was applied");
}

#[tokio::test]
async fn an_invalid_proposal_leaves_an_applied_one_in_place() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	propose(&mut client, object(json!({"attachments": "none"}))).await;
	assert!(matches!(recv(&mut client).await, Message::Invalid { .. }));
	assert_eq!(confirmed(&mut client).await, proposal());
}

#[tokio::test]
async fn a_new_proposal_while_one_is_applied_replaces_it() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	let mut second = proposal();
	second.insert("regulatory-domain".to_owned(), json!("AU"));
	propose(&mut client, second.clone()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(confirmed(&mut client).await, second);
}

#[tokio::test]
async fn a_proposal_while_another_is_being_verified_supersedes_it() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()))))
		.await;

	device.log.set(Applying::Succeed);
	propose(&mut client, recorded()).await;
	assert!(
		matches!(
			recv(&mut client).await,
			Message::Invalid { reached: None, .. }
		),
		"the interrupted proposal is answered first"
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal())),
			Call::Aborted,
			Call::Apply(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn anything_else_sent_during_verification_is_answered_after_the_proposal() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()))))
		.await;

	send(&mut client, Message::Scan { interface: None }).await;
	assert!(quiet(&mut client).await, "the scan waits for the proposal");

	send(&mut client, Message::Discard).await;
	assert!(matches!(recv(&mut client).await, Message::Invalid { .. }));
	assert!(matches!(recv(&mut client).await, Message::Networks { .. }));
}

#[tokio::test]
async fn confirm_with_nothing_applied_records_nothing() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	assert_eq!(confirmed(&mut client).await, recorded());
	assert_eq!(device.log.calls(), []);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
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
async fn survey_is_answered_with_the_spectrum_or_invalid_where_unsupported() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, Message::Survey { interface: None }).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$"
	));

	let spectrum = object(json!({"channels": [{"number": 6, "busy": 0.4}]}));
	device.log.surveys(spectrum.clone());
	send(&mut client, Message::Survey { interface: None }).await;
	assert_eq!(recv(&mut client).await, Message::Spectrum { spectrum });
}

#[tokio::test]
async fn wps_proposes_what_it_joined_for_the_client_to_confirm() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.joins(Ok(proposal()));
	send(
		&mut client,
		Message::Wps {
			method: "push-button".to_owned(),
			interface: None,
		},
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: proposal(),
			capabilities: None,
		}
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));

	assert_eq!(confirmed(&mut client).await, proposal());
	assert_eq!(device.store().load().unwrap(), Some(proposal()));
	assert_eq!(
		device.log.calls(),
		[Call::Wps("push-button".to_owned(), None)]
	);
}

#[tokio::test]
async fn wps_that_fails_is_invalid_and_restores() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(
		&mut client,
		Message::Wps {
			method: "pin".to_owned(),
			interface: Some("wlan0".to_owned()),
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, .. } if at == "$"
	));
	assert_eq!(
		device.log.calls(),
		[
			Call::Wps("pin".to_owned(), Some("wlan0".to_owned())),
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn messages_only_a_device_sends_are_no_ops() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	for message in [
		Message::Applied { capabilities: None },
		Message::Busy,
		Message::Invalid {
			at: path(&[]),
			reason: "echo".to_owned(),
			reached: None,
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
async fn a_protocol_fault_ends_the_session_and_restores() {
	let device = Device::new().await;
	let (mut client, task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	write_message(&mut client, b"not json").await.unwrap();
	task.await.unwrap();
	assert!(
		device
			.log
			.calls()
			.ends_with(&[Call::Restore(parsed(&recorded()))])
	);
}
