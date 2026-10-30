//! The state of a run, held in one place and changed only by the engine (RUN).
//!
//! Plain data and no waiting, so every rule about which act is accepted where, what a prompt blocks,
//! and what is reported can be read here and tested without a task.

use std::collections::HashMap;

use bliti_core::provisioning::{
	self,
	report::{Answer, DoneState, PromptKind, PromptReport, RunState, StepReport, StepState},
};
use jiff::civil::Date;
use serde_json::{Map, Value as Json};
use tokio::sync::oneshot;

use super::{run::Snapshot, step::Done};

/// One step, as the run reports it.
pub struct Entry {
	pub id: String,
	pub title: String,
	pub done: Done,
	pub state: StepState,
	pub progress: Option<f64>,
	pub reason: Option<String>,
	pub result: Option<Map<String, Json>>,
}

/// Whether a step accepts the text an operator answered a prompt with, and if not, why.
pub type Check = fn(&str) -> Result<(), String>;

/// A prompt a step asks for ahead of its turn, which does not hold the run up (RUN, "Prompts").
#[derive(Clone)]
pub struct Question {
	key: String,
	kind: PromptKind,
	text: String,
	default: Option<String>,
	check: Option<Check>,
}

impl Question {
	pub fn new(key: &str, kind: PromptKind, text: &str) -> Self {
		Self {
			key: key.to_owned(),
			kind,
			text: text.to_owned(),
			default: None,
			check: None,
		}
	}

	/// The answer offered, for a `text` prompt.
	pub fn default(mut self, default: impl Into<String>) -> Self {
		self.default = Some(default.into());
		self
	}

	/// What decides whether an answer is accepted.
	pub fn check(mut self, check: Check) -> Self {
		self.check = Some(check);
		self
	}
}

/// A prompt that is open.
pub struct Open {
	pub id: String,
	/// The step that asked.
	pub step: usize,
	/// What the answer is held under, across the whole run.
	pub key: String,
	pub kind: PromptKind,
	pub text: String,
	/// The answer offered, where there is one.
	pub default: Option<String>,
	/// Decides whether an answer of text is accepted.
	pub check: Option<Check>,
	/// The step waiting for the answer, where one is, which need not be the step that asked.
	pub blocks: Option<usize>,
	/// How the waiting step is given the answer.
	pub waiter: Option<oneshot::Sender<Answer>>,
}

impl Open {
	pub fn blocking(&self) -> bool {
		self.blocks.is_some()
	}
}

/// What a step that needs an answer is given.
pub enum Need {
	/// The device already holds it.
	Held(Answer),
	/// It is asked for, and arrives here.
	Wait(oneshot::Receiver<Answer>),
}

pub struct Model {
	/// The date the image on the provisioning medium was built.
	built: Date,
	pub steps: Vec<Entry>,
	/// The state of the device. Never `waiting`: that is read from the prompts, by [`Model::run`].
	state: RunState,
	pub prompts: Vec<Open>,
	/// What was answered, by the key it was asked under. In memory only.
	answers: HashMap<String, Answer>,
	/// Counts the runs, so what a stopped run's task does is not taken for the next run's.
	pub generation: u64,
}

impl Model {
	pub fn new(built: Date, steps: impl IntoIterator<Item = (String, String)>) -> Self {
		Self {
			built,
			steps: steps
				.into_iter()
				.map(|(id, title)| Entry {
					id,
					title,
					done: Done {
						state: DoneState::Unknown,
						why: None,
					},
					state: StepState::Unchosen,
					progress: None,
					reason: None,
					result: None,
				})
				.collect(),
			state: RunState::Idle,
			prompts: Vec::new(),
			answers: HashMap::new(),
			generation: 0,
		}
	}

	/// The state of the device (RUN, "Run states"): `waiting` while the step being carried out has a
	/// blocking prompt open.
	pub fn run(&self) -> RunState {
		if self.state == RunState::Running && self.prompts.iter().any(Open::blocking) {
			RunState::Waiting
		} else {
			self.state
		}
	}

	pub fn at_rest(&self) -> bool {
		matches!(self.state, RunState::Idle | RunState::Finished)
	}

	/// The state of one step, `waiting` where it has a blocking prompt open.
	fn step_state(&self, index: usize) -> StepState {
		let entry = &self.steps[index];
		if entry.state == StepState::Running
			&& self
				.prompts
				.iter()
				.any(|prompt| prompt.blocks == Some(index))
		{
			StepState::Waiting
		} else {
			entry.state
		}
	}

