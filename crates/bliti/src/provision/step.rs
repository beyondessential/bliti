//! What a step is, to the engine that runs it (RUN, "Steps").
//!
//! The steps a device offers, and the effect of each, are specified by the specs that define them.
//! The registry is empty until one does.

use std::sync::Arc;

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;

use super::context::Context;

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

	/// Carry the step out, asking an operator through `context` where it must. Safe to start again
	/// after it has been interrupted at any point. An error is why the step failed.
	async fn run(&self, context: &mut Context) -> Result<(), String>;
}

/// The steps a device offers, in the one fixed order a run carries them out in.
pub fn registry() -> Vec<Arc<dyn Step>> {
	Vec::new()
}
