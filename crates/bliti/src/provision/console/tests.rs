//! The console against a real [`Provisioner`] on a socket in a temp directory, driven by scripted
//! typing with its screen captured.

use std::{
	io,
	path::{Path, PathBuf},
	pin::Pin,
	sync::{Arc, Mutex},
	time::Duration,
};

use anyhow::Result;
use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::Message,
	},
	provisioning::report::{Answer, DoneState, PromptKind, PromptReport, RunState},
};
use futures::{Stream, StreamExt};
use tempfile::TempDir;
use tokio::{net::UnixStream, task::JoinHandle};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::{Console, attach, connect, run, terminal::Lines};
use crate::{
	carrier::{Carrier, incoming},
	provision::{
		Provisioner, Step,
		fake::{Do, Fake, Gate, Log},
		socket::listen,
		store::Store,
	},
	session::AbortOnDrop,
};

const PATIENCE: Duration = Duration::from_secs(5);

/// What the operator types, with a record of which lines were asked for with no echo.
struct Script {
	typed: std::sync::mpsc::Receiver<String>,
	hidden: Arc<Mutex<Vec<bool>>>,
}

impl Lines for Script {
	fn line(&mut self, hidden: bool) -> io::Result<Option<String>> {
		self.hidden.lock().unwrap().push(hidden);
		Ok(self.typed.recv().ok())
	}
}

#[derive(Clone, Default)]
struct Screen(Arc<Mutex<String>>);

impl io::Write for Screen {
	fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
		self.0
			.lock()
			.unwrap()
			.push_str(&String::from_utf8_lossy(bytes));
		Ok(bytes.len())
	}

	fn flush(&mut self) -> io::Result<()> {
		Ok(())
	}
}

/// A console running, with its keyboard and screen.
struct Operator {
	typed: Option<std::sync::mpsc::Sender<String>>,
	screen: Screen,
	hidden: Arc<Mutex<Vec<bool>>>,
	console: JoinHandle<Result<()>>,
}

impl Operator {
	fn new(socket: &Path) -> Self {
		let (typed, keys) = std::sync::mpsc::channel();
		let hidden = Arc::new(Mutex::new(Vec::new()));
		let script = Script {
			typed: keys,
			hidden: hidden.clone(),
		};
		let screen = Screen::default();
		let socket = socket.to_owned();
		let shown = screen.clone();
		let console = tokio::spawn(async move {
			let (reader, writer) = connect(&socket).await?.into_split();
			attach(reader.compat(), writer.compat_write(), script, shown).await
		});
		Self {
			typed: Some(typed),
			screen,
			hidden,
			console,
		}
	}

	fn says(&self, line: &str) {
		self.typed
			.as_ref()
			.unwrap()
			.send(line.to_owned())
			.expect("the console has stopped reading");
	}

	fn text(&self) -> String {
		self.screen.0.lock().unwrap().clone()
	}

	/// Wait until `text` is on the screen, after the `from`th character of it.
	async fn sees_after(&self, from: usize, text: &str) -> usize {
		let found = tokio::time::timeout(PATIENCE, async {
			loop {
				let screen = self.text();
				if let Some(at) = screen[from..].find(text) {
					return from + at + text.len();
				}
				tokio::time::sleep(Duration::from_millis(5)).await;
			}
		})
		.await;
		found.unwrap_or_else(|_| panic!("{text:?} never appeared on:\n{}", self.text()))
	}

	async fn sees(&self, text: &str) {
		self.sees_after(0, text).await;
	}

	/// How the console ended, and what it had shown.
	async fn ends(self) -> Result<String> {
		let ended = tokio::time::timeout(PATIENCE, self.console)
			.await
			.unwrap_or_else(|_| {
				panic!(
					"the console never ended; its screen:\n{}",
					self.screen.0.lock().unwrap()
				)
			})
			.unwrap();
		ended.map(|()| self.screen.0.lock().unwrap().clone())
	}
}

fn arcs(steps: Vec<Arc<Fake>>) -> Vec<Arc<dyn Step>> {
	steps
		.into_iter()
		.map(|step| step as Arc<dyn Step>)
		.collect()
}

/// A device in provisioning mode, with the console's socket.
struct Device {
	provisioner: Provisioner,
	log: Log,
	dir: TempDir,
	listening: Option<AbortOnDrop>,
}

