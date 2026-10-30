use std::{sync::Arc, time::Duration};

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		stream::{Stream, connect_initiator, connect_initiator_unauthenticated},
	},
	key_schedule::Root,
	provisioning::{self, report::RunState},
};
use tokio_util::compat::TokioAsyncReadCompatExt;

use super::*;
use crate::provision::{
	fake::{BUILT, Do, Fake, Gate, Log},
	step::Step,
	store::Store,
};

const PATIENCE: Duration = Duration::from_secs(5);

fn device_keys() -> DeviceKeys {
	DeviceKeys::provisioning([0x42; 32], [0x24; 32])
}

struct Rig {
	streams: Streams,
	provisioner: Provisioner,
	_medium: tempfile::TempDir,
}

/// A client holding only the provisioning token, against a device in provisioning mode running
/// `steps`, over an in-memory duplex.
async fn paired(steps: Vec<Arc<Fake>>) -> Rig {
	let medium = tempfile::tempdir().unwrap();
	let provisioner = Provisioner::start(
		steps
			.into_iter()
			.map(|step| step as Arc<dyn Step>)
			.collect(),
		Some(Store::new(medium.path(), &[1])),
		BUILT,
	)
	.await;
	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let keys = device_keys();
	let device = provisioner.clone();
	tokio::spawn(async move {
		let _ = run(device_side.compat(), &keys, device).await;
	});
	let encrypted =
		connect_initiator_unauthenticated(client_side.compat(), &provisioning::pre_shared_key())
			.await
			.unwrap();
	let (streams, driver) = multiplex(encrypted, Mode::Client);
	tokio::spawn(async move {
		let _ = driver.await;
	});
	Rig {
		streams,
		provisioner,
		_medium: medium,
	}
}

async fn next(stream: &mut Stream) -> Message {
	let raw = tokio::time::timeout(PATIENCE, read_message(stream))
		.await
		.expect("the device said nothing")
		.unwrap()
		.expect("the stream ended");
	match read::<Message>(&raw).unwrap() {
		Reading::Message(message) => message,
		other => panic!("not a message: {other:?}"),
	}
}

async fn say(stream: &mut Stream, message: &Message) {
	write_message(stream, &message.to_json()).await.unwrap();
}

/// Open a provisioning stream and read the state the device answers `provision` with.
async fn open(streams: &mut Streams) -> (Stream, Message) {
	let mut stream = streams.open().await.unwrap();
	say(&mut stream, &Message::Provision).await;
	let state = next(&mut stream).await;
	(stream, state)
}

fn start(steps: &[&str]) -> Message {
	Message::ProvisionAct {
		act: "start".to_owned(),
		steps: Some(steps.iter().map(|id| (*id).to_owned()).collect()),
		prompt: None,
		value: None,
	}
}

fn run_of(message: &Message) -> RunState {
	let Message::ProvisionState { run, .. } = message else {
		panic!("not a state: {message:?}");
	};
	*run
}

/// Read until the stream reports `run`, skipping the states before it.
async fn until(stream: &mut Stream, run: RunState) -> Message {
	loop {
		let message = next(stream).await;
		if matches!(&message, Message::ProvisionState { run: held, .. } if *held == run) {
			return message;
		}
	}
}

#[tokio::test]
async fn a_client_holding_only_the_token_is_greeted_with_a_hello() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![])]).await;
	let mut hello = rig.streams.accept().await.expect("a stream");
	assert!(matches!(next(&mut hello).await, Message::Hello { .. }));
}

#[tokio::test]
async fn the_state_is_sent_on_opening() {
	let log = Log::default();
	let mut rig = paired(vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![]),
	])
	.await;
	let (_stream, state) = open(&mut rig.streams).await;
	let Message::ProvisionState {
		revision,
		built,
		run,
		steps,
		prompts,
	} = state
	else {
		panic!("not a state");
	};
	assert_eq!(revision, provisioning::REVISION);
	assert_eq!(built, BUILT);
	assert_eq!(run, RunState::Idle);
	assert_eq!(
		steps.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
		["a", "b"]
	);
	assert!(prompts.is_empty());
}

#[tokio::test]
async fn a_start_is_accepted_and_the_state_follows_it() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![])]).await;
	let (mut stream, _) = open(&mut rig.streams).await;
	say(&mut stream, &start(&["a"])).await;
	let mut accepted = false;
	loop {
		match next(&mut stream).await {
			Message::ProvisionAccepted => accepted = true,
			message if run_of_finished(&message) => break,
			_ => {}
		}
	}
	assert!(accepted, "the act was answered");
}

/// Every state a client is sent carries the revision and the image's date it was given, so a client
/// that connects at any point in a run can refuse a medium it should not drive (PRS).
#[tokio::test]
async fn every_state_carries_the_revision_and_the_images_date() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![Do::Progress(0.5)])]).await;
	let (mut stream, _) = open(&mut rig.streams).await;
	say(&mut stream, &start(&["a"])).await;
	let mut states = 0;
	loop {
		let message = next(&mut stream).await;
		if let Message::ProvisionState {
			revision, built, ..
		} = &message
		{
			assert_eq!((*revision, *built), (provisioning::REVISION, BUILT));
			states += 1;
		}
		if run_of_finished(&message) {
			break;
		}
	}
	assert!(states > 1, "the run changed the state more than once");
}

