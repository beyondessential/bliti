use std::sync::Mutex;

use bliti_core::provisioning::report::{DoneState, StepState};

use super::*;
use crate::{
	facts::i2c,
	provision::{
		button::{self, Button as _},
		steps::testing::{Rig, Switch, open},
	},
};

/// Chips and a TPM that answer as scripted, and note what was read.
struct Fitted {
	air: Outcome,
	light: Outcome,
	tpm: Outcome,
	chip: u8,
	read: Mutex<Vec<&'static str>>,
}

impl Fitted {
	fn good() -> Self {
		Self {
			air: Ok(()),
			light: Ok(()),
			tpm: Ok(()),
			chip: 0x61,
			read: Mutex::default(),
		}
	}

	fn note(&self, what: &'static str) {
		self.read.lock().unwrap().push(what);
	}
}

impl Air for Fitted {
	fn chip_id(&self) -> Result<u8, i2c::Error> {
		Ok(self.chip)
	}
}

impl Checks for Fitted {
	fn air(&self) -> Result<(), String> {
		self.note("air");
		self.air.clone()
	}

	fn light(&self) -> Result<(), String> {
		self.note("light");
		self.light.clone()
	}

	fn tpm(&self) -> Result<(), String> {
		self.note("tpm");
		self.tpm.clone()
	}
}

/// A step, and what an operator works.
struct Bench {
	rig: Rig,
	fitted: Arc<Fitted>,
	case: Switch,
	user: button::Presser,
	/// The USER button, which only records a press once something waits on it, as the daemon does.
	button: tokio::task::JoinHandle<()>,
}

impl Bench {
	async fn new(fitted: Fitted, user_pressed: bool) -> Self {
		let fitted = Arc::new(fitted);
		let (fake, presser) = button::Fake::new();
		let (mut recorded, seen) = button::record(fake);
		let button = tokio::spawn(async move {
			loop {
				recorded.pressed().await;
			}
		});
		let case = Switch::default();
		let step = Hardware::new(fitted.clone(), seen, case.clone());
		if user_pressed {
			presser.press();
			tokio::time::sleep(Duration::from_millis(1)).await;
		}
		Self {
			rig: Rig::new(Arc::new(step)).await,
			fitted,
			case,
			user: presser,
			button,
		}
	}
}

impl Drop for Bench {
	fn drop(&mut self) {
		self.button.abort();
	}
}

fn checks(result: &Map<String, Json>) -> Vec<(String, bool, Option<String>)> {
	result["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|check| {
			(
				check["id"].as_str().unwrap().to_owned(),
				check["passed"].as_bool().unwrap(),
				check.get("reason").map(|r| r.as_str().unwrap().to_owned()),
			)
		})
		.collect()
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn a_good_board_passes_every_check_and_is_placed() {
	let bench = Bench::new(Fitted::good(), true).await;
	bench.rig.start();
	bench.rig.prompted("case").await;
	bench.case.work().await;
	let step = bench.rig.ended().await;

	assert_eq!(step.state, StepState::Passed);
	let result = step.result.expect("a result");
	assert_eq!(
		checks(&result),
		["user", "case", "air", "light", "tpm"].map(|id| (id.to_owned(), true, None))
	);
	assert_eq!(result["version"], "4.0");
}