impl Device {
	async fn new(steps: Vec<Arc<Fake>>, log: &Log) -> Self {
		let dir = tempfile::tempdir().unwrap();
		let store = Store::new(dir.path(), &[0xaa]);
		let provisioner = Provisioner::start(arcs(steps), Some(store)).await;
		let listening = listen(&dir.path().join("provision.sock"), provisioner.clone()).unwrap();
		Self {
			provisioner,
			log: log.clone(),
			dir,
			listening: Some(listening),
		}
	}

	fn socket(&self) -> PathBuf {
		self.dir.path().join("provision.sock")
	}

	fn operator(&self) -> Operator {
		Operator::new(&self.socket())
	}

	async fn is(&self, state: RunState) {
		let mut changes = self.provisioner.watch();
		tokio::time::timeout(PATIENCE, changes.wait_for(|snapshot| snapshot.run == state))
			.await
			.unwrap_or_else(|_| panic!("the device never became {state}"))
			.unwrap();
	}

	async fn logged(&self, line: &str) {
		tokio::time::timeout(PATIENCE, async {
			while !self.log.lines().iter().any(|logged| logged == line) {
				tokio::time::sleep(Duration::from_millis(5)).await;
			}
		})
		.await
		.unwrap_or_else(|_| panic!("{line:?} was never logged: {:?}", self.log.lines()));
	}
}

/// A client of the same socket, speaking the stream as a client of the channel does.
struct Client {
	writer: Compat<tokio::net::unix::OwnedWriteHalf>,
	incoming: Pin<Box<dyn Stream<Item = io::Result<Vec<u8>>> + Send>>,
}

impl Client {
	async fn connect(socket: &Path) -> Self {
		let (reader, writer) = UnixStream::connect(socket).await.unwrap().into_split();
		let mut client = Self {
			writer: writer.compat_write(),
			incoming: Box::pin(incoming(reader.compat(), Carrier::Socket)),
		};
		client.send(&Message::Provision).await;
		client
	}

	async fn send(&mut self, message: &Message) {
		Carrier::Socket
			.write(&mut self.writer, message)
			.await
			.unwrap();
	}

	async fn next(&mut self) -> Message {
		let raw = tokio::time::timeout(PATIENCE, self.incoming.next())
			.await
			.expect("the daemon said nothing")
			.expect("the daemon closed the stream")
			.unwrap();
		match read::<Message>(&raw).unwrap() {
			Reading::Message(message) => message,
			other => panic!("the daemon sent {other:?}"),
		}
	}

	/// The first state to satisfy `until`.
	async fn state(
		&mut self,
		until: impl Fn(RunState, &[PromptReport]) -> bool,
	) -> Vec<PromptReport> {
		loop {
			if let Message::ProvisionState { run, prompts, .. } = self.next().await
				&& until(run, &prompts)
			{
				return prompts;
			}
		}
	}

	/// Ask for an act and read what the device answers it with.
	async fn act(&mut self, message: &Message) -> Message {
		self.send(message).await;
		loop {
			match self.next().await {
				Message::ProvisionState { .. } => {}
				answer => return answer,
			}
		}
	}
}

fn answer_to(prompt: &str, value: Answer) -> Message {
	Message::ProvisionAct {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some(prompt.to_owned()),
		value: Some(value),
	}
}

/// What heads the step list.
const STEPS: &str = "Steps, in the order they run:";

