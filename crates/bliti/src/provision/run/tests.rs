use std::{sync::Arc, time::Duration};

use bliti_core::provisioning::report::{DoneState, PromptKind, RunState, StepState};

use super::*;
use crate::provision::{
	fake::{BUILT, Do, Fake, Gate, Log},
	model::Question,
};

const PATIENCE: Duration = Duration::from_secs(5);

fn arcs(steps: Vec<Arc<Fake>>) -> Vec<Arc<dyn Step>> {
	steps
		.into_iter()
		.map(|step| step as Arc<dyn Step>)
		.collect()
}

fn start(steps: &[&str]) -> Act {
	Act {
		act: "start".to_owned(),
		steps: Some(steps.iter().map(|id| (*id).to_owned()).collect()),
		prompt: None,
		value: None,
	}
}

fn act(name: &str) -> Act {
	Act {
		act: name.to_owned(),
		steps: None,
		prompt: None,
		value: None,
	}
}

fn answer(prompt: &str, value: Answer) -> Act {
	Act {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some(prompt.to_owned()),
		value: Some(value),
	}
}

/// Wait until the state satisfies `until`, and return it.
async fn until(provisioner: &Provisioner, until: impl Fn(&Snapshot) -> bool) -> Snapshot {
	let mut changes = provisioner.watch();
	let held = tokio::time::timeout(PATIENCE, changes.wait_for(|snapshot| until(snapshot)))
		.await
		.expect("the state never came to what was waited for")
		.expect("the provisioner outlives its watchers");
	held.clone()
}

/// Wait until the steps have logged `line`.
async fn logged(log: &Log, line: &str) {
	tokio::time::timeout(PATIENCE, async {
		while !log.lines().iter().any(|logged| logged == line) {
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
	})
	.await
	.unwrap_or_else(|_| panic!("{line:?} was never logged: {:?}", log.lines()));
}

async fn finished(provisioner: &Provisioner) -> Snapshot {
	until(provisioner, |snapshot| snapshot.run == RunState::Finished).await
}

fn states(snapshot: &Snapshot) -> Vec<StepState> {
	snapshot.steps.iter().map(|step| step.state).collect()
}

struct Medium(tempfile::TempDir);

impl Medium {
	fn new() -> Self {
		Self(tempfile::tempdir().unwrap())
	}

	fn store(&self) -> Store {
		Store::new(self.0.path(), &[0xaa, 0xbb])
	}

	fn contents(&self) -> String {
		let mut all = String::new();
		for entry in walk(self.0.path()) {
			// A file may be renamed into place between listing it and reading it.
			if let Ok(bytes) = std::fs::read(entry) {
				all.push_str(&String::from_utf8_lossy(&bytes));
			}
		}
		all
	}
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
	let mut files = Vec::new();
	let Ok(entries) = std::fs::read_dir(dir) else {
		return files;
	};
	for entry in entries.flatten() {
		let path = entry.path();
		if path.is_dir() {
			files.extend(walk(&path));
		} else {
			files.push(path);
		}
	}
	files
}

async fn device(steps: Vec<Arc<Fake>>, medium: &Medium) -> Provisioner {
	Provisioner::start(arcs(steps), Some(medium.store()), BUILT).await
}

#[tokio::test]
async fn the_chosen_steps_are_carried_out_in_the_device_order() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![]),
		Fake::new("c", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["c", "a"])).unwrap();
	let done = finished(&device).await;
	assert_eq!(log.lines(), ["a begins", "a ends", "c begins", "c ends"]);
	assert_eq!(
		states(&done),
		[StepState::Passed, StepState::Unchosen, StepState::Passed]
	);
}

#[tokio::test]
async fn a_failing_step_stops_the_run_and_says_why() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![Do::FailUntil(2, "no disk")]),
		Fake::new("c", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b", "c"])).unwrap();
	let failed = until(&device, |s| s.run == RunState::Failed).await;
	assert_eq!(
		states(&failed),
		[StepState::Passed, StepState::Failed, StepState::Pending]
	);
	assert_eq!(failed.steps[1].reason.as_deref(), Some("no disk"));
	assert_eq!(log.lines(), ["a begins", "a ends", "b begins"]);
}

