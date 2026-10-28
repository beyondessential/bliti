//! The run: its states, the acts that move it, and the task that carries it out (RUN).

use std::sync::Arc;

use bliti_core::{
	channel::messages::Message,
	provisioning::report::{Answer, PromptReport, RunState, StepReport},
};
use tokio::sync::watch;

use super::{step::Step, store::Store};

/// Everything `provision-state` carries: where the run stands, every step, every open prompt (PRS).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
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

/// Why an act is refused: the member at fault, as an RFC 9535 Normalized Path rooted at the message
/// asking for it, and why (PRS, "The acts").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CannotAct {
	/// Which part of the request is at fault.
	pub at: String,
	/// Why, in the device's own words.
	pub reason: String,
}

/// What a step is handed to report its work and to ask an operator for something.
pub struct Context {}

/// The one handle every front end shares.
#[derive(Clone)]
pub struct Provisioner {
	_inner: Arc<()>,
}

impl Provisioner {
	/// The state now, and every change to it.
	pub fn watch(&self) -> watch::Receiver<Snapshot> {
		todo!()
	}

	/// Decide an act asked for, carrying it out if it is accepted.
	pub fn act(&self, _act: &Act) -> Result<(), CannotAct> {
		todo!()
	}

	/// Determine every step's done-state again, where the device is idle or finished, as a front end
	/// attaching asks (RUN, "Done-state").
	pub fn attach(&self) {
		todo!()
	}

	/// A device holding `steps`, resuming the run the medium holds for it, if any.
	pub async fn start(_steps: Vec<Arc<dyn Step>>, _store: Option<Store>) -> Self {
		todo!()
	}
}
