//! A run's state turned into the plain lines that say what changed (PCON).
//!
//! The console prints lines as the state changes and never redraws, since it may be on a serial
//! line.

use std::collections::BTreeMap;

use bliti_core::provisioning::report::{PromptKind, PromptReport, RunState, StepReport, StepState};
use serde_json::{Map, Value as Json};

use super::question::asks;
use crate::provision::Snapshot;

/// Remembers what has been shown, so each change is said once.
#[derive(Debug, Default)]
pub struct Reporter {
	last: Option<Snapshot>,
	/// The tenth of its work each running step was last reported at.
	progress: BTreeMap<String, u32>,
	/// The result of each step as last printed.
	shown: BTreeMap<String, Map<String, Json>>,
}

impl Reporter {
	/// The lines that say how `now` differs from the state last reported: every part of it, the first
	/// time.
	pub fn update(&mut self, now: &Snapshot) -> Vec<String> {
		let before = self.last.replace(now.clone());
		let mut lines = Vec::new();

		// spec: PCON
		if before
			.as_ref()
			.is_none_or(|before| (before.revision, before.built) != (now.revision, now.built))
		{
			lines.push(format!(
				"Provisioning image built {}, steps revision {}.",
				now.built, now.revision
			));
		}

		// spec: PCON
		if before.as_ref().is_none_or(|before| before.run != now.run) {
			lines.push(run_line(now.run).to_owned());
		}

		for step in &now.steps {
			let was = before
				.as_ref()
				.and_then(|before| before.steps.iter().find(|was| was.id == step.id));
			let was_state = was.map_or(StepState::Unchosen, |was| was.state);
			if step.state != was_state {
				lines.extend(state_line(step));
			}
			self.report_progress(step, &mut lines);
			self.report_result(step, &mut lines);
		}

		let was_open = before.as_ref().map(|before| &before.prompts[..]);
		for prompt in &now.prompts {
			if !was_open.is_some_and(|open| open.iter().any(|was| was.id == prompt.id)) {
				lines.extend(prompt_lines(prompt, &now.steps));
			}
		}
		if now.run != RunState::Idle {
			for gone in was_open.unwrap_or_default() {
				if gone.kind == PromptKind::Notice && !now.prompts.iter().any(|p| p.id == gone.id) {
					lines.push(format!("Done: {}", gone.text));
				}
			}
		}
		lines
	}

	fn report_progress(&mut self, step: &StepReport, lines: &mut Vec<String>) {
		let under_way = matches!(step.state, StepState::Running | StepState::Waiting);
		let Some(fraction) = step.progress.filter(|_| under_way) else {
			self.progress.remove(&step.id);
			return;
		};
		let tenth = (fraction.clamp(0.0, 1.0) * 10.0).floor() as u32;
		if tenth > 0 && self.progress.get(&step.id) != Some(&tenth) {
			self.progress.insert(step.id.clone(), tenth);
			lines.push(format!(
				"{}: {}%",
				step.title,
				(fraction * 100.0).round() as u32
			));
		}
	}

	fn report_result(&mut self, step: &StepReport, lines: &mut Vec<String>) {
		let Some(result) = &step.result else {
			self.shown.remove(&step.id);
			return;
		};
		if self.shown.get(&step.id) != Some(result) {
			// spec: PCON
			lines.extend(result_lines(&step.title, result));
			self.shown.insert(step.id.clone(), result.clone());
		}
	}

	/// Whether every result `now` carries has been printed, which a run is not acknowledged before.
	pub fn all_shown(&self, now: &Snapshot) -> bool {
		now.steps.iter().all(|step| {
			step.result
				.as_ref()
				.is_none_or(|result| self.shown.get(&step.id) == Some(result))
		})
	}
}

fn run_line(state: RunState) -> &'static str {
	match state {
		RunState::Idle => "No run is held; the device is idle.",
		RunState::Running => "A run is under way.",
		RunState::Waiting => "A run is under way and is waiting for a person.",
		RunState::Failed => "A run has stopped at a failed step.",
		RunState::Finished => "The run has finished.",
	}
}

fn state_line(step: &StepReport) -> Option<String> {
	let words = match step.state {
		StepState::Unchosen => return None,
		StepState::Pending => "queued".to_owned(),
		StepState::Running => "running".to_owned(),
		StepState::Waiting => "waiting for you".to_owned(),
		StepState::Passed => "passed".to_owned(),
		StepState::Failed => format!(
			"failed: {}",
			step.reason.as_deref().unwrap_or("no reason given")
		),
	};
	Some(format!("{}: {words}", step.title))
}