#[tokio::test]
async fn a_retry_carries_out_the_failed_step_again_and_not_the_passed_ones() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![Do::FailUntil(2, "no disk")]),
		Fake::new("c", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b", "c"])).unwrap();
	until(&device, |s| s.run == RunState::Failed).await;
	device.act(&act("retry")).unwrap();
	finished(&device).await;
	assert_eq!(
		log.lines(),
		[
			"a begins", "a ends", "b begins", "b begins", "b ends", "c begins", "c ends"
		]
	);
}

#[tokio::test]
async fn the_run_goes_on_with_no_front_end_attached() {
	let (log, medium) = (Log::default(), Medium::new());
	let gate = Gate::new();
	let steps = vec![
		Fake::new("a", &log, vec![Do::Hold(gate.clone())]),
		Fake::new("b", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	// Nobody has watched the run so far.
	gate.open();
	let done = finished(&device).await;
	assert_eq!(states(&done), [StepState::Passed, StepState::Passed]);
}

#[tokio::test]
async fn progress_and_results_are_reported() {
	let (log, medium) = (Log::default(), Medium::new());
	let gate = Gate::new();
	let steps = vec![Fake::new(
		"a",
		&log,
		vec![
			Do::Progress(0.25),
			Do::Result("abc"),
			Do::Hold(gate.clone()),
		],
	)];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let working = until(&device, |s| {
		s.steps[0].progress.is_some() && s.steps[0].result.is_some()
	})
	.await;
	assert_eq!(working.steps[0].progress, Some(0.25));
	assert_eq!(working.steps[0].state, StepState::Running);
	gate.open();
	let done = finished(&device).await;
	assert_eq!(done.steps[0].progress, None);
	assert_eq!(done.steps[0].result.as_ref().unwrap()["found"], "abc");
}

#[tokio::test]
async fn a_run_is_resumed_after_a_restart_at_the_step_it_was_in() {
	let (log, medium) = (Log::default(), Medium::new());
	let hung = Gate::new();
	let first = device(
		vec![
			Fake::new("a", &log, vec![]),
			Fake::new("b", &log, vec![Do::Hold(hung.clone())]),
			Fake::new("c", &log, vec![]),
		],
		&medium,
	)
	.await;
	first.act(&start(&["a", "b", "c"])).unwrap();
	until(&first, |s| s.steps[1].state == StepState::Running).await;
	logged(&log, "b begins").await;

	// The power goes: a new process starts with the same medium, nothing of the first remembered.
	let second = device(
		vec![
			Fake::new("a", &log, vec![]),
			Fake::new("b", &log, vec![]),
			Fake::new("c", &log, vec![]),
		],
		&medium,
	)
	.await;
	let done = finished(&second).await;
	assert_eq!(
		log.lines(),
		[
			"a begins", "a ends", "b begins", "b begins", "b ends", "c begins", "c ends"
		]
	);
	assert_eq!(states(&done), [StepState::Passed; 3]);
	// Discarded once the finish has been reported.
	tokio::time::timeout(PATIENCE, async {
		while !medium.contents().is_empty() {
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
	})
	.await
	.expect("a finished run left its state on the medium");
}

#[tokio::test]
async fn a_failed_run_stays_failed_across_a_restart_until_retried() {
	let (log, medium) = (Log::default(), Medium::new());
	let first = device(
		vec![
			Fake::new("a", &log, vec![]),
			Fake::new("b", &log, vec![Do::FailUntil(2, "no disk")]),
		],
		&medium,
	)
	.await;
	first.act(&start(&["a", "b"])).unwrap();
	until(&first, |s| s.run == RunState::Failed).await;
	// The failure is kept after the state is reported. Read as the device reads it, so a temporary
	// file not yet renamed into place does not count.
	tokio::time::timeout(PATIENCE, async {
		while medium
			.store()
			.load()
			.ok()
			.flatten()
			.and_then(|stored| stored.failed)
			.is_none()
		{
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
	})
	.await
	.expect("the failure was never kept");

	let second = device(
		vec![Fake::new("a", &log, vec![]), Fake::new("b", &log, vec![])],
		&medium,
	)
	.await;
	let held = second.watch().borrow().clone();
	assert_eq!(held.run, RunState::Failed);
	assert_eq!(held.steps[1].reason.as_deref(), Some("no disk"));
	assert_eq!(states(&held), [StepState::Passed, StepState::Failed]);
	tokio::time::sleep(Duration::from_millis(50)).await;
	assert_eq!(
		log.lines(),
		["a begins", "a ends", "b begins"],
		"nothing ran again"
	);

	second.act(&act("retry")).unwrap();
	finished(&second).await;
	assert_eq!(log.lines().last().unwrap(), "b ends");
}

#[tokio::test]
async fn a_run_that_does_not_match_the_steps_is_discarded() {
	let (log, medium) = (Log::default(), Medium::new());
	let first = device(
		vec![Fake::new("old", &log, vec![Do::Hold(Gate::new())])],
		&medium,
	)
	.await;
	first.act(&start(&["old"])).unwrap();
	logged(&log, "old begins").await;

	let second = device(vec![Fake::new("new", &log, vec![])], &medium).await;
	assert_eq!(second.watch().borrow().run, RunState::Idle);
	assert_eq!(medium.contents(), "");
}

#[tokio::test]
async fn a_secret_never_reaches_the_medium() {
	let (log, medium) = (Log::default(), Medium::new());
	let gate = Gate::new();
	let steps = vec![Fake::new(
		"a",
		&log,
		vec![Do::Secret("password"), Do::Hold(gate.clone())],
	)];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let asked = until(&device, |s| s.run == RunState::Waiting).await;
	assert_eq!(asked.prompts[0].kind, PromptKind::Secret);
	device
		.act(&answer(&asked.prompts[0].id, Answer::text("hunter2")))
		.unwrap();
	logged(&log, "a secret hunter2").await;
	assert!(!medium.contents().is_empty(), "the run itself is kept");
	assert!(!medium.contents().contains("hunter2"));
}

#[tokio::test]
async fn a_second_answer_to_a_prompt_is_refused() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![Fake::new(
		"a",
		&log,
		vec![
			Do::AskLater("name", PromptKind::Text),
			Do::Hold(Gate::new()),
		],
	)];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let asked = until(&device, |s| !s.prompts.is_empty()).await;
	assert!(!asked.prompts[0].blocking);
	assert_eq!(asked.run, RunState::Running);

	let id = asked.prompts[0].id.clone();
	device.act(&answer(&id, Answer::text("Ada"))).unwrap();
	let refused = device.act(&answer(&id, Answer::text("Bo"))).unwrap_err();
	assert_eq!(refused.at, "$['prompt']");
	assert!(device.watch().borrow().prompts.is_empty());
}

#[tokio::test]
async fn an_answer_given_early_is_used_by_the_step_that_needs_it() {
	let (log, medium) = (Log::default(), Medium::new());
	let hold = Gate::new();
	let steps = vec![
		Fake::new(
			"a",
			&log,
			vec![
				Do::AskLater("name", PromptKind::Text),
				Do::Hold(hold.clone()),
			],
		),
		Fake::new("b", &log, vec![Do::Text("name")]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	let asked = until(&device, |s| !s.prompts.is_empty()).await;
	device
		.act(&answer(&asked.prompts[0].id, Answer::text("Ada")))
		.unwrap();
	hold.open();
	finished(&device).await;
	assert!(log.lines().contains(&"b text Ada".to_owned()));
}

#[tokio::test]
async fn a_step_whose_turn_comes_without_the_answer_waits_for_it() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![Do::AskLater("name", PromptKind::Text)]),
		Fake::new("b", &log, vec![Do::Text("name")]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	let waiting = until(&device, |s| s.run == RunState::Waiting).await;
	assert_eq!(waiting.steps[1].state, StepState::Waiting);
	assert!(waiting.prompts[0].blocking);
	device
		.act(&answer(&waiting.prompts[0].id, Answer::text("Ada")))
		.unwrap();
	finished(&device).await;
	assert!(log.lines().contains(&"b text Ada".to_owned()));
}

#[tokio::test]
async fn a_notice_holds_the_step_until_it_is_closed() {
	let (log, medium) = (Log::default(), Medium::new());
	let gate = Gate::new();
	let steps = vec![Fake::new(
		"a",
		&log,
		vec![Do::Notice("Press SW1", gate.clone())],
	)];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let waiting = until(&device, |s| s.run == RunState::Waiting).await;
	assert_eq!(waiting.prompts[0].kind, PromptKind::Notice);
	assert_eq!(waiting.prompts[0].text, "Press SW1");
	gate.open();
	let done = finished(&device).await;
	assert!(done.prompts.is_empty());
}

#[tokio::test]
async fn stopping_a_waiting_run_ends_it() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![Do::Notice("Press SW1", Gate::new())]),
		Fake::new("b", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	until(&device, |s| s.run == RunState::Waiting).await;
	device.act(&act("stop")).unwrap();
	let idle = until(&device, |s| s.run == RunState::Idle).await;
	assert!(idle.prompts.is_empty());
	assert_eq!(states(&idle), [StepState::Unchosen; 2]);
	assert_eq!(medium.contents(), "");
	tokio::time::sleep(Duration::from_millis(50)).await;
	assert_eq!(log.lines(), ["a begins"]);
}

#[tokio::test]
async fn a_finished_run_is_held_until_acknowledged_or_replaced() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![Do::Result("abc")]),
		Fake::new("b", &log, vec![]),
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let done = finished(&device).await;
	assert_eq!(done.steps[0].result.as_ref().unwrap()["found"], "abc");

	device.act(&act("acknowledge")).unwrap();
	let idle = until(&device, |s| s.run == RunState::Idle).await;
	assert_eq!(idle.steps[0].result, None);

	device.act(&start(&["a"])).unwrap();
	finished(&device).await;
	device.act(&start(&["b"])).unwrap();
	let replaced = finished(&device).await;
	assert_eq!(states(&replaced), [StepState::Unchosen, StepState::Passed]);
	assert_eq!(replaced.steps[0].result, None);
}

#[tokio::test]
async fn an_act_is_refused_at_the_member_at_fault() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![
		Fake::new("a", &log, vec![Do::Hold(Gate::new())]),
		Fake::new("b", &log, vec![]),
	];
	let device = device(steps, &medium).await;

	let at = |result: Result<(), CannotAct>| result.unwrap_err().at;
	assert_eq!(at(device.act(&act("dance"))), "$['act']");
	assert_eq!(at(device.act(&act("start"))), "$['steps']");
	assert_eq!(at(device.act(&start(&[]))), "$['steps']");
	assert_eq!(at(device.act(&start(&["a", "zz", "yy"]))), "$['steps'][1]");
	for wrong_state in ["retry", "stop", "acknowledge", "answer"] {
		assert_eq!(
			at(device.act(&act(wrong_state))),
			"$",
			"{wrong_state} while idle"
		);
	}

	device.act(&start(&["a"])).unwrap();
	until(&device, |s| s.run == RunState::Running).await;
	for wrong_state in ["start", "retry", "stop", "acknowledge"] {
		let refused = if wrong_state == "start" {
			start(&["a"])
		} else {
			act(wrong_state)
		};
		assert_eq!(at(device.act(&refused)), "$", "{wrong_state} while running");
	}
	assert_eq!(
		at(device.act(&answer("nothing", Answer::boolean(true)))),
		"$['prompt']"
	);
}

