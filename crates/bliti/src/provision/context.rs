//! What a step is handed to report its work and to ask an operator for something (RUN, "Prompts").

use std::{future::pending, sync::Arc};

use bliti_core::provisioning::report::{Answer, PromptKind};
use serde_json::{Map, Value as Json};

use super::{model::Need, run::Inner};

/// What a step does with the operator and the run while it is carried out.
pub struct Context {
	inner: Arc<Inner>,
	step: usize,
	generation: u64,
}

impl Context {
	pub(super) fn new(inner: Arc<Inner>, step: usize, generation: u64) -> Self {
		Self {
			inner,
			step,
			generation,
		}
	}

	/// Report how far through its work the step is, from 0 to 1.
	pub fn progress(&mut self, fraction: f64) {
		let fraction = fraction.clamp(0.0, 1.0);
		self.inner.mutate(self.generation, |model| {
			model.steps[self.step].progress = Some(fraction);
		});
	}

	/// Report what the step found or produced, as the spec defining it gives.
	pub fn result(&mut self, result: Map<String, Json>) {
		self.inner.mutate(self.generation, |model| {
			model.steps[self.step].result = Some(result);
		});
	}

	/// Ask an operator to confirm, and wait for the answer unless one is already held.
	pub async fn confirm(&mut self, key: &str, text: &str) -> bool {
		self.need(key, PromptKind::Confirm, text)
			.await
			.as_bool()
			.unwrap_or(false)
	}

	/// Ask an operator for text, and wait for the answer unless one is already held.
	pub async fn text(&mut self, key: &str, text: &str) -> String {
		self.text_of(key, PromptKind::Text, text).await
	}

	/// Ask an operator for text that is not to be shown or kept, and wait for the answer unless one is
	/// already held.
	pub async fn secret(&mut self, key: &str, text: &str) -> String {
		self.text_of(key, PromptKind::Secret, text).await
	}

	/// Ask for something the step will need later and does not wait on now: the prompt stays open
	/// until it is answered or the run ends, and the step that needs the answer then waits only if it
	/// has not come.
	pub fn ask_later(&mut self, key: &str, kind: PromptKind, text: &str) {
		self.inner.mutate(self.generation, |model| {
			model.ask_later(self.step, key, kind, text);
		});
	}

	/// Ask an operator to do something, which holds the step until the returned [`Notice`] is closed
	/// or dropped. The step closes it when it sees the thing done.
	pub fn notice(&mut self, text: &str) -> Notice {
		let id = self
			.inner
			.mutate(self.generation, |model| model.notice(self.step, text));
		Notice {
			inner: self.inner.clone(),
			generation: self.generation,
			id,
		}
	}

	async fn text_of(&mut self, key: &str, kind: PromptKind, text: &str) -> String {
		self.need(key, kind, text)
			.await
			.as_text()
			.unwrap_or_default()
			.to_owned()
	}

	async fn need(&mut self, key: &str, kind: PromptKind, text: &str) -> Answer {
		let need = self.inner.mutate(self.generation, |model| {
			model.need(self.step, key, kind, text)
		});
		match need {
			Some(Need::Held(answer)) => answer,
			Some(Need::Wait(answer)) => match answer.await {
				Ok(answer) => answer,
				// The run ended with the question open, so the step is about to be dropped.
				Err(_) => pending().await,
			},
			// This step's run is over, so the step is about to be dropped.
			None => pending().await,
		}
	}
}

/// An open request that an operator do something, held until the step sees it done.
pub struct Notice {
	inner: Arc<Inner>,
	generation: u64,
	id: Option<String>,
}

impl Notice {
	/// The step has seen the thing done.
	pub fn close(self) {}
}

impl Drop for Notice {
	fn drop(&mut self) {
		if let Some(id) = self.id.take() {
			self.inner.mutate(self.generation, |model| model.close(&id));
		}
	}
}