fn prompt_lines(prompt: &PromptReport, steps: &[StepReport]) -> Vec<String> {
	let asker = steps
		.iter()
		.find(|step| step.id == prompt.step)
		.map_or(prompt.step.as_str(), |step| step.title.as_str());
	match &prompt.kind {
		// spec: RUN
		PromptKind::Notice => vec![
			format!("{asker} asks you to do this: {}", prompt.text),
			"  (This closes by itself once it is done.)".to_owned(),
		],
		kind if asks(kind).is_some() => {
			let later = if prompt.blocking {
				""
			} else {
				" (the run goes on meanwhile)"
			};
			let mut lines = vec![format!(
				"{asker} asks{later} [{}]: {}",
				prompt.id, prompt.text
			)];
			if let Some(default) = prompt
				.default
				.as_ref()
				.filter(|_| *kind == PromptKind::Text)
			{
				lines.push(format!("  Press Enter to answer {default}."));
			}
			lines
		}
		// spec: PRS
		kind => vec![format!(
			"{asker} shows a {} prompt this console cannot answer [{}]: {}",
			kind.as_str(),
			prompt.id,
			prompt.text
		)],
	}
}

/// A step's result, member by member.
fn result_lines(title: &str, result: &Map<String, Json>) -> Vec<String> {
	let mut lines = vec![format!("{title} reports:")];
	for (member, value) in result {
		let value = match value {
			Json::String(text) => text.clone(),
			Json::Null => "none".to_owned(),
			other => other.to_string(),
		};
		let mut text = value.lines();
		lines.push(format!("  {member}: {}", text.next().unwrap_or_default()));
		lines.extend(text.map(|rest| format!("    {rest}")));
	}
	lines
}

#[cfg(test)]
mod tests {
	use bliti_core::provisioning::report::DoneState;
	use serde_json::json;

	use super::*;

	fn step(id: &str, state: StepState) -> StepReport {
		StepReport {
			id: id.to_owned(),
			title: id.to_uppercase(),
			done: DoneState::NotDone,
			why: None,
			state,
			progress: None,
			reason: None,
			result: None,
		}
	}

	fn snapshot(run: RunState, steps: Vec<StepReport>, prompts: Vec<PromptReport>) -> Snapshot {
		Snapshot {
			revision: 1,
			built: jiff::civil::date(2026, 10, 29),
			run,
			steps,
			prompts,
		}
	}

	fn prompt(id: &str, kind: PromptKind, blocking: bool) -> PromptReport {
		PromptReport {
			id: id.to_owned(),
			step: "a".to_owned(),
			kind,
			text: format!("text of {id}"),
			blocking,
			default: None,
		}
	}

	#[test]
	fn an_idle_device_is_said_idle_once() {
		let mut reporter = Reporter::default();
		let idle = snapshot(RunState::Idle, vec![step("a", StepState::Unchosen)], vec![]);
		assert_eq!(
			reporter.update(&idle),
			[
				"Provisioning image built 2026-10-29, steps revision 1.",
				"No run is held; the device is idle."
			]
		);
		assert!(reporter.update(&idle).is_empty());
	}

	#[test]
	fn joining_a_run_describes_all_of_it() {
		let mut reporter = Reporter::default();
		let mut passed = step("a", StepState::Passed);
		passed.result = json!({"found": "yes", "n": 3, "list": [1, 2]})
			.as_object()
			.cloned();
		let mut failed = step("b", StepState::Failed);
		failed.reason = Some("it broke".to_owned());
		let lines = reporter.update(&snapshot(
			RunState::Failed,
			vec![
				passed,
				failed,
				step("c", StepState::Pending),
				step("d", StepState::Unchosen),
			],
			vec![],
		));
		assert_eq!(
			lines,
			[
				"Provisioning image built 2026-10-29, steps revision 1.",
				"A run has stopped at a failed step.",
				"A: passed",
				"A reports:",
				"  found: yes",
				"  list: [1,2]",
				"  n: 3",
				"B: failed: it broke",
				"C: queued",
			]
		);
	}

	#[test]
	fn a_new_revision_or_image_date_is_said_again() {
		let mut reporter = Reporter::default();
		let idle = snapshot(RunState::Idle, vec![step("a", StepState::Unchosen)], vec![]);
		reporter.update(&idle);
		let mut newer = idle.clone();
		newer.revision = 2;
		assert_eq!(
			reporter.update(&newer),
			["Provisioning image built 2026-10-29, steps revision 2."]
		);
		newer.built = jiff::civil::date(2026, 11, 5);
		assert_eq!(
			reporter.update(&newer),
			["Provisioning image built 2026-11-05, steps revision 2."]
		);
	}

