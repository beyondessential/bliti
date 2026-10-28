//! What the console asks the operator next, and how what they type is read (PCON).

use std::collections::BTreeSet;

use bliti_core::provisioning::report::{Answer, PromptKind, RunState};

use crate::provision::Snapshot;

/// What a prompt kind this console knows takes as an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asks {
	Confirm,
	Text,
	/// Text that is not echoed.
	Secret,
}

/// How the console answers a prompt of `kind`, or nothing where it does not answer that kind: a
/// `notice` closes by itself, and one it does not know is shown and left (PRS).
pub fn asks(kind: &PromptKind) -> Option<Asks> {
	match kind {
		PromptKind::Confirm => Some(Asks::Confirm),
		PromptKind::Text => Some(Asks::Text),
		PromptKind::Secret => Some(Asks::Secret),
		PromptKind::Notice | PromptKind::Other(_) => None,
	}
}

/// The one thing the console is asking the operator at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
	/// Which steps to run, with the device idle.
	Choose,
	/// The answer to an open prompt.
	Prompt { id: String, asks: Asks },
	/// What to do about a run waiting on a person.
	Waiting,
	/// What to do about a failed run.
	Failed,
	/// Whether to discard a finished run.
	Finished,
}

impl Question {
	/// Whether what is typed is kept from the screen.
	pub fn hidden(&self) -> bool {
		matches!(
			self,
			Self::Prompt {
				asks: Asks::Secret,
				..
			}
		)
	}
}

/// Prompts the operator has answered, and prompts they have put off.
#[derive(Debug, Default)]
pub struct Handled {
	answered: BTreeSet<String>,
	skipped: BTreeSet<String>,
}

impl Handled {
	pub fn answered(&mut self, id: &str) {
		self.answered.insert(id.to_owned());
	}

	pub fn skipped(&mut self, id: &str) {
		self.skipped.insert(id.to_owned());
	}

	/// Forget prompts that are no longer open, so an id a later run reuses is asked again.
	pub fn retain_open(&mut self, snapshot: &Snapshot) {
		let open = |id: &String| snapshot.prompts.iter().any(|prompt| prompt.id == *id);
		self.answered.retain(open);
		self.skipped.retain(open);
	}

	fn has(&self, id: &str) -> bool {
		self.answered.contains(id) || self.skipped.contains(id)
	}
}

/// What the console should ask next, given where the run stands.
///
/// Open prompts the console can answer come first, in the order the device lists them. Then, by the
/// state of the device, what the operator can do about it. A finished run is offered for
/// acknowledging only once every result has been shown.
pub fn next(snapshot: &Snapshot, handled: &Handled, results_shown: bool) -> Option<Question> {
	// spec: PCON
	if snapshot.run != RunState::Idle {
		let open = snapshot.prompts.iter().find_map(|prompt| {
			let asks = asks(&prompt.kind)?;
			(!handled.has(&prompt.id)).then(|| Question::Prompt {
				id: prompt.id.clone(),
				asks,
			})
		});
		if open.is_some() {
			return open;
		}
	}
	match snapshot.run {
		RunState::Idle => (!snapshot.steps.is_empty()).then_some(Question::Choose),
		RunState::Running => None,
		RunState::Waiting => Some(Question::Waiting),
		RunState::Failed => Some(Question::Failed),
		RunState::Finished => results_shown.then_some(Question::Finished),
	}
}

/// What the operator can ask for besides an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
	Retry,
	Stop,
	Acknowledge,
	/// Leave this prompt for now.
	Skip,
	/// Leave the console, and the run as it is.
	Quit,
}

/// What a typed line comes to.
#[derive(Debug, PartialEq, Eq)]
pub enum Typed {
	Answer(Answer),
	Verb(Verb),
	/// Not understood, and why.
	Bad(String),
}