/// The steps are listed in order with their done-states and reasons, the ones reported done are
/// unticked, and Enter starts a run of the ticked ones, which run in the device order (PCON).
#[tokio::test]
async fn the_ticked_steps_are_run_and_the_one_done_is_not() {
	let log = Log::default();
	let done = Fake::new("a", &log, vec![]);
	done.done_is(DoneState::Done, Some("Up to date"));
	let device = Device::new(
		vec![
			done,
			Fake::new("b", &log, vec![]),
			Fake::new("c", &log, vec![]),
		],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.sees("[ ] a: done: Up to date").await;
	operator.sees("2. [x] b: not done").await;
	operator.says("");
	operator.sees("Starting: b, c").await;
	operator.sees("The run has finished.").await;
	let at = operator.sees_after(0, "Acknowledge the finished run").await;
	assert_eq!(log.lines(), ["b begins", "b ends", "c begins", "c ends"]);

	// Acknowledging discards the run, and the list comes back with what the run did to it.
	operator.says("y");
	let at = operator
		.sees_after(at, "No run is held; the device is idle.")
		.await;
	operator.sees_after(at, STEPS).await;
	operator.says("q");
	let screen = operator.ends().await.unwrap();
	assert!(screen.contains("Nothing started."), "{screen}");
}

/// The operator ticks and unticks steps by number and by id before starting.
#[tokio::test]
async fn the_operator_edits_the_selection_by_number_and_by_name() {
	let log = Log::default();
	let done = Fake::new("a", &log, vec![]);
	done.done_is(DoneState::Done, None);
	let device = Device::new(
		vec![
			done,
			Fake::new("b", &log, vec![]),
			Fake::new("c", &log, vec![]),
		],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.sees("[ ] a: done").await;
	operator.says("a 2, 9");
	operator
		.sees("neither a step number from 1 to 3 nor a step name")
		.await;
	operator.says("a 2");
	operator.sees("1. [x] a: done").await;
	operator.sees("2. [ ] b: not done").await;
	operator.says("");
	operator.sees("Starting: a, c").await;
	operator.sees("The run has finished.").await;
	assert_eq!(log.lines(), ["a begins", "a ends", "c begins", "c ends"]);
}

/// Nothing ticked is not started, and a quit leaves the device idle.
#[tokio::test]
async fn nothing_ticked_does_not_start_and_quitting_starts_nothing() {
	let log = Log::default();
	let device = Device::new(vec![Fake::new("a", &log, vec![])], &log).await;
	let operator = device.operator();

	operator.sees("[x] a: ").await;
	operator.says("none");
	operator.says("");
	operator.sees("No step is ticked").await;
	operator.says("q");
	let screen = operator.ends().await.unwrap();
	assert!(screen.contains("Nothing started."), "{screen}");
	assert!(log.lines().is_empty());
	device.is(RunState::Idle).await;
}

/// A confirm, a text and a secret prompt are answered with typed lines, the secret read with no
/// echo and never shown, and a line that is not an answer is asked for again (PCON).
#[tokio::test]
async fn prompts_are_answered_by_typing_and_a_secret_is_not_shown() {
	let log = Log::default();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![Do::Confirm("yes"), Do::Text("name"), Do::Secret("pw")],
		)],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.says("");
	operator.sees("x asks [x-yes]: Sure?").await;
	operator.sees("x-yes [y/n]> ").await;
	operator.says("maybe");
	operator.sees("answer y or n").await;
	operator.says("yes");
	device.logged("x confirmed true").await;

	operator.sees("x-name> ").await;
	operator.says("Ada");
	device.logged("x text Ada").await;

	operator.sees("x-pw (not shown as you type)> ").await;
	operator.says("hunter2");
	device.logged("x secret hunter2").await;

	operator.sees("The run has finished.").await;
	// Choose, confirm twice, text, secret, then the acknowledgement.
	assert_eq!(
		operator.hidden.lock().unwrap()[..5],
		[false, false, false, false, true]
	);
	assert!(!operator.text().contains("hunter2"));
}

/// A step's progress, its result and why it failed are shown, a retry carries on, and the console
/// leaving does not touch the run (PCON, RUN).
#[tokio::test]
async fn progress_result_and_failure_are_shown_and_leaving_leaves_the_run() {
	let log = Log::default();
	let gate = Gate::new();
	let device = Device::new(
		vec![
			Fake::new(
				"a",
				&log,
				vec![Do::Progress(0.5), Do::Result("abc"), Do::Hold(gate.clone())],
			),
			Fake::new("b", &log, vec![Do::FailUntil(2, "no disk")]),
		],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.says("");
	operator.sees("a: 50%").await;
	operator.sees("a reports:").await;
	operator.sees("  found: abc").await;
	gate.open();
	operator.sees("b: failed: no disk").await;
	operator
		.sees("retry, stop to end the run, or q to leave it> ")
		.await;
	operator.says("retry");
	operator.sees("The run has finished.").await;
	operator.sees("Acknowledge the finished run").await;

	operator.says("n");
	let screen = operator.ends().await.unwrap();
	assert!(
		screen.contains("Leaving; the run carries on at the device."),
		"{screen}"
	);
	device.is(RunState::Finished).await;
}

/// A notice is shown as something to do and closes by itself; stop is offered while the run waits.
#[tokio::test]
async fn a_notice_closes_by_itself_and_stop_is_offered_while_it_waits() {
	let log = Log::default();
	let gate = Gate::new();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![Do::Notice("Bridge pads J1", gate.clone())],
		)],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.says("");
	operator.sees("x asks you to do this: Bridge pads J1").await;
	operator
		.sees("stop to end the run, or q to leave it running> ")
		.await;
	gate.open();
	operator.sees("Done: Bridge pads J1").await;
	operator
		.sees("That question has closed; press Enter to go on.")
		.await;
	operator.says("");
	operator
		.sees("That question was settled before you answered")
		.await;
	operator.sees("Acknowledge the finished run").await;
	assert_eq!(log.lines(), ["x begins", "x ends"]);
}

