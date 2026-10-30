//! The run: its states, the acts that move it, and the task that carries it out (RUN).
//!
//! One [`Provisioner`] holds the run for the whole device. A run belongs to the device, not to a
//! front end: it goes on whether or not anyone is attached, and survives a reboot by the state kept
//! on the provisioning medium.

use std::{
	panic::AssertUnwindSafe,
	sync::{Arc, Mutex, MutexGuard},
};

use bliti_core::{
	channel::messages::Message,
	provisioning::report::{Answer, PromptReport, RunState, StepReport, StepState},
};
use futures::FutureExt as _;
use jiff::civil::Date;
use tokio::{sync::watch, task::AbortHandle};

use super::{
	context::Context,
	model::{Model, Refusal},
	step::{Done, Step},
	store::{Store, Stored, StoredFailure},
};

/// Everything `provision-state` carries: where the run stands, every step, every open prompt (PRS).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
	/// The revision of the steps the device offers.
	pub revision: u32,
	/// The date the image on the provisioning medium was built.
	pub built: Date,
	/// The state of the device.
	pub run: RunState,
	/// Every step in order.
	pub steps: Vec<StepReport>,
	/// Every open prompt.
	pub prompts: Vec<PromptReport>,
}

impl Snapshot {
	/// This as the message that carries it.
	pub fn to_message(&self) -> Message {
		Message::ProvisionState {
			revision: self.revision,
			built: self.built,
			run: self.run,
			steps: self.steps.clone(),
			prompts: self.prompts.clone(),
		}
	}
}

/// An act a client or the console asks for, as the members of `provision-act` give it (PRS).
#[derive(Debug, Clone, PartialEq)]
pub struct Act {
	/// The act, by wire name.
	pub act: String,
	/// The steps a `start` chooses.
	pub steps: Option<Vec<String>>,
	/// The prompt an `answer` answers.
	pub prompt: Option<String>,
	/// What an `answer` answers with.
	pub value: Option<Answer>,
}

impl Act {
	/// The act a `provision-act` message asks for, or nothing for any other message.
	pub fn of(message: &Message) -> Option<Self> {
		let Message::ProvisionAct {
			act,
			steps,
			prompt,
			value,
		} = message
		else {
			return None;
		};
		Some(Self {
			act: act.clone(),
			steps: steps.clone(),
			prompt: prompt.clone(),
			value: value.clone(),
		})
	}
}

/// Why an act is refused: the member at fault, as an RFC 9535 Normalized Path rooted at the message
/// asking for it, and why (PRS, "The acts").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CannotAct {
	/// Which part of the request is at fault.
	pub at: String,
	/// Why, in the device's own words.
	pub reason: String,
}

impl From<Refusal> for CannotAct {
	fn from(refusal: Refusal) -> Self {
		Self {
			at: refusal.at,
			reason: refusal.reason,
		}
	}
}

/// The acts of RUN, "Acts".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
	Start,
	Answer,
	Retry,
	Stop,
	Acknowledge,
}

impl Kind {
	fn read(name: &str) -> Option<Self> {
		Some(match name {
			"start" => Self::Start,
			"answer" => Self::Answer,
			"retry" => Self::Retry,
			"stop" => Self::Stop,
			"acknowledge" => Self::Acknowledge,
			_ => return None,
		})
	}

	/// Whether the device accepts this act in `state`.
	// spec: RUN
	fn accepted_in(self, state: RunState) -> bool {
		use RunState::*;
		match self {
			Self::Start => matches!(state, Idle | Finished),
			Self::Answer => matches!(state, Running | Waiting | Failed),
			Self::Retry => state == Failed,
			Self::Stop => matches!(state, Waiting | Failed),
			Self::Acknowledge => state == Finished,
		}
	}
}

/// The one handle every front end shares.
#[derive(Clone)]
pub struct Provisioner {
	inner: Arc<Inner>,
}