/// Read a line typed at a prompt.
///
/// A line that starts with a colon is a command: `:stop`, `:retry`, `:skip` or `:quit`, so a run can
/// be ended or put off from a prompt that is not wanted. An answer that starts with a colon is typed
/// with a second one.
pub fn read_answer(asks: Asks, line: &str) -> Typed {
	if let Some(rest) = line.strip_prefix(':') {
		if rest.starts_with(':') && asks != Asks::Confirm {
			return Typed::Answer(Answer::text(rest));
		}
		return match rest {
			"stop" => Typed::Verb(Verb::Stop),
			"retry" => Typed::Verb(Verb::Retry),
			"skip" => Typed::Verb(Verb::Skip),
			"quit" => Typed::Verb(Verb::Quit),
			_ => Typed::Bad(
				"a line starting with a colon is :stop, :retry, :skip or :quit; \
				 start an answer with two colons to answer with one"
					.to_owned(),
			),
		};
	}
	match asks {
		Asks::Confirm => match line.trim().to_ascii_lowercase().as_str() {
			"y" | "yes" => Typed::Answer(Answer::boolean(true)),
			"n" | "no" => Typed::Answer(Answer::boolean(false)),
			_ => Typed::Bad("answer y or n".to_owned()),
		},
		Asks::Text | Asks::Secret if line.is_empty() => {
			Typed::Bad("type an answer, or :skip to leave it for now".to_owned())
		}
		Asks::Text | Asks::Secret => Typed::Answer(Answer::text(line)),
	}
}

/// Read a line typed at one of the menus.
pub fn read_menu(question: &Question, line: &str) -> Typed {
	let word = line.trim().trim_start_matches(':').to_ascii_lowercase();
	let verb = match (question, word.as_str()) {
		(_, "q" | "quit") => Verb::Quit,
		(Question::Waiting | Question::Failed, "s" | "stop") => Verb::Stop,
		(Question::Failed, "r" | "retry") => Verb::Retry,
		(Question::Finished, "" | "y" | "yes" | "a" | "acknowledge") => Verb::Acknowledge,
		(Question::Finished, "n" | "no") => Verb::Quit,
		_ => return Typed::Bad(menu_hint(question).to_owned()),
	};
	Typed::Verb(verb)
}

fn menu_hint(question: &Question) -> &'static str {
	match question {
		Question::Failed => "type retry, stop or q",
		Question::Finished => "type y to acknowledge the run, or n to leave it",
		_ => "type stop or q",
	}
}

#[cfg(test)]
mod tests {
	use bliti_core::provisioning::report::{DoneState, PromptReport, StepReport, StepState};

	use super::*;

	fn snapshot(run: RunState, prompts: &[(&str, PromptKind)]) -> Snapshot {
		Snapshot {
			run,
			steps: vec![StepReport {
				id: "a".to_owned(),
				title: "a".to_owned(),
				done: DoneState::NotDone,
				why: None,
				state: StepState::Running,
				progress: None,
				reason: None,
				result: None,
			}],
			prompts: prompts
				.iter()
				.map(|(id, kind)| PromptReport {
					id: (*id).to_owned(),
					step: "a".to_owned(),
					kind: kind.clone(),
					text: String::new(),
					blocking: true,
				})
				.collect(),
		}
	}

	fn prompt(id: &str, asks: Asks) -> Question {
		Question::Prompt {
			id: id.to_owned(),
			asks,
		}
	}

	#[test]
	fn what_to_ask_follows_the_state() {
		let handled = Handled::default();
		let ask = |run| next(&snapshot(run, &[]), &handled, true);
		assert_eq!(ask(RunState::Idle), Some(Question::Choose));
		assert_eq!(ask(RunState::Running), None);
		assert_eq!(ask(RunState::Waiting), Some(Question::Waiting));
		assert_eq!(ask(RunState::Failed), Some(Question::Failed));
		assert_eq!(ask(RunState::Finished), Some(Question::Finished));
	}

	#[test]
	fn a_device_with_no_steps_has_nothing_to_choose() {
		let mut idle = snapshot(RunState::Idle, &[]);
		idle.steps.clear();
		assert_eq!(next(&idle, &Handled::default(), true), None);
	}

	/// The run is not offered for acknowledging until every result has been shown (PCON).
	#[test]
	fn a_finished_run_is_not_offered_before_its_results_are_shown() {
		let finished = snapshot(RunState::Finished, &[]);
		assert_eq!(next(&finished, &Handled::default(), false), None);
	}

