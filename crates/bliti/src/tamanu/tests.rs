use std::time::Duration;

use bliti_core::channel::{
	envelope::{Reading, read},
	stream::{read_message, write_message},
};
use futures::io::{AsyncReadExt, ReadHalf, WriteHalf};
use tokio::sync::Semaphore;
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use super::*;
use fake::{DEGRADED, Fake, RUNNING, STOPPED};
use state::State;

type End = Compat<tokio::io::DuplexStream>;

/// Whatever the client end of a Tamanu stream has been sent.
struct Client {
	reader: ReadHalf<End>,
	writer: WriteHalf<End>,
	serving: tokio::task::JoinHandle<()>,
}

impl Client {
	async fn send(&mut self, message: Message) {
		write_message(&mut self.writer, &message.to_json())
			.await
			.unwrap();
	}

	async fn act(&mut self, act: &str) {
		self.send(Message::TamanuAct {
			act: act.to_owned(),
		})
		.await;
	}

	async fn next(&mut self) -> Message {
		let raw = tokio::time::timeout(Duration::from_secs(3600), read_message(&mut self.reader))
			.await
			.expect("the device answers")
			.unwrap()
			.expect("the stream is open");
		match read::<Message>(&raw).unwrap() {
			Reading::Message(message) => message,
			other => panic!("{other:?}"),
		}
	}

	async fn next_acts(&mut self) -> Vec<String> {
		match self.next().await {
			Message::TamanuActs { acts } => acts,
			other => panic!("{other:?}"),
		}
	}

	/// The next message that is not a change in the acts listed.
	async fn next_result(&mut self) -> Message {
		loop {
			match self.next().await {
				Message::TamanuActs { .. } => {}
				other => return other,
			}
		}
	}
}

async fn tamanu_with(fake: Fake) -> (Tamanu, Arc<Fake>) {
	let fake = Arc::new(fake);
	let tamanu = Tamanu::start(
		fake.clone(),
		Arc::new(|| true),
		Duration::from_secs(30 * 60),
	);
	tamanu.looked().await;
	(tamanu, fake)
}

/// A device held on its first act until the returned gate is given a permit.
async fn gated(fake: Fake) -> (Tamanu, Arc<Fake>, Arc<Semaphore>) {
	let (fake, gate) = fake.gated();
	let (tamanu, fake) = tamanu_with(fake).await;
	(tamanu, fake, gate)
}

/// Open a Tamanu stream against `tamanu` and read its opening list of acts.
async fn open(tamanu: &Tamanu) -> (Client, Vec<String>) {
	let (device, client) = tokio::io::duplex(64 * 1024);
	let tamanu = tamanu.clone();
	let serving = tokio::spawn(async move {
		let mut device = device.compat();
		let _ = serve(&mut device, &tamanu, &Peer::default()).await;
	});
	let (reader, writer) = client.compat().split();
	let mut client = Client {
		reader,
		writer,
		serving,
	};
	client.send(Message::Tamanu).await;
	let acts = client.next_acts().await;
	(client, acts)
}

fn strings(acts: &[&str]) -> Vec<String> {
	acts.iter().map(|act| (*act).to_owned()).collect()
}

fn refused(message: Message) -> (String, String, String) {
	match message {
		Message::CannotAct { act, at, reason } => (act, at, reason),
		other => panic!("{other:?}"),
	}
}

#[tokio::test(start_paused = true)]
async fn the_opening_list_follows_the_state() {
	for (fake, expected) in [
		(Fake::reporting(RUNNING), strings(&["restart"])),
		(Fake::reporting(DEGRADED), strings(&["restart"])),
		(Fake::reporting(STOPPED), strings(&["start"])),
		(Fake::absent(), strings(&[])),
		(Fake::unrunnable("bestool is missing"), strings(&[])),
	] {
		let (tamanu, _) = tamanu_with(fake).await;
		let (_client, acts) = open(&tamanu).await;
		assert_eq!(acts, expected);
	}
}

#[tokio::test(start_paused = true)]
async fn a_device_with_nothing_to_say_lists_no_act() {
	let (_client, acts) = open(&Tamanu::none()).await;
	assert_eq!(acts, strings(&[]));
}