#[tokio::test]
async fn an_answer_of_the_wrong_kind_is_refused_at_the_value() {
	let (log, medium) = (Log::default(), Medium::new());
	let steps = vec![Fake::new("a", &log, vec![Do::Confirm("ok")])];
	let device = device(steps, &medium).await;
	device.act(&start(&["a"])).unwrap();
	let asked = until(&device, |s| s.run == RunState::Waiting).await;
	let refused = device
		.act(&answer(&asked.prompts[0].id, Answer::text("yes")))
		.unwrap_err();
	assert_eq!(refused.at, "$['value']");
	device
		.act(&answer(&asked.prompts[0].id, Answer::boolean(true)))
		.unwrap();
	finished(&device).await;
	assert!(log.lines().contains(&"a confirmed true".to_owned()));
}

#[tokio::test]
async fn a_board_with_nowhere_to_keep_a_run_is_not_started_on() {
	let log = Log::default();
	let device = Provisioner::start(arcs(vec![Fake::new("a", &log, vec![])]), None, BUILT).await;
	let refused = device.act(&start(&["a"])).unwrap_err();
	assert_eq!(refused.at, "$");
}

#[tokio::test]
async fn done_states_are_read_from_the_board_when_idle_and_when_finished() {
	let (log, medium) = (Log::default(), Medium::new());
	let (a, b) = (Fake::new("a", &log, vec![]), Fake::new("b", &log, vec![]));
	a.done_is(DoneState::Done, Some("Up to date"));
	b.done_is(DoneState::Unknown, None);
	let gate = Gate::new();
	let c = Fake::new("c", &log, vec![Do::Hold(gate.clone())]);
	let device = device(vec![a.clone(), b.clone(), c], &medium).await;

	let idle = until(&device, |s| s.steps[0].done == DoneState::Done).await;
	assert_eq!(idle.steps[0].why.as_deref(), Some("Up to date"));
	assert_eq!(idle.steps[1].done, DoneState::Unknown);

	// A front end attaching asks again, and gets what the board says now.
	a.done_is(DoneState::NotDone, None);
	device.attach();
	until(&device, |s| s.steps[0].done == DoneState::NotDone).await;

	// Not looked at again while a run is under way, and looked at once it finishes.
	device.act(&start(&["c"])).unwrap();
	until(&device, |s| s.run == RunState::Running).await;
	a.done_is(DoneState::Done, Some("Written"));
	device.attach();
	tokio::time::sleep(Duration::from_millis(50)).await;
	assert_eq!(device.watch().borrow().steps[0].done, DoneState::NotDone);
	gate.open();
	until(&device, |s| {
		s.run == RunState::Finished && s.steps[0].why.as_deref() == Some("Written")
	})
	.await;
}

