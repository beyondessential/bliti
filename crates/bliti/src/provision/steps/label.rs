//! The step `label`: hand a client what it needs to print the board's label (PLBL).

use std::sync::Arc;

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use serde_json::{Map, Value as Json, json};

use super::{
	eeprom::{Memory, held_version},
	hardware::place::{Air, place},
};
use crate::{
	facts::product::Version,
	provision::{
		context::Context,
		name::Known,
		step::{Done, Step},
	},
};

/// The step.
pub struct Label {
	known: Known,
	air: Arc<dyn Air>,
	memory: Arc<dyn Memory>,
}

impl Label {
	/// A step taking the payload from what the device established, the version from the EEPROM
	/// through `memory`, and otherwise placing the board through `air`.
	pub fn new(known: Known, air: Arc<dyn Air>, memory: Arc<dyn Memory>) -> Self {
		Self { known, air, memory }
	}

	/// The board's version: the EEPROM's where it holds a Tamanu Iti, otherwise the board placed.
	// spec: PLBL
	async fn version(&self) -> Option<Version> {
		match held_version(&self.memory).await {
			Ok(Some(version)) => Some(version),
			_ => place(&self.air).await,
		}
	}
}

#[async_trait]
impl Step for Label {
	fn id(&self) -> &str {
		"label"
	}

	fn title(&self) -> &str {
		"Make the label"
	}

	// spec: PLBL
	async fn done(&self) -> Done {
		Done {
			state: DoneState::NotDone,
			why: None,
		}
	}

	// spec: PLBL
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let mut known = self.known.clone();
		let payload = known
			.wait_for(Option::is_some)
			.await
			.map_err(|_| "the device never established this board's ID".to_owned())?
			.as_ref()
			.and_then(|established| established.payload.clone())
			.ok_or("the device could not establish this board's ID, so it has no label to make")?;
		let version = self.version().await.ok_or(
			"the board could not be placed as a version of the Tamanu Iti and needs looking at",
		)?;
		let at = jiff::Timestamp::now()
			.strftime("%Y-%m-%dT%H:%M:%SZ")
			.to_string();

		let mut result = Map::new();
		result.insert("payload".to_owned(), json!(payload.to_text()));
		result.insert("version".to_owned(), Json::String(version.to_string()));
		result.insert("at".to_owned(), json!(at));
		context.result(result);
		Ok(())
	}
}

#[cfg(test)]
mod tests;