pub(super) struct Inner {
	steps: Vec<Arc<dyn Step>>,
	model: Mutex<Model>,
	changes: watch::Sender<Snapshot>,
	store: Option<Store>,
	/// The task carrying the run out.
	task: Mutex<Option<AbortHandle>>,
	/// Held while the medium is written or cleared, so a write for a run that has since ended cannot
	/// land after the clearing.
	disk: Arc<Mutex<()>>,
	/// Held while done-states are being determined, so two determinations do not interleave.
	determining: tokio::sync::Mutex<()>,
}

impl Provisioner {
	/// A device holding `steps` in the one order a run carries them out in, whose medium's image was
	/// built on `built`, resuming the run the medium holds for this board, if any (RUN, "Surviving a
	/// reboot").
	///
	/// Without a `store` a run is carried out but not kept, and `start` is refused.
	pub async fn start(steps: Vec<Arc<dyn Step>>, store: Option<Store>, built: Date) -> Self {
		let model = Model::new(
			built,
			steps
				.iter()
				.map(|step| (step.id().to_owned(), step.title().to_owned())),
		);
		let (changes, _) = watch::channel(model.snapshot());
		let inner = Arc::new(Inner {
			steps,
			model: Mutex::new(model),
			changes,
			store,
			task: Mutex::new(None),
			disk: Arc::new(Mutex::new(())),
			determining: tokio::sync::Mutex::new(()),
		});
		inner.resume().await;
		Self { inner }
	}

	/// The state now, and every change to it.
	pub fn watch(&self) -> watch::Receiver<Snapshot> {
		self.inner.changes.subscribe()
	}

	/// Decide an act asked for, and carry it out if it is accepted (RUN, "Acts").
	pub fn act(&self, act: &Act) -> Result<(), CannotAct> {
		self.inner.act(act)
	}

	/// A front end has attached: determine every step's done-state again where the device is idle or
	/// finished (RUN, "Done-state").
	pub fn attach(&self) {
		self.inner.determine_soon();
	}
}