#[tokio::test(start_paused = true)]
async fn an_act_is_refused_where_it_is_not_listed() {
	let (tamanu, _) = tamanu_with(Fake::reporting(RUNNING)).await;
	let (mut client, _) = open(&tamanu).await;
	for act in ["start", "reset-data"] {
		client.act(act).await;
		let (named, at, _) = refused(client.next().await);
		assert_eq!(named, act);
		assert_eq!(at, "$['act']");
	}
	assert_eq!(tamanu.view().acting, None);
}

#[tokio::test(start_paused = true)]
async fn a_restart_is_reported_as_restarting_until_it_has_been_carried_out() {
	let fake = Fake::reporting(RUNNING).on_act(Act::Restart, Ok(()), RUNNING);
	let (tamanu, fake, gate) = gated(fake).await;
	let (mut client, _) = open(&tamanu).await;

	client.act("restart").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);
	assert_eq!(client.next_acts().await, strings(&[]));
	assert_eq!(tamanu.view().state(), Some(State::Restarting));

	gate.add_permits(1);
	assert_eq!(client.next_acts().await, strings(&["restart"]));
	assert_eq!(tamanu.view().state(), Some(State::Running));
	assert_eq!(
		fake.calls()
			.iter()
			.filter(|call| *call == "restart")
			.count(),
		1
	);
}

#[tokio::test(start_paused = true)]
async fn a_start_brings_a_stopped_tamanu_to_running() {
	let fake = Fake::reporting(STOPPED).on_act(Act::Start, Ok(()), RUNNING);
	let (tamanu, _, gate) = gated(fake).await;
	let (mut client, acts) = open(&tamanu).await;
	assert_eq!(acts, strings(&["start"]));

	client.act("start").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);
	assert_eq!(client.next_acts().await, strings(&[]));
	assert_eq!(tamanu.view().state(), Some(State::Starting));

	gate.add_permits(1);
	assert_eq!(client.next_acts().await, strings(&["restart"]));
	assert_eq!(tamanu.view().state(), Some(State::Running));
}

#[tokio::test(start_paused = true)]
async fn a_second_act_is_refused_on_any_stream_while_one_is_under_way() {
	let fake = Fake::reporting(RUNNING).on_act(Act::Restart, Ok(()), RUNNING);
	let (tamanu, fake, gate) = gated(fake).await;
	let (mut first, _) = open(&tamanu).await;
	first.act("restart").await;
	assert_eq!(first.next().await, Message::TamanuAccepted);

	let (mut second, acts) = open(&tamanu).await;
	assert_eq!(acts, strings(&[]));
	second.act("restart").await;
	let (act, at, reason) = refused(second.next().await);
	assert_eq!(act, "restart");
	assert_eq!(at, "$");
	assert!(reason.contains("already restarting"), "{reason}");

	gate.add_permits(1);
	assert_eq!(second.next_acts().await, strings(&["restart"]));
	assert_eq!(
		fake.calls()
			.iter()
			.filter(|call| *call == "restart")
			.count(),
		1
	);
}

#[tokio::test(start_paused = true)]
async fn an_act_that_leaves_services_short_fails_on_the_asking_stream_only() {
	let fake =
		Fake::reporting(RUNNING).on_act(Act::Restart, Err("the unit did not start"), DEGRADED);
	let (tamanu, _, gate) = gated(fake).await;
	let (mut asking, _) = open(&tamanu).await;
	let (mut watching, _) = open(&tamanu).await;

	asking.act("restart").await;
	assert_eq!(asking.next().await, Message::TamanuAccepted);
	gate.add_permits(1);

	let Message::TamanuFailed { reason } = asking.next_result().await else {
		panic!("expected a failure");
	};
	assert!(reason.contains("the unit did not start"), "{reason}");
	assert!(
		reason.contains("tamanu-facility-api has 1 of 2 instances running"),
		"{reason}"
	);
	assert_eq!(tamanu.view().state(), Some(State::Degraded));

	// The other stream sees the acts change, and never the failure.
	assert_eq!(watching.next_acts().await, strings(&[]));
	assert_eq!(watching.next_acts().await, strings(&["restart"]));
}