/// A waiting run is stopped from the console, as the stop act of RUN does for a client.
#[tokio::test]
async fn stop_ends_a_waiting_run_and_the_list_comes_back() {
	let log = Log::default();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![Do::Notice("Never done", Gate::new())],
		)],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.says("");
	let at = operator.sees_after(0, "stop to end the run").await;
	operator.says("stop");
	device.is(RunState::Idle).await;
	let at = operator
		.sees_after(at, "No run is held; the device is idle.")
		.await;
	operator.sees_after(at, STEPS).await;
	operator.says("q");
	operator.ends().await.unwrap();
}

/// A prompt can be put off with `:skip`, is not asked again, and does not stop the run; one of a
/// kind the console does not know is shown and left alone (PRS).
#[tokio::test]
async fn a_prompt_can_be_skipped_and_an_unknown_kind_is_only_shown() {
	let log = Log::default();
	let gate = Gate::new();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![
				Do::AskLater("later", PromptKind::Text),
				Do::AskLater("odd", PromptKind::Other("dial".to_owned())),
				Do::Hold(gate.clone()),
			],
		)],
		&log,
	)
	.await;
	let operator = device.operator();

	operator.says("");
	operator.sees("x-later> ").await;
	operator
		.sees("x shows a dial prompt this console cannot answer [x-odd]: Later?")
		.await;
	operator.says(":skip");
	gate.open();
	operator.sees("The run has finished.").await;
	operator.sees("Acknowledge the finished run").await;
	// Choose, the one prompt, and the acknowledgement: nothing was asked twice.
	assert_eq!(operator.hidden.lock().unwrap().len(), 3);
	// Nothing was answered: no step logged an answer.
	assert_eq!(log.lines(), ["x begins", "x ends"]);
}

/// A console started while a run is under way joins it, and one started on a finished run shows its
/// results before offering to acknowledge it (PCON, RUN).
#[tokio::test]
async fn the_console_joins_a_run_under_way_and_a_finished_one() {
	let log = Log::default();
	let gate = Gate::new();
	let device = Device::new(
		vec![
			Fake::new("a", &log, vec![Do::Result("one")]),
			Fake::new("b", &log, vec![Do::Result("two"), Do::Hold(gate.clone())]),
		],
		&log,
	)
	.await;
	let start = Message::ProvisionAct {
		act: "start".to_owned(),
		steps: Some(vec!["a".to_owned(), "b".to_owned()]),
		prompt: None,
		value: None,
	};
	let mut client = Client::connect(&device.socket()).await;
	assert_eq!(client.act(&start).await, Message::ProvisionAccepted);
	device.logged("b begins").await;

	let operator = device.operator();
	operator.sees("A run is under way.").await;
	operator.sees("a: passed").await;
	operator.sees("b: running").await;
	assert!(!operator.text().contains(STEPS));
	gate.open();
	operator.sees("The run has finished.").await;
	operator.sees("Acknowledge the finished run").await;
	operator.says("n");
	operator.ends().await.unwrap();

	let late = device.operator();
	late.sees("The run has finished.").await;
	late.sees("  found: one").await;
	late.sees("  found: two").await;
	let at = late.sees_after(0, "  found: two").await;
	late.sees_after(at, "Acknowledge the finished run").await;
	late.says("n");
	late.ends().await.unwrap();
}