impl Inner {
	fn lock(&self) -> MutexGuard<'_, Model> {
		self.model
			.lock()
			.expect("the model is never held across a panic")
	}

	fn publish(&self, model: &Model) {
		let snapshot = model.snapshot();
		self.changes.send_if_modified(|held| {
			let changed = *held != snapshot;
			if changed {
				*held = snapshot;
			}
			changed
		});
	}

	/// Change the model for the run `generation`, publishing what it comes to. Nothing happens, and
	/// nothing is returned, where that run has ended.
	pub(super) fn mutate<R>(
		&self,
		generation: u64,
		change: impl FnOnce(&mut Model) -> R,
	) -> Option<R> {
		let mut model = self.lock();
		if model.generation != generation {
			return None;
		}
		let result = change(&mut model);
		self.publish(&model);
		Some(result)
	}

	fn act(self: &Arc<Self>, act: &Act) -> Result<(), CannotAct> {
		let refuse = |at: &str, reason: String| CannotAct {
			at: at.to_owned(),
			reason,
		};
		let Some(kind) = Kind::read(&act.act) else {
			return Err(refuse(
				"$['act']",
				format!("{:?} is not an act this device knows", act.act),
			));
		};

		let mut model = self.lock();
		let state = model.run();
		if !kind.accepted_in(state) {
			return Err(refuse(
				"$",
				format!("a {} is not accepted while the device is {state}", act.act),
			));
		}

		let generation = match kind {
			Kind::Start => {
				if self.store.is_none() {
					return Err(refuse(
						"$",
						"this board has no platform serial to keep a run under".to_owned(),
					));
				}
				let chosen = act.steps.as_deref().unwrap_or_default();
				if chosen.is_empty() {
					return Err(refuse(
						"$['steps']",
						"a run chooses at least one step".to_owned(),
					));
				}
				let mut indices = Vec::new();
				for (position, id) in chosen.iter().enumerate() {
					let Some(index) = model.index_of(id) else {
						return Err(refuse(
							&format!("$['steps'][{position}]"),
							format!("this device has no step {id:?}"),
						));
					};
					indices.push(index);
				}
				indices.sort_unstable();
				indices.dedup();
				model.begin_run(&indices);
				model.generation
			}
			Kind::Answer => {
				model.answer(act.prompt.as_deref(), act.value.as_ref())?;
				self.publish(&model);
				return Ok(());
			}
			Kind::Retry => {
				model.retry();
				model.generation
			}
			Kind::Stop | Kind::Acknowledge => {
				model.clear_run();
				model.generation += 1;
				let generation = model.generation;
				self.publish(&model);
				drop(model);
				if let Some(task) = self
					.task
					.lock()
					.expect("the task is never held across a panic")
					.take()
				{
					task.abort();
				}
				self.forget_run(generation);
				self.determine_soon();
				return Ok(());
			}
		};
		self.publish(&model);
		drop(model);
		self.drive_soon(generation);
		Ok(())
	}

	/// Discard the run held on the medium, for the run `generation` having ended.
	fn forget_run(&self, generation: u64) {
		let Some(store) = &self.store else { return };
		let _disk = self
			.disk
			.lock()
			.expect("the disk is never held across a panic");
		if self.lock().generation != generation {
			return;
		}
		if let Err(err) = store.clear() {
			tracing::warn!(%err, "could not discard the run kept on the medium");
		}
	}

	/// Keep the run as it stands on the medium, for the run `generation`, unless it has since ended.
	async fn record(self: &Arc<Self>, generation: u64) -> std::io::Result<()> {
		let Some(store) = self.store.clone() else {
			return Ok(());
		};
		let inner = self.clone();
		tokio::task::spawn_blocking(move || {
			let _disk = inner
				.disk
				.lock()
				.expect("the disk is never held across a panic");
			let stored = {
				let model = inner.lock();
				if model.generation != generation {
					return Ok(());
				}
				Stored {
					chosen: model.chosen(),
					passed: model.ids(StepState::Passed),
					failed: model
						.steps
						.iter()
						.find(|entry| entry.state == StepState::Failed)
						.map(|entry| StoredFailure {
							step: entry.id.clone(),
							reason: entry.reason.clone().unwrap_or_default(),
						}),
				}
			};
			store.save(&stored)
		})
		.await
		.map_err(std::io::Error::other)?
	}

	/// Take up the run the medium holds for this board, or settle into idle (RUN, "Surviving a
	/// reboot").
	async fn resume(self: &Arc<Self>) {
		let stored = match self.store.clone() {
			Some(store) => match tokio::task::spawn_blocking(move || store.load()).await {
				Ok(Ok(stored)) => stored,
				Ok(Err(err)) => {
					tracing::warn!(%err, "could not read the run kept on the medium");
					None
				}
				Err(err) => {
					tracing::warn!(%err, "could not read the run kept on the medium");
					None
				}
			},
			None => None,
		};
		let Some(stored) = stored else {
			self.determine_soon();
			return;
		};

		let adopted = {
			let mut model = self.lock();
			let adopted = adopt(&mut model, &stored);
			self.publish(&model);
			adopted.then(|| (model.run(), model.generation))
		};
		match adopted {
			Some((RunState::Failed, _)) => {
				tracing::info!("resuming a failed run; it waits for a retry");
			}
			Some((_, generation)) => {
				tracing::info!("resuming a run that was under way");
				self.drive_soon(generation);
			}
			None => {
				tracing::warn!(
					"the run kept on the medium does not match this device's steps; discarding it"
				);
				if let Some(store) = &self.store {
					let _ = store.clear();
				}
				self.determine_soon();
			}
		}
	}

	fn drive_soon(self: &Arc<Self>, generation: u64) {
		let inner = self.clone();
		let task = tokio::spawn(async move { inner.drive(generation).await });
		*self
			.task
			.lock()
			.expect("the task is never held across a panic") = Some(task.abort_handle());
	}

	/// Carry the steps of the run out one after another until one fails or every one has passed.
	async fn drive(self: Arc<Self>, generation: u64) {
		loop {
			self.ask_ahead(generation).await;
			let Some(next) = self.mutate(generation, Model::begin_next) else {
				return;
			};
			let Some(index) = next else {
				self.finish(generation);
				return;
			};

			if let Err(err) = self.record(generation).await {
				let reason = format!("the run could not be kept on the medium: {err}");
				self.fail(generation, index, reason).await;
				return;
			}

			let step = self.steps[index].clone();
			let mut context = Context::new(self.clone(), index, generation);
			let outcome = AssertUnwindSafe(step.run(&mut context))
				.catch_unwind()
				.await;
			drop(context);
			self.mutate(generation, |model| model.release(index));

			match outcome {
				Ok(Ok(())) => {
					if self.mutate(generation, |model| model.pass(index)).is_none() {
						return;
					}
					if let Err(err) = self.record(generation).await {
						tracing::warn!(%err, "a step passed and could not be recorded; it is carried out again after a reboot");
					}
				}
				Ok(Err(reason)) => {
					self.fail(generation, index, reason).await;
					return;
				}
				Err(_) => {
					self.fail(generation, index, "the step panicked".to_owned())
						.await;
					return;
				}
			}
		}
	}

	/// Let each pending step whose gate has passed ask for what it will need later (RUN, "Prompts").
	async fn ask_ahead(self: &Arc<Self>, generation: u64) {
		let due: Vec<usize> = {
			let model = self.lock();
			if model.generation != generation {
				return;
			}
			(0..self.steps.len())
				.filter(|index| model.steps[*index].state == StepState::Pending)
				.filter(|index| {
					self.steps[*index]
						.asks_after()
						.is_some_and(|after| model.passed_through(after))
				})
				.collect()
		};
		for index in due {
			let mut context = Context::new(self.clone(), index, generation);
			self.steps[index].ask_ahead(&mut context).await;
		}
	}

	async fn fail(self: &Arc<Self>, generation: u64, index: usize, reason: String) {
		tracing::warn!(step = %self.steps[index].id(), %reason, "a step failed");
		if self
			.mutate(generation, |model| model.fail(index, reason))
			.is_none()
		{
			return;
		}
		if let Err(err) = self.record(generation).await {
			tracing::warn!(%err, "could not keep the failed run on the medium");
		}
	}

	fn finish(self: &Arc<Self>, generation: u64) {
		if self.mutate(generation, Model::finish).is_none() {
			return;
		}
		self.forget_run(generation);
		self.determine_soon();
	}

	fn determine_soon(self: &Arc<Self>) {
		let inner = self.clone();
		tokio::spawn(async move { inner.determine().await });
	}

	/// Determine every step's done-state, where the device is idle or finished (RUN, "Done-state").
	async fn determine(&self) {
		let _turn = self.determining.lock().await;
		if !self.lock().at_rest() {
			return;
		}
		let mut found = Vec::with_capacity(self.steps.len());
		for step in &self.steps {
			found.push(
				AssertUnwindSafe(step.done())
					.catch_unwind()
					.await
					.unwrap_or_else(|_| Done {
						state: bliti_core::provisioning::report::DoneState::Unknown,
						why: Some("the check failed".to_owned()),
					}),
			);
		}
		let mut model = self.lock();
		if !model.at_rest() {
			return;
		}
		for (entry, done) in model.steps.iter_mut().zip(found) {
			entry.done = done;
		}
		self.publish(&model);
	}
}

// spec: RUN
/// Hold the run `stored` describes, where it matches the steps this device has.
fn adopt(model: &mut Model, stored: &Stored) -> bool {
	let known = |id: &String| model.index_of(id);
	let chosen: Option<Vec<usize>> = stored.chosen.iter().map(known).collect();
	let passed: Option<Vec<usize>> = stored.passed.iter().map(known).collect();
	let failed = stored.failed.as_ref().map(|failed| known(&failed.step));
	let (Some(chosen), Some(passed)) = (chosen, passed) else {
		return false;
	};
	if chosen.is_empty()
		|| passed.iter().any(|index| !chosen.contains(index))
		|| matches!(failed, Some(None))
	{
		return false;
	}
	model.begin_run(&chosen);
	for index in passed {
		model.pass(index);
	}
	if let (Some(Some(index)), Some(failed)) = (failed, &stored.failed) {
		if !chosen.contains(&index) {
			model.clear_run();
			return false;
		}
		model.fail(index, failed.reason.clone());
	}
	true
}

#[cfg(test)]
mod tests;