	#[test]
	fn only_what_changed_is_said() {
		let mut reporter = Reporter::default();
		let before = snapshot(
			RunState::Running,
			vec![step("a", StepState::Running), step("b", StepState::Pending)],
			vec![],
		);
		reporter.update(&before);
		let after = snapshot(
			RunState::Running,
			vec![step("a", StepState::Passed), step("b", StepState::Running)],
			vec![],
		);
		assert_eq!(reporter.update(&after), ["A: passed", "B: running"]);
	}

	/// Progress is said at each tenth, not on every change.
	#[test]
	fn progress_is_said_at_each_tenth() {
		let mut reporter = Reporter::default();
		let mut said = Vec::new();
		for fraction in [0.0, 0.02, 0.1, 0.12, 0.19, 0.2, 0.55, 1.0] {
			let mut running = step("a", StepState::Running);
			running.progress = Some(fraction);
			let lines = reporter.update(&snapshot(RunState::Running, vec![running], vec![]));
			said.extend(lines.into_iter().filter(|line| line.contains('%')));
		}
		assert_eq!(said, ["A: 10%", "A: 20%", "A: 55%", "A: 100%"]);
	}

	#[test]
	fn a_failure_says_why_and_a_result_is_printed_once() {
		let mut reporter = Reporter::default();
		reporter.update(&snapshot(
			RunState::Running,
			vec![step("a", StepState::Running)],
			vec![],
		));
		let mut failed = step("a", StepState::Failed);
		failed.reason = Some("no disk".to_owned());
		failed.result = json!({"line1": "x\ny"}).as_object().cloned();
		let state = snapshot(RunState::Failed, vec![failed], vec![]);
		assert!(!reporter.all_shown(&state));
		assert_eq!(
			reporter.update(&state),
			[
				"A run has stopped at a failed step.",
				"A: failed: no disk",
				"A reports:",
				"  line1: x",
				"    y",
			]
		);
		assert!(reporter.all_shown(&state));
		assert!(reporter.update(&state).is_empty());
	}

	#[test]
	fn the_default_of_a_text_prompt_is_offered() {
		let mut reporter = Reporter::default();
		let steps = vec![step("a", StepState::Pending)];
		reporter.update(&snapshot(RunState::Running, steps.clone(), vec![]));
		let mut name = prompt("name", PromptKind::Text, false);
		name.default = Some("tamanu-iti-k7qa".to_owned());
		let lines = reporter.update(&snapshot(RunState::Running, steps, vec![name]));
		assert_eq!(
			lines,
			[
				"A asks (the run goes on meanwhile) [name]: text of name",
				"  Press Enter to answer tamanu-iti-k7qa.",
			]
		);
	}

	#[test]
	fn a_prompt_is_shown_when_it_opens_by_what_it_asks() {
		let mut reporter = Reporter::default();
		let steps = vec![step("a", StepState::Waiting)];
		reporter.update(&snapshot(RunState::Waiting, steps.clone(), vec![]));
		let prompts = vec![
			prompt("p-notice", PromptKind::Notice, true),
			prompt("p-confirm", PromptKind::Confirm, true),
			prompt("p-later", PromptKind::Text, false),
			prompt("p-odd", PromptKind::Other("dial".to_owned()), true),
		];
		let lines = reporter.update(&snapshot(RunState::Waiting, steps.clone(), prompts.clone()));
		assert_eq!(
			lines,
			[
				"A asks you to do this: text of p-notice",
				"  (This closes by itself once it is done.)",
				"A asks [p-confirm]: text of p-confirm",
				"A asks (the run goes on meanwhile) [p-later]: text of p-later",
				"A shows a dial prompt this console cannot answer [p-odd]: text of p-odd",
			]
		);
		assert!(
			reporter
				.update(&snapshot(RunState::Waiting, steps.clone(), prompts))
				.is_empty()
		);

		let after = snapshot(RunState::Running, steps, vec![]);
		assert_eq!(
			reporter.update(&after),
			["A run is under way.", "Done: text of p-notice"]
		);
	}

	/// A run that is replaced prints its results afresh.
	#[test]
	fn a_second_run_prints_its_results_again() {
		let mut reporter = Reporter::default();
		let mut passed = step("a", StepState::Passed);
		passed.result = json!({"found": "yes"}).as_object().cloned();
		let finished = snapshot(RunState::Finished, vec![passed], vec![]);
		reporter.update(&finished);
		reporter.update(&snapshot(
			RunState::Idle,
			vec![step("a", StepState::Unchosen)],
			vec![],
		));
		assert!(
			reporter
				.update(&finished)
				.iter()
				.any(|line| line == "A reports:")
		);
	}
}
