//! What a step is, to the engine that runs it (RUN, "Steps").
//!
//! The steps a device offers, and the effect of each, are specified by the specs that define them.

use std::{path::Path, sync::Arc};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;

use super::{button::Seen, context::Context, name::Known, steps};

/// What a step says of whether the board already holds what it would produce (RUN, "Done-state").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
	/// Whether it does.
	pub state: DoneState,
	/// Why, as text for an operator.
	pub why: Option<String>,
}

/// One unit of work of a run.
#[async_trait]
pub trait Step: Send + Sync {
	/// Names the step, in lower case letters, digits and hyphens.
	fn id(&self) -> &str;

	/// The step's name for an operator.
	fn title(&self) -> &str;

	/// Whether the board already holds what this step would produce, determined from the board as it
	/// stands and never from the record of an earlier run.
	async fn done(&self) -> Done;

	/// The id of the step after which this one asks for what it will need, ahead of its own turn.
	///
	/// A step that returns one is asked through [`Step::ask_ahead`] once that step, and every step
	/// chosen before it, has passed.
	fn asks_after(&self) -> Option<&str> {
		None
	}

	/// Ask for what the step will need later, without holding the run up. Called each time the run
	/// is about to carry out a step while this one is pending and its gate has passed, so it must
	/// be safe to call again: it is also what asks once more after a reboot.
	async fn ask_ahead(&self, _context: &mut Context) {}

	/// Carry the step out, asking an operator through `context` where it must. Safe to start again
	/// after it has been interrupted at any point. An error is why the step failed.
	async fn run(&self, context: &mut Context) -> Result<(), String>;
}

/// The steps a device offers, in the one fixed order a run carries them out in. `user` is what the
/// daemon has seen of the USER button, `name` the device's name once it has one, and `medium` where
/// the provisioning medium's data partition is mounted.
pub fn registry(user: Seen, name: Known, medium: &Path) -> Vec<Arc<dyn Step>> {
	steps::all(user, name, medium)
}