/// A press that found the board counts, so the operator is not asked for it again.
// spec: PHW
#[tokio::test(start_paused = true)]
async fn a_press_seen_since_the_start_is_not_asked_for_again() {
	let bench = Bench::new(Fitted::good(), true).await;
	bench.rig.start();
	let snapshot = bench.rig.prompted("case").await;
	assert_eq!(open(&snapshot), ["case"]);
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn without_a_press_the_operator_is_asked_for_one() {
	let bench = Bench::new(Fitted::good(), false).await;
	bench.rig.start();
	let snapshot = bench.rig.prompted("user").await;
	let prompt = &snapshot.prompts[0];
	assert_eq!(
		prompt.kind,
		bliti_core::provisioning::report::PromptKind::Notice
	);
	assert!(prompt.blocking);
	assert!(prompt.text.contains("USER"), "{}", prompt.text);

	bench.user.press();
	let snapshot = bench.rig.prompted("case").await;
	assert_eq!(
		open(&snapshot),
		["case"],
		"the USER prompt closes on a press"
	);
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn the_case_prompt_names_the_lever_by_its_label() {
	let bench = Bench::new(Fitted::good(), true).await;
	bench.rig.start();
	let snapshot = bench.rig.prompted("case").await;
	assert!(snapshot.prompts[0].text.contains("CASE"));
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn the_case_lever_not_worked_in_five_minutes_fails_and_closes_the_prompt() {
	let bench = Bench::new(Fitted::good(), true).await;
	bench.rig.start();
	bench.rig.prompted("case").await;
	let step = bench.rig.ended().await;

	assert_eq!(step.state, StepState::Failed);
	let reason = step.reason.unwrap();
	assert!(reason.contains("case: "), "{reason}");
	assert!(reason.contains("five minutes"), "{reason}");
	let result = step.result.unwrap();
	let failed: Vec<_> = checks(&result)
		.into_iter()
		.filter(|(_, passed, _)| !passed)
		.map(|(id, ..)| id)
		.collect();
	assert_eq!(failed, ["case"]);
	assert_eq!(
		result["version"], "4.0",
		"the board is placed whatever failed"
	);
	let snapshot = bench.rig.provisioner.watch().borrow().clone();
	assert!(snapshot.prompts.is_empty());
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn the_user_button_not_pressed_in_five_minutes_fails_that_check_alone() {
	let bench = Bench::new(Fitted::good(), false).await;
	bench.rig.start();
	bench.rig.prompted("user").await;
	// The prompt for the next check opens only once this one has failed.
	bench.rig.prompted("case").await;
	bench.case.work().await;
	let step = bench.rig.ended().await;

	assert_eq!(step.state, StepState::Failed);
	let reason = step.reason.unwrap();
	assert!(reason.contains("user: "), "{reason}");
	assert!(!reason.contains("case: "), "{reason}");
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn a_case_switch_that_cannot_be_read_fails_with_a_reason() {
	let bench = Bench::new(Fitted::good(), true).await;
	bench.case.break_wire();
	bench.rig.start();
	let step = bench.rig.ended().await;

	assert_eq!(step.state, StepState::Failed);
	assert!(
		step.reason
			.unwrap()
			.contains("CASE switch cannot be read: no such line")
	);
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn another_part_at_a_sensor_s_address_fails_the_check_naming_the_id() {
	let mut fitted = Fitted::good();
	fitted.air = Err("the BME680's address answers as another part, ID 0x58".to_owned());
	fitted.chip = 0x58;
	let bench = Bench::new(fitted, true).await;
	bench.rig.start();
	bench.rig.prompted("case").await;
	bench.case.work().await;
	let step = bench.rig.ended().await;

	assert_eq!(step.state, StepState::Failed);
	let reason = step.reason.unwrap();
	assert!(
		reason.contains("air: the BME680's address answers as another part, ID 0x58"),
		"{reason}"
	);
	assert!(
		reason.contains("could not be placed and needs looking at"),
		"{reason}"
	);
	let result = step.result.unwrap();
	assert!(!result.contains_key("version"));
	assert!(!checks(&result)[2].1);
	assert!(checks(&result)[2].2.as_deref().unwrap().contains("0x58"));
}

/// A board a sensor fails on is checked as far as it goes, so the operator learns every fault at once.
// spec: PHW
#[tokio::test(start_paused = true)]
async fn every_check_is_carried_out_whatever_failed_before() {
	let mut fitted = Fitted::good();
	fitted.air = Err("the BME680 does not answer".to_owned());
	fitted.light = Err("the LTR-308ALS does not answer".to_owned());
	fitted.tpm = Err("the TPM does not answer".to_owned());
	let bench = Bench::new(fitted, true).await;
	bench.case.break_wire();
	bench.rig.start();
	let step = bench.rig.ended().await;

	assert_eq!(*bench.fitted.read.lock().unwrap(), ["air", "light", "tpm"]);
	let reason = step.reason.unwrap();
	for id in ["case: ", "air: ", "light: ", "tpm: "] {
		assert!(reason.contains(id), "{id} not in {reason}");
	}
	let failed: Vec<_> = checks(&step.result.unwrap())
		.into_iter()
		.filter(|(_, passed, _)| !passed)
		.map(|(id, ..)| id)
		.collect();
	assert_eq!(failed, ["case", "air", "light", "tpm"]);
}

// spec: PHW
#[tokio::test(start_paused = true)]
async fn a_board_no_row_matches_is_not_placed() {
	let mut fitted = Fitted::good();
	fitted.chip = 0x00;
	let bench = Bench::new(fitted, true).await;
	bench.rig.start();
	bench.rig.prompted("case").await;
	bench.case.work().await;
	let step = bench.rig.ended().await;
	assert_eq!(step.state, StepState::Failed);
	assert!(step.reason.unwrap().contains("could not be placed"));
}

// spec: RUN
#[tokio::test(start_paused = true)]
async fn a_check_always_says_not_done() {
	let bench = Bench::new(Fitted::good(), true).await;
	let step = Hardware::new(
		bench.fitted.clone(),
		button::record(button::Fake::new().0).1,
		Switch::default(),
	);
	let done = step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(step.id(), "hardware");
	assert_eq!(step.title(), "Check the hardware");
}
