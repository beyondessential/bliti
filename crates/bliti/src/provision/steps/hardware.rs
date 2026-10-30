//! The step `hardware`: exercise the devices on the HAT, and place the board as a version (PHW).

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use serde_json::{Map, Value as Json, json};
use tokio::time::timeout;

use self::place::{Air, place};
use super::blocking;
use crate::{
	facts::product::Version,
	provision::{
		button::{Line, Seen, pressed_and_released},
		context::Context,
		step::{Done, Step},
	},
};

pub mod place;

/// How long an operator has to press the USER button, or the CASE lever and let it go (PHW, "Waiting
/// on the operator").
const WAIT: Duration = Duration::from_secs(5 * 60);

/// The chips and the TPM the step reads, each by blocking calls that say why they failed.
pub trait Checks: Air {
	/// One measurement of the air sensor.
	fn air(&self) -> Result<(), String>;

	/// One measurement of the light sensor.
	fn light(&self) -> Result<(), String>;

	/// The board ID from the TPM.
	fn tpm(&self) -> Result<(), String>;
}

/// The step.
pub struct Hardware<L> {
	checks: Arc<dyn Checks>,
	user: Seen,
	case: L,
}

impl<L> Hardware<L> {
	/// A step reading `case` as the line the case switch is on, and asking `user` whether the USER
	/// button has been pressed.
	pub fn new(checks: Arc<dyn Checks>, user: Seen, case: L) -> Self {
		Self { checks, user, case }
	}
}

/// How a check came out: why it failed, where it did.
type Outcome = Result<(), String>;

#[async_trait]
impl<L: Line + Clone + Sync> Step for Hardware<L> {
	fn id(&self) -> &str {
		"hardware"
	}

	fn title(&self) -> &str {
		"Check the hardware"
	}

	// spec: PHW
	async fn done(&self) -> Done {
		Done {
			state: DoneState::NotDone,
			why: None,
		}
	}

	// spec: PHW
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let user = self.user(context).await;
		let case = self.case(context).await;
		let air = self.sensor(|checks| checks.air()).await;
		let light = self.sensor(|checks| checks.light()).await;
		let tpm = self.sensor(|checks| checks.tpm()).await;
		let version = place(&self.checks).await;

		let checks = [
			("user", user),
			("case", case),
			("air", air),
			("light", light),
			("tpm", tpm),
		];
		context.result(result(&checks, version));
		outcome(&checks, version)
	}
}

impl<L: Line + Clone + Sync> Hardware<L> {
	// spec: PHW
	async fn user(&self, context: &mut Context) -> Outcome {
		if self.user.pressed() {
			return Ok(());
		}
		let notice = context.notice("user", "Press the USER button.");
		let mut seen = self.user.clone();
		let pressed = timeout(WAIT, seen.wait()).await;
		notice.close();
		pressed.map_err(|_| "the USER button was not pressed within five minutes".to_owned())
	}

	// spec: PHW
	async fn case(&self, context: &mut Context) -> Outcome {
		let notice = context.notice("case", "Press the CASE lever, then let it go.");
		let mut line = self.case.clone();
		let found = timeout(WAIT, pressed_and_released(&mut line)).await;
		notice.close();
		match found {
			Ok(Ok(())) => Ok(()),
			Ok(Err(err)) => Err(format!("the CASE switch cannot be read: {err}")),
			Err(_) => {
				Err("the CASE lever was not pressed and let go within five minutes".to_owned())
			}
		}
	}

	async fn sensor(&self, check: impl FnOnce(&dyn Checks) -> Outcome + Send + 'static) -> Outcome {
		let checks = self.checks.clone();
		blocking(move || check(checks.as_ref())).await
	}
}

/// What the step reports, whether it passes or fails (PHW, "The outcome").
// spec: PHW
fn result(checks: &[(&str, Outcome)], version: Option<Version>) -> Map<String, Json> {
	let checks = checks
		.iter()
		.map(|(id, outcome)| match outcome {
			Ok(()) => json!({ "id": id, "passed": true }),
			Err(reason) => json!({ "id": id, "passed": false, "reason": reason }),
		})
		.collect();
	let mut result = Map::new();
	result.insert("checks".to_owned(), Json::Array(checks));
	if let Some(version) = version {
		result.insert("version".to_owned(), Json::String(version.to_string()));
	}
	result
}

// spec: PHW
fn outcome(checks: &[(&str, Outcome)], version: Option<Version>) -> Result<(), String> {
	let mut problems: Vec<String> = checks
		.iter()
		.filter_map(|(id, outcome)| Some(format!("{id}: {}", outcome.as_ref().err()?)))
		.collect();
	if version.is_none() {
		problems.push("the board could not be placed and needs looking at".to_owned());
	}
	if problems.is_empty() {
		Ok(())
	} else {
		Err(problems.join("; "))
	}
}

#[cfg(test)]
mod tests;