/// A device with no steps has nothing to offer, and the console says so and ends.
#[tokio::test]
async fn a_device_with_no_steps_says_so() {
	let device = Device::new(vec![], &Log::default()).await;
	let screen = device.operator().ends().await.unwrap();
	assert!(
		screen.contains("This device offers no steps to run."),
		"{screen}"
	);
}

/// A daemon that goes away ends the console with a message that says so.
#[tokio::test]
async fn a_daemon_that_goes_away_ends_the_console() {
	let log = Log::default();
	let mut device = Device::new(vec![Fake::new("a", &log, vec![])], &log).await;
	let operator = device.operator();
	operator.sees(STEPS).await;
	device.listening.take();
	let err = operator.ends().await.unwrap_err();
	assert!(err.to_string().contains("closed the connection"), "{err}");
}

/// Where no daemon answers, the command says it runs only in provisioning mode.
#[tokio::test]
async fn with_no_daemon_the_command_says_where_it_runs() {
	let dir = tempfile::tempdir().unwrap();
	let err = run(&dir.path().join("none.sock")).await.unwrap_err();
	let message = err.to_string();
	assert!(
		message.contains("no provisioning daemon answers"),
		"{message}"
	);
	assert!(message.contains("provisioning mode"), "{message}");
}

/// A client and the console see the one run; the first answer to a prompt wins and the second is
/// refused at the prompt (PRS, RUN).
#[tokio::test]
async fn the_console_and_a_client_share_one_run_and_the_first_answer_wins() {
	let log = Log::default();
	let gate = Gate::new();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![Do::Confirm("go"), Do::Hold(gate.clone())],
		)],
		&log,
	)
	.await;
	let operator = device.operator();
	let mut client = Client::connect(&device.socket()).await;

	operator.says("");
	let prompts = client
		.state(|run, prompts| run == RunState::Waiting && !prompts.is_empty())
		.await;
	assert_eq!(prompts[0].id, "x-go");
	operator.sees("x-go [y/n]> ").await;

	// The console answers first.
	operator.says("y");
	device.logged("x confirmed true").await;
	let refused = client.act(&answer_to("x-go", Answer::boolean(false))).await;
	let Message::CannotAct { act, at, .. } = refused else {
		panic!("the second answer was not refused: {refused:?}");
	};
	assert_eq!((act.as_str(), at.as_str()), ("answer", "$['prompt']"));
	gate.open();
	device.is(RunState::Finished).await;
	operator.sees("The run has finished.").await;
	assert_eq!(log.lines(), ["x begins", "x confirmed true", "x ends"]);
}

/// Where a client answers first, the console learns the prompt has closed, and what the operator
/// typed for it is not taken as the answer to anything else.
#[tokio::test]
async fn an_answer_from_a_client_closes_the_consoles_question() {
	let log = Log::default();
	let device = Device::new(
		vec![Fake::new(
			"x",
			&log,
			vec![Do::Confirm("go"), Do::Confirm("again")],
		)],
		&log,
	)
	.await;
	let operator = device.operator();
	let mut client = Client::connect(&device.socket()).await;

	operator.says("");
	operator.sees("x-go [y/n]> ").await;
	assert_eq!(
		client.act(&answer_to("x-go", Answer::boolean(true))).await,
		Message::ProvisionAccepted
	);
	device.logged("x confirmed true").await;
	operator
		.sees("That question has closed; press Enter to go on.")
		.await;

	// What was typed for the closed prompt does not answer the next one.
	operator.says("n");
	operator
		.sees("That question was settled before you answered")
		.await;
	operator.sees("x-again [y/n]> ").await;
	operator.says("n");
	device.logged("x confirmed false").await;
	device.is(RunState::Finished).await;
}

/// The device refusing an act is printed with where and why (PRS).
#[test]
fn a_refusal_is_printed_with_its_path_and_reason() {
	let screen = Screen::default();
	let mut console = Console::new(screen.clone());
	console
		.hear(&Message::CannotAct {
			act: "answer".to_owned(),
			at: "$['prompt']".to_owned(),
			reason: "that prompt is closed".to_owned(),
		})
		.unwrap();
	assert_eq!(
		screen.0.lock().unwrap().as_str(),
		"The device refused answer at $['prompt']: that prompt is closed\n"
	);
}