#[tokio::test]
async fn done_states_are_unknown_until_determined() {
	let log = Log::default();
	let medium = Medium::new();
	let a = Fake::new("a", &log, vec![]);
	a.done_is(DoneState::Done, None);
	let steps = arcs(vec![a]);
	let (changes, _) = (
		Provisioner::start(steps, Some(medium.store()), BUILT).await,
		(),
	);
	// Determined in the background, so the first state held may not have it.
	let held = changes.watch().borrow().clone();
	assert!(matches!(
		held.steps[0].done,
		DoneState::Unknown | DoneState::Done
	));
	until(&changes, |s| s.steps[0].done == DoneState::Done).await;
}

#[tokio::test]
async fn a_step_that_panics_fails_the_run() {
	use async_trait::async_trait;

	struct Panics;

	#[async_trait]
	impl Step for Panics {
		fn id(&self) -> &str {
			"p"
		}
		fn title(&self) -> &str {
			"p"
		}
		async fn done(&self) -> Done {
			Done {
				state: DoneState::NotDone,
				why: None,
			}
		}
		async fn run(&self, _context: &mut Context) -> Result<(), String> {
			panic!("on purpose")
		}
	}

	let medium = Medium::new();
	let device = Provisioner::start(vec![Arc::new(Panics)], Some(medium.store()), BUILT).await;
	device.act(&start(&["p"])).unwrap();
	let failed = until(&device, |s| s.run == RunState::Failed).await;
	assert_eq!(failed.steps[0].reason.as_deref(), Some("the step panicked"));
}

