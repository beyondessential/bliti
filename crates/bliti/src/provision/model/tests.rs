use super::*;

fn model() -> Model {
	Model::new(
		crate::provision::fake::BUILT,
		[
			("one".to_owned(), "One".to_owned()),
			("two".to_owned(), "Two".to_owned()),
		],
	)
}

#[test]
fn a_fresh_model_holds_no_run() {
	let model = model();
	let snapshot = model.snapshot();
	assert_eq!(snapshot.run, RunState::Idle);
	assert!(
		snapshot
			.steps
			.iter()
			.all(|s| s.state == StepState::Unchosen)
	);
	assert!(snapshot.steps.iter().all(|s| s.done == DoneState::Unknown));
	assert!(snapshot.prompts.is_empty());
}

#[test]
fn a_run_is_waiting_only_while_a_blocking_prompt_is_open() {
	let mut model = model();
	model.begin_run(&[0, 1]);
	let step = model.begin_next().unwrap();
	assert_eq!(model.run(), RunState::Running);

	model.ask_later(step, "name", PromptKind::Text, "Name?");
	assert_eq!(
		model.run(),
		RunState::Running,
		"a prompt that does not block"
	);
	assert_eq!(model.snapshot().steps[0].state, StepState::Running);

	let Need::Wait(_) = model.need(step, "name", PromptKind::Text, "Name?") else {
		panic!("the answer is not held");
	};
	assert_eq!(model.run(), RunState::Waiting);
	assert_eq!(model.snapshot().steps[0].state, StepState::Waiting);
	assert_eq!(model.snapshot().steps[1].state, StepState::Pending);
}

#[test]
fn a_prompt_opened_earlier_is_the_one_a_step_then_waits_on() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	model.ask_later(step, "name", PromptKind::Text, "Name?");
	let Need::Wait(_) = model.need(step, "name", PromptKind::Text, "Name?") else {
		panic!()
	};
	assert_eq!(model.prompts.len(), 1);
	assert!(model.prompts[0].blocking());
}

#[test]
fn an_answer_is_held_and_given_to_the_step_that_needs_it() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	model.ask_later(step, "name", PromptKind::Text, "Name?");
	model
		.answer(Some("one-name"), Some(&Answer::text("Ada")))
		.unwrap();
	assert!(model.prompts.is_empty());
	let Need::Held(answer) = model.need(step, "name", PromptKind::Text, "Name?") else {
		panic!("an answered question is not asked again");
	};
	assert_eq!(answer.as_text(), Some("Ada"));
}

#[test]
fn an_answer_reaches_the_step_waiting_for_it() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	let Need::Wait(mut answer) = model.need(step, "ok", PromptKind::Confirm, "Sure?") else {
		panic!()
	};
	model
		.answer(Some("one-ok"), Some(&Answer::boolean(true)))
		.unwrap();
	assert_eq!(answer.try_recv().unwrap().as_bool(), Some(true));
	assert_eq!(model.run(), RunState::Running);
}

#[test]
fn a_second_answer_is_refused_at_the_prompt() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	model.ask_later(step, "name", PromptKind::Text, "Name?");
	model
		.answer(Some("one-name"), Some(&Answer::text("Ada")))
		.unwrap();
	let refused = model
		.answer(Some("one-name"), Some(&Answer::text("Bo")))
		.unwrap_err();
	assert_eq!(refused.at, "$['prompt']");
}

#[test]
fn a_notice_takes_no_answer() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	let id = model.notice(step, "Press SW1");
	assert_eq!(model.run(), RunState::Waiting);
	let refused = model
		.answer(Some(&id), Some(&Answer::boolean(true)))
		.unwrap_err();
	assert_eq!(refused.at, "$['prompt']");
	model.close(&id);
	assert_eq!(model.run(), RunState::Running);
}

#[test]
fn an_answer_of_the_wrong_type_is_refused_at_the_value() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	model.ask_later(step, "ok", PromptKind::Confirm, "Sure?");
	model.ask_later(step, "name", PromptKind::Secret, "Password?");
	for (id, value) in [
		("one-ok", Some(Answer::text("yes"))),
		("one-ok", None),
		("one-name", Some(Answer::boolean(true))),
		("one-name", None),
	] {
		let refused = model.answer(Some(id), value.as_ref()).unwrap_err();
		assert_eq!(refused.at, "$['value']", "{id} {value:?}");
	}
	assert_eq!(
		model.prompts.len(),
		2,
		"a refused answer leaves the prompt open"
	);
	assert!(model.answer(None, Some(&Answer::boolean(true))).is_err());
}

#[test]
fn a_failed_step_stops_the_run_and_a_retry_resumes_it() {
	let mut model = model();
	model.begin_run(&[0, 1]);
	let first = model.begin_next().unwrap();
	model.pass(first);
	let second = model.begin_next().unwrap();
	model.fail(second, "no disk".to_owned());
	assert_eq!(model.run(), RunState::Failed);
	assert_eq!(model.snapshot().steps[1].reason.as_deref(), Some("no disk"));
	assert_eq!(model.next(), None);

	assert_eq!(model.retry(), Some(1));
	assert_eq!(model.run(), RunState::Running);
	assert_eq!(model.snapshot().steps[0].state, StepState::Passed);
	assert_eq!(model.snapshot().steps[1].reason, None);
	assert_eq!(model.begin_next(), Some(1));
}

#[test]
fn finishing_closes_every_prompt_and_forgets_every_answer() {
	let mut model = model();
	model.begin_run(&[0]);
	let step = model.begin_next().unwrap();
	model.ask_later(step, "late", PromptKind::Text, "Later?");
	model.pass(step);
	model.finish();
	assert_eq!(model.run(), RunState::Finished);
	assert!(model.prompts.is_empty());
	assert_eq!(model.snapshot().steps[0].state, StepState::Passed);
	assert_eq!(model.snapshot().steps[1].state, StepState::Unchosen);
	assert!(model.at_rest());
}

#[test]
fn clearing_a_run_leaves_every_step_unchosen() {
	let mut model = model();
	model.begin_run(&[0, 1]);
	model.begin_next();
	model.clear_run();
	assert_eq!(model.run(), RunState::Idle);
	assert!(
		model
			.snapshot()
			.steps
			.iter()
			.all(|s| s.state == StepState::Unchosen)
	);
	assert_eq!(model.chosen(), Vec::<String>::new());
}

#[test]
fn a_prompt_one_step_asked_is_waited_on_by_the_step_that_needs_it() {
	let mut model = model();
	model.begin_run(&[0, 1]);
	let first = model.begin_next().unwrap();
	model.ask_later(first, "name", PromptKind::Text, "Name?");
	model.pass(first);
	model.release(first);
	assert_eq!(
		model.prompts.len(),
		1,
		"the question outlives the step that asked"
	);

	let second = model.begin_next().unwrap();
	let Need::Wait(_) = model.need(second, "name", PromptKind::Text, "Name?") else {
		panic!()
	};
	assert_eq!(model.prompts.len(), 1);
	let snapshot = model.snapshot();
	assert_eq!(snapshot.run, RunState::Waiting);
	assert_eq!(snapshot.steps[1].state, StepState::Waiting);
	assert_eq!(snapshot.prompts[0].step, "one");
	assert!(snapshot.prompts[0].blocking);

	// The step ends without it: the question stays, and holds nothing up.
	model.release(second);
	assert_eq!(model.run(), RunState::Running);
	assert_eq!(model.prompts.len(), 1);
	assert!(!model.prompts[0].blocking());
}