fn run_of_finished(message: &Message) -> bool {
	matches!(
		message,
		Message::ProvisionState {
			run: RunState::Finished,
			..
		}
	)
}

#[tokio::test]
async fn every_refusal_names_the_act_and_the_member_at_fault() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![])]).await;
	let (mut stream, _) = open(&mut rig.streams).await;

	let act = |act: &str| Message::ProvisionAct {
		act: act.to_owned(),
		steps: None,
		prompt: None,
		value: None,
	};
	for (asked, at) in [
		(act("dance"), "$['act']"),
		(act("stop"), "$"),
		(start(&[]), "$['steps']"),
		(start(&["a", "zz"]), "$['steps'][1]"),
	] {
		let name = match &asked {
			Message::ProvisionAct { act, .. } => act.clone(),
			_ => unreachable!(),
		};
		say(&mut stream, &asked).await;
		let Message::CannotAct {
			act, at: refused, ..
		} = next(&mut stream).await
		else {
			panic!("a refusal was expected for {name}");
		};
		assert_eq!((act, refused.as_str()), (name, at));
	}
}

#[tokio::test]
async fn two_streams_at_once_see_the_same_run() {
	let log = Log::default();
	let gate = Gate::new();
	let mut rig = paired(vec![Fake::new("a", &log, vec![Do::Hold(gate.clone())])]).await;
	let (mut first, _) = open(&mut rig.streams).await;
	let (mut second, _) = open(&mut rig.streams).await;

	say(&mut first, &start(&["a"])).await;
	until(&mut first, RunState::Running).await;
	until(&mut second, RunState::Running).await;

	gate.open();
	until(&mut first, RunState::Finished).await;
	until(&mut second, RunState::Finished).await;
}

#[tokio::test]
async fn an_answer_is_carried_to_the_step_waiting_for_it() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![Do::Confirm("ok")])]).await;
	let (mut stream, _) = open(&mut rig.streams).await;
	say(&mut stream, &start(&["a"])).await;
	let Message::ProvisionState { prompts, .. } = until(&mut stream, RunState::Waiting).await
	else {
		unreachable!()
	};
	say(
		&mut stream,
		&Message::ProvisionAct {
			act: "answer".to_owned(),
			steps: None,
			prompt: Some(prompts[0].id.clone()),
			value: Some(bliti_core::provisioning::report::Answer::boolean(true)),
		},
	)
	.await;
	until(&mut stream, RunState::Finished).await;
	assert!(log.lines().contains(&"a confirmed true".to_owned()));
}

#[tokio::test]
async fn a_run_goes_on_after_the_client_leaves() {
	let log = Log::default();
	let gate = Gate::new();
	let mut rig = paired(vec![Fake::new("a", &log, vec![Do::Hold(gate.clone())])]).await;
	let (mut stream, _) = open(&mut rig.streams).await;
	say(&mut stream, &start(&["a"])).await;
	until(&mut stream, RunState::Running).await;

	let Rig {
		streams,
		provisioner,
		..
	} = rig;
	drop(stream);
	drop(streams);
	gate.open();
	let mut changes = provisioner.watch();
	tokio::time::timeout(PATIENCE, changes.wait_for(|s| s.run == RunState::Finished))
		.await
		.expect("the run stopped when the client left")
		.unwrap();
}

#[tokio::test]
async fn streams_for_anything_else_are_passed_over() {
	let log = Log::default();
	let mut rig = paired(vec![Fake::new("a", &log, vec![])]).await;

	let mut other = rig.streams.open().await.unwrap();
	for message in [
		Message::Subscribe {
			topic: "default".to_owned(),
		},
		Message::Configure,
		Message::Power,
		Message::Tamanu,
		Message::Curve,
	] {
		say(&mut other, &message).await;
	}
	let silence = tokio::time::timeout(Duration::from_millis(300), read_message(&mut other)).await;
	assert!(
		silence.is_err(),
		"the device answered a stream it does not serve"
	);

	// The connection and the provisioning stream are unharmed.
	let (_stream, state) = open(&mut rig.streams).await;
	assert_eq!(run_of(&state), RunState::Idle);
}

#[tokio::test]
async fn a_client_not_holding_the_token_does_not_complete_the_handshake() {
	let log = Log::default();
	let medium = tempfile::tempdir().unwrap();
	let provisioner = Provisioner::start(
		vec![Fake::new("a", &log, vec![]) as Arc<dyn Step>],
		Some(Store::new(medium.path(), &[1])),
		BUILT,
	)
	.await;
	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let keys = device_keys();
	let device = tokio::spawn(async move { run(device_side.compat(), &keys, provisioner).await });

	// A client holding a QR code's presence token instead.
	let other = Root::from_bytes([9; 32]).device_keys();
	let client = connect_initiator(
		client_side.compat(),
		&other.pre_shared_key,
		&other.fingerprint(),
	)
	.await;
	assert!(client.is_err());
	assert!(matches!(
		device.await.unwrap(),
		Err(SessionError::Handshake(_))
	));
}