fn later(step: &Fake, after: &'static str) {
	step.asks_ahead(
		after,
		Question::new("q", PromptKind::Text, "Q?").default("dflt"),
	);
}

#[tokio::test]
async fn a_step_asks_ahead_once_the_step_it_waits_for_has_passed() {
	let (log, medium) = (Log::default(), Medium::new());
	let (first, second) = (Gate::new(), Gate::new());
	let asker = Fake::new("c", &log, vec![Do::TakeText("q", Some("fallback"))]);
	later(&asker, "a");
	let steps = vec![
		Fake::new("a", &log, vec![Do::Hold(first.clone())]),
		Fake::new("b", &log, vec![Do::Hold(second.clone())]),
		asker,
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b", "c"])).unwrap();
	logged(&log, "a begins").await;
	assert!(device.watch().borrow().prompts.is_empty());

	first.open();
	let asked = until(&device, |s| !s.prompts.is_empty()).await;
	assert_eq!(asked.prompts[0].step, "c");
	assert_eq!(asked.prompts[0].default.as_deref(), Some("dflt"));
	assert!(!asked.prompts[0].blocking);
	assert_eq!(asked.run, RunState::Running);
	assert_eq!(asked.steps[2].state, StepState::Pending);

	// The step takes the answer it was given, without waiting.
	device.act(&answer("q", Answer::text("Ada"))).unwrap();
	second.open();
	finished(&device).await;
	assert!(log.lines().contains(&"c took Some(\"Ada\")".to_owned()));
}

#[tokio::test]
async fn an_unanswered_prompt_is_closed_and_defaulted_when_the_step_comes_to_it() {
	let (log, medium) = (Log::default(), Medium::new());
	let hold = Gate::new();
	let asker = Fake::new("b", &log, vec![Do::TakeText("q", Some("fallback"))]);
	later(&asker, "a");
	let steps = vec![Fake::new("a", &log, vec![Do::Hold(hold.clone())]), asker];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	logged(&log, "a begins").await;
	hold.open();
	let done = finished(&device).await;
	assert!(done.prompts.is_empty());
	assert!(
		log.lines()
			.contains(&"b took Some(\"fallback\")".to_owned())
	);
}

#[tokio::test]
async fn a_step_asks_at_the_start_where_the_step_it_waits_for_is_not_chosen() {
	let (log, medium) = (Log::default(), Medium::new());
	let hold = Gate::new();
	let asker = Fake::new("c", &log, vec![]);
	later(&asker, "a");
	let steps = vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![Do::Hold(hold.clone())]),
		asker,
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["b", "c"])).unwrap();
	let asked = until(&device, |s| !s.prompts.is_empty()).await;
	assert_eq!(asked.steps[1].state, StepState::Running);
	assert!(!asked.prompts[0].blocking);
}