#[tokio::test(start_paused = true)]
async fn an_act_whose_command_ends_well_but_leaves_tamanu_short_fails_after_waiting() {
	let fake = Fake::reporting(STOPPED).on_act(Act::Start, Ok(()), DEGRADED);
	let (tamanu, fake) = tamanu_with(fake).await;
	let (mut client, _) = open(&tamanu).await;

	client.act("start").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);
	let Message::TamanuFailed { reason } = client.next_result().await else {
		panic!("expected a failure");
	};
	assert_eq!(reason, "tamanu-facility-api has 1 of 2 instances running");
	assert_eq!(tamanu.view().acting, None);
	assert!(
		fake.calls().iter().filter(|call| *call == "status").count() > 2,
		"looked again while it settled"
	);
}

#[tokio::test(start_paused = true)]
async fn an_act_is_carried_out_after_the_stream_that_asked_for_it_has_ended() {
	let fake = Fake::reporting(RUNNING).on_act(Act::Restart, Ok(()), RUNNING);
	let (tamanu, fake, gate) = gated(fake).await;
	let (mut client, _) = open(&tamanu).await;
	client.act("restart").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);

	let Client {
		reader,
		writer,
		serving,
	} = client;
	drop((reader, writer));
	serving.await.unwrap();
	assert_eq!(tamanu.view().state(), Some(State::Restarting));

	gate.add_permits(1);
	while tamanu.view().acting.is_some() {
		tokio::time::sleep(Duration::from_secs(1)).await;
	}
	assert_eq!(tamanu.view().state(), Some(State::Running));
	assert!(fake.calls().contains(&"restart".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn the_list_changes_when_tamanu_changes_without_an_act() {
	let (tamanu, fake) = tamanu_with(Fake::reporting(RUNNING)).await;
	let (mut client, acts) = open(&tamanu).await;
	assert_eq!(acts, strings(&["restart"]));
	fake.set_status(STOPPED);
	assert_eq!(client.next_acts().await, strings(&["start"]));
}

#[tokio::test(start_paused = true)]
async fn an_act_that_never_ends_is_given_up_on() {
	let (tamanu, _, _gate) = gated(Fake::reporting(STOPPED)).await;
	let (mut client, _) = open(&tamanu).await;
	client.act("start").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);
	let Message::TamanuFailed { reason } = client.next_result().await else {
		panic!("expected a failure");
	};
	assert!(reason.contains("took too long"), "{reason}");
	assert_eq!(tamanu.view().acting, None);
}

#[tokio::test(start_paused = true)]
async fn a_command_that_fails_is_a_failure_even_where_tamanu_is_running() {
	let fake =
		Fake::reporting(RUNNING).on_act(Act::Restart, Err("sudo: a password is required"), RUNNING);
	let (tamanu, _) = tamanu_with(fake).await;
	let (mut client, _) = open(&tamanu).await;

	client.act("restart").await;
	assert_eq!(client.next().await, Message::TamanuAccepted);
	let Message::TamanuFailed { reason } = client.next_result().await else {
		panic!("expected a failure");
	};
	assert_eq!(reason, "the restart failed (sudo: a password is required)");
	assert_eq!(tamanu.view().state(), Some(State::Running));
}

#[tokio::test(start_paused = true)]
async fn nothing_is_reported_listed_or_accepted_before_tamanu_has_been_looked_at() {
	let (fake, gate) = Fake::reporting(RUNNING).status_gated();
	let tamanu = Tamanu::start(
		Arc::new(fake),
		Arc::new(|| true),
		Duration::from_secs(30 * 60),
	);
	assert_eq!(tamanu.view().state(), None);
	assert!(crate::facts::tamanu::entries(0, &tamanu.view()).is_empty());

	let (mut client, acts) = open(&tamanu).await;
	assert_eq!(acts, strings(&[]));
	client.act("restart").await;
	let (_, at, _) = refused(client.next().await);
	assert_eq!(at, "$['act']");

	gate.add_permits(1);
	assert_eq!(client.next_acts().await, strings(&["restart"]));
	assert_eq!(crate::facts::tamanu::entries(0, &tamanu.view()).len(), 3);
}

#[tokio::test(start_paused = true)]
async fn a_look_that_began_earlier_is_not_published_after_a_later_one() {
	let (tamanu, _) = tamanu_with(Fake::reporting(RUNNING)).await;
	let held = tamanu.inner.looking.lock().await;
	let inner = tamanu.inner.clone();
	let refreshing = tokio::spawn(async move { inner.refresh().await });
	tokio::time::sleep(Duration::from_secs(60)).await;
	assert!(!refreshing.is_finished(), "waits for the look under way");
	drop(held);
	assert!(refreshing.await.unwrap().is_running());
}