	pub fn snapshot(&self) -> Snapshot {
		Snapshot {
			// spec: PRS
			revision: provisioning::REVISION,
			built: self.built,
			run: self.run(),
			steps: self
				.steps
				.iter()
				.enumerate()
				.map(|(index, entry)| StepReport {
					id: entry.id.clone(),
					title: entry.title.clone(),
					done: entry.done.state,
					why: entry.done.why.clone(),
					state: self.step_state(index),
					progress: entry.progress,
					reason: entry.reason.clone(),
					result: entry.result.clone(),
				})
				.collect(),
			prompts: self
				.prompts
				.iter()
				.map(|prompt| PromptReport {
					id: prompt.id.clone(),
					step: self.steps[prompt.step].id.clone(),
					kind: prompt.kind.clone(),
					text: prompt.text.clone(),
					blocking: prompt.blocking(),
					default: prompt.default.clone(),
				})
				.collect(),
		}
	}

	/// The step carried out after the ones that have passed, which is the first not yet reached.
	pub fn next(&self) -> Option<usize> {
		self.steps
			.iter()
			.position(|entry| entry.state == StepState::Pending)
	}

	/// Whether every step chosen up to and including the one named has passed.
	pub fn passed_through(&self, id: &str) -> bool {
		let Some(last) = self.index_of(id) else {
			return false;
		};
		self.steps[..=last]
			.iter()
			.all(|entry| matches!(entry.state, StepState::Unchosen | StepState::Passed))
	}

	/// The ids of the steps in `state`, in order.
	pub fn ids(&self, state: StepState) -> Vec<String> {
		self.steps
			.iter()
			.filter(|entry| entry.state == state)
			.map(|entry| entry.id.clone())
			.collect()
	}

	/// The ids of every step that is part of the run, in order.
	pub fn chosen(&self) -> Vec<String> {
		self.steps
			.iter()
			.filter(|entry| entry.state != StepState::Unchosen)
			.map(|entry| entry.id.clone())
			.collect()
	}

	pub fn index_of(&self, id: &str) -> Option<usize> {
		self.steps.iter().position(|entry| entry.id == id)
	}

	/// Hold no run: every step unchosen, every prompt closed, every answer forgotten.
	pub fn clear_run(&mut self) {
		self.state = RunState::Idle;
		self.prompts.clear();
		self.answers.clear();
		for entry in &mut self.steps {
			entry.state = StepState::Unchosen;
			entry.progress = None;
			entry.reason = None;
			entry.result = None;
		}
	}

	/// Begin a run of the steps at `chosen`, replacing whatever run is held.
	pub fn begin_run(&mut self, chosen: &[usize]) {
		self.clear_run();
		for index in chosen {
			self.steps[*index].state = StepState::Pending;
		}
		self.state = RunState::Running;
		self.generation += 1;
	}

	/// Mark the next step as the one being carried out, returning it.
	pub fn begin_next(&mut self) -> Option<usize> {
		let index = self.next()?;
		let entry = &mut self.steps[index];
		entry.state = StepState::Running;
		entry.progress = None;
		entry.reason = None;
		entry.result = None;
		Some(index)
	}

	pub fn pass(&mut self, index: usize) {
		let entry = &mut self.steps[index];
		entry.state = StepState::Passed;
		entry.progress = None;
	}

	/// Stop the run at `index`, which failed for `reason`.
	pub fn fail(&mut self, index: usize, reason: String) {
		let entry = &mut self.steps[index];
		entry.state = StepState::Failed;
		entry.reason = Some(reason);
		self.state = RunState::Failed;
	}

	/// Carry the failed step out again.
	pub fn retry(&mut self) -> Option<usize> {
		let index = self
			.steps
			.iter()
			.position(|entry| entry.state == StepState::Failed)?;
		let entry = &mut self.steps[index];
		entry.state = StepState::Pending;
		entry.reason = None;
		self.state = RunState::Running;
		self.generation += 1;
		Some(index)
	}

	/// Every step has passed.
	pub fn finish(&mut self) {
		self.state = RunState::Finished;
		self.prompts.clear();
		self.answers.clear();
	}

	/// `step` has ended: the prompts it was waiting on stop holding anything up, and the ones it asked
	/// only to wait on are closed. One it asked for a later step stays open.
	pub fn release(&mut self, step: usize) {
		self.prompts
			.retain(|prompt| !(prompt.blocks == Some(step) && prompt.step == step));
		for prompt in &mut self.prompts {
			if prompt.blocks == Some(step) {
				prompt.blocks = None;
				prompt.waiter = None;
			}
		}
	}