#[tokio::test]
async fn the_prompts_are_asked_again_by_a_run_resumed_after_a_restart() {
	let (log, medium) = (Log::default(), Medium::new());
	let hung = Gate::new();
	let asker = Fake::new("c", &log, vec![Do::TakeText("q", None)]);
	later(&asker, "a");
	let first = device(
		vec![
			Fake::new("a", &log, vec![]),
			Fake::new("b", &log, vec![Do::Hold(hung.clone())]),
			asker,
		],
		&medium,
	)
	.await;
	first.act(&start(&["a", "b", "c"])).unwrap();
	until(&first, |s| !s.prompts.is_empty()).await;
	first.act(&answer("q", Answer::text("Ada"))).unwrap();

	// The power goes: the answer was in memory only, so a new process asks again.
	let asker = Fake::new("c", &log, vec![]);
	later(&asker, "a");
	let second = device(
		vec![
			Fake::new("a", &log, vec![]),
			Fake::new("b", &log, vec![Do::Hold(Gate::new())]),
			asker,
		],
		&medium,
	)
	.await;
	let asked = until(&second, |s| !s.prompts.is_empty()).await;
	assert_eq!(asked.prompts[0].id, "q");
	assert!(!asked.prompts[0].blocking);
}

#[tokio::test]
async fn a_forgotten_answer_is_asked_for_again_when_the_step_is_retried() {
	let (log, medium) = (Log::default(), Medium::new());
	let asker = Fake::new(
		"b",
		&log,
		vec![Do::Text("q"), Do::Forget("q"), Do::FailUntil(2, "no good")],
	);
	later(&asker, "a");
	let steps = vec![Fake::new("a", &log, vec![]), asker];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b"])).unwrap();
	let first = until(&device, |s| s.run == RunState::Waiting).await;
	device
		.act(&answer(&first.prompts[0].id, Answer::text("one")))
		.unwrap();
	until(&device, |s| s.run == RunState::Failed).await;

	device.act(&act("retry")).unwrap();
	let again = until(&device, |s| s.run == RunState::Waiting).await;
	assert!(again.prompts[0].blocking);
	device.act(&answer("q", Answer::text("two"))).unwrap();
	finished(&device).await;
	assert!(log.lines().contains(&"b text two".to_owned()));
}

#[tokio::test]
async fn an_answer_a_step_refuses_is_refused_at_the_value() {
	let (log, medium) = (Log::default(), Medium::new());
	let hold = Gate::new();
	let asker = Fake::new("c", &log, vec![]);
	asker.asks_ahead(
		"a",
		Question::new("q", PromptKind::Text, "Q?").check(|text| {
			if text.is_empty() {
				Err("say something".to_owned())
			} else {
				Ok(())
			}
		}),
	);
	let steps = vec![
		Fake::new("a", &log, vec![]),
		Fake::new("b", &log, vec![Do::Hold(hold.clone())]),
		asker,
	];
	let device = device(steps, &medium).await;
	device.act(&start(&["a", "b", "c"])).unwrap();
	until(&device, |s| !s.prompts.is_empty()).await;

	let refused = device.act(&answer("q", Answer::text(""))).unwrap_err();
	assert_eq!(refused.reason, "say something");
	assert_eq!(refused.at, "$['value']");
	assert_eq!(device.watch().borrow().prompts.len(), 1);

	device.act(&answer("q", Answer::text("fine"))).unwrap();
	assert!(device.watch().borrow().prompts.is_empty());
}