	#[test]
	fn prompts_come_first_in_order_and_only_those_the_console_answers() {
		let mut handled = Handled::default();
		let open = snapshot(
			RunState::Waiting,
			&[
				("n", PromptKind::Notice),
				("o", PromptKind::Other("dial".to_owned())),
				("c", PromptKind::Confirm),
				("t", PromptKind::Text),
				("s", PromptKind::Secret),
			],
		);
		assert_eq!(
			next(&open, &handled, true),
			Some(prompt("c", Asks::Confirm))
		);
		handled.answered("c");
		assert_eq!(next(&open, &handled, true), Some(prompt("t", Asks::Text)));
		handled.skipped("t");
		assert_eq!(next(&open, &handled, true), Some(prompt("s", Asks::Secret)));
		handled.answered("s");
		assert_eq!(next(&open, &handled, true), Some(Question::Waiting));
	}

	/// A prompt is asked while the run goes on, not only once it is waiting.
	#[test]
	fn a_prompt_is_asked_while_a_step_is_running() {
		let running = snapshot(RunState::Running, &[("t", PromptKind::Text)]);
		assert_eq!(
			next(&running, &Handled::default(), true),
			Some(prompt("t", Asks::Text))
		);
	}

	#[test]
	fn answered_prompts_are_forgotten_once_they_close() {
		let mut handled = Handled::default();
		handled.answered("t");
		handled.skipped("c");
		handled.retain_open(&snapshot(RunState::Running, &[("t", PromptKind::Text)]));
		assert!(handled.has("t"));
		assert!(!handled.has("c"));
	}

	#[test]
	fn a_confirm_takes_yes_or_no() {
		for (line, answer) in [("y", true), ("YES", true), (" n ", false), ("no", false)] {
			assert_eq!(
				read_answer(Asks::Confirm, line),
				Typed::Answer(Answer::boolean(answer)),
				"{line:?}"
			);
		}
		assert!(matches!(read_answer(Asks::Confirm, "maybe"), Typed::Bad(_)));
		assert!(matches!(read_answer(Asks::Confirm, ""), Typed::Bad(_)));
	}

	#[test]
	fn text_is_taken_as_typed() {
		assert_eq!(
			read_answer(Asks::Text, " Ada Lovelace "),
			Typed::Answer(Answer::text(" Ada Lovelace "))
		);
		assert_eq!(
			read_answer(Asks::Secret, "p@ss word"),
			Typed::Answer(Answer::text("p@ss word"))
		);
		assert!(matches!(read_answer(Asks::Text, ""), Typed::Bad(_)));
	}

	#[test]
	fn a_colon_makes_a_command_and_two_make_an_answer() {
		for (line, verb) in [
			(":stop", Verb::Stop),
			(":retry", Verb::Retry),
			(":skip", Verb::Skip),
			(":quit", Verb::Quit),
		] {
			assert_eq!(read_answer(Asks::Text, line), Typed::Verb(verb));
		}
		assert_eq!(
			read_answer(Asks::Secret, "::skip"),
			Typed::Answer(Answer::text(":skip"))
		);
		assert!(matches!(read_answer(Asks::Text, ":nope"), Typed::Bad(_)));
	}

	#[test]
	fn the_menus_take_their_own_words() {
		let verb = |question, line| read_menu(&question, line);
		assert_eq!(verb(Question::Failed, "retry"), Typed::Verb(Verb::Retry));
		assert_eq!(verb(Question::Failed, "S"), Typed::Verb(Verb::Stop));
		assert_eq!(verb(Question::Waiting, ":stop"), Typed::Verb(Verb::Stop));
		assert_eq!(verb(Question::Waiting, "q"), Typed::Verb(Verb::Quit));
		assert!(matches!(verb(Question::Waiting, "retry"), Typed::Bad(_)));
		assert_eq!(verb(Question::Finished, ""), Typed::Verb(Verb::Acknowledge));
		assert_eq!(
			verb(Question::Finished, "y"),
			Typed::Verb(Verb::Acknowledge)
		);
		assert_eq!(verb(Question::Finished, "n"), Typed::Verb(Verb::Quit));
		assert!(matches!(verb(Question::Failed, "go"), Typed::Bad(_)));
	}
}