	pub fn close(&mut self, id: &str) {
		self.prompts.retain(|prompt| prompt.id != id);
	}

	/// A step needs the answer it asks for under `key`: it is given it where held, and otherwise a
	/// blocking prompt is open for it, turning one the step opened earlier into one it waits on.
	pub fn need(&mut self, step: usize, key: &str, kind: PromptKind, text: &str) -> Need {
		if let Some(answer) = self.answers.get(key) {
			return Need::Held(answer.clone());
		}
		let (waiter, answer) = oneshot::channel();
		if let Some(open) = self.prompts.iter_mut().find(|prompt| prompt.key == key) {
			open.blocks = Some(step);
			open.waiter = Some(waiter);
		} else {
			let question = Question::new(key, kind, text);
			self.prompts
				.push(self.open(step, &question, Some(step), Some(waiter)));
		}
		Need::Wait(answer)
	}

	/// A step asks for something it does not wait on.
	pub fn ask_later(&mut self, step: usize, question: Question) {
		let key = &question.key;
		let known =
			self.answers.contains_key(key) || self.prompts.iter().any(|prompt| prompt.key == *key);
		if !known {
			self.prompts.push(self.open(step, &question, None, None));
		}
	}

	/// What was answered under `key`, if anything.
	pub fn held(&self, key: &str) -> Option<Answer> {
		self.answers.get(key).cloned()
	}

	/// Close the prompt open under `key`, if any, without an answer.
	pub fn close_key(&mut self, key: &str) {
		self.prompts.retain(|prompt| prompt.key != key);
	}

	/// Forget what was answered under `key`.
	pub fn forget(&mut self, key: &str) {
		self.answers.remove(key);
	}

	/// A step asks an operator to do something, and waits until it sees it done. Returns the prompt.
	pub fn notice(&mut self, step: usize, key: &str, text: &str) -> String {
		let question = Question::new(key, PromptKind::Notice, text);
		let prompt = self.open(step, &question, Some(step), None);
		let id = prompt.id.clone();
		self.prompts.push(prompt);
		id
	}

	fn open(
		&self,
		step: usize,
		question: &Question,
		blocks: Option<usize>,
		waiter: Option<oneshot::Sender<Answer>>,
	) -> Open {
		Open {
			id: question.key.clone(),
			step,
			key: question.key.clone(),
			kind: question.kind.clone(),
			text: question.text.clone(),
			default: question.default.clone(),
			check: question.check,
			blocks,
			waiter,
		}
	}

	/// Answer the open prompt `id` with `value`, or say at which member of the act it cannot be.
	pub fn answer(&mut self, id: Option<&str>, value: Option<&Answer>) -> Result<(), Refusal> {
		let position = id
			.and_then(|id| self.prompts.iter().position(|prompt| prompt.id == id))
			.filter(|position| self.prompts[*position].kind != PromptKind::Notice)
			.ok_or_else(|| Refusal {
				at: "$['prompt']".to_owned(),
				reason: "no prompt that takes an answer is open under that id".to_owned(),
			})?;
		let kind = &self.prompts[position].kind;
		let fits = match (kind, value) {
			(PromptKind::Confirm, Some(value)) => value.as_bool().is_some(),
			(PromptKind::Text | PromptKind::Secret, Some(value)) => value.as_text().is_some(),
			_ => false,
		};
		let Some(value) = value.filter(|_| fits) else {
			return Err(Refusal {
				at: "$['value']".to_owned(),
				reason: match kind {
					PromptKind::Confirm => "this prompt takes true or false".to_owned(),
					_ => "this prompt takes text".to_owned(),
				},
			});
		};
		if let (Some(check), Some(text)) = (self.prompts[position].check, value.as_text())
			&& let Err(reason) = check(text)
		{
			return Err(Refusal {
				at: "$['value']".to_owned(),
				reason,
			});
		}
		let open = self.prompts.remove(position);
		self.answers.insert(open.key, value.clone());
		if let Some(waiter) = open.waiter {
			let _ = waiter.send(value.clone());
		}
		Ok(())
	}
}

/// Which member of an act is at fault, and why.
#[derive(Debug)]
pub struct Refusal {
	pub at: String,
	pub reason: String,
}

#[cfg(test)]
mod tests;
