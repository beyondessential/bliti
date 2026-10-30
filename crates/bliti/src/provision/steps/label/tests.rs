use bliti_core::{
	key_schedule::Root,
	provisioning::{
		Name, Suffix,
		report::{DoneState, StepState},
	},
	qr::QrPayload,
	version::VERSION_MARKER,
};
use tokio::sync::watch;

use super::*;
use crate::{
	facts::i2c,
	provision::{
		name::Established,
		steps::{
			eeprom::{image, iti_4_0},
			testing::Rig,
		},
	},
};

/// An EEPROM holding `bytes`, or one that cannot be read.
struct Rom(Option<Vec<u8>>);

impl Memory for Rom {
	fn read(&self, len: usize) -> Result<Vec<u8>, String> {
		let bytes = self.0.as_ref().ok_or("no answer")?;
		let mut read = bytes.clone();
		read.resize(len.max(bytes.len()), 0xff);
		read.truncate(len);
		Ok(read)
	}

	fn write(&self, _: &[u8]) -> Result<(), String> {
		Err("these tests write nothing".to_owned())
	}
}

/// What answers at the air sensor's address.
struct Sensor(Option<u8>);

impl Air for Sensor {
	fn chip_id(&self) -> Result<u8, i2c::Error> {
		self.0.ok_or(i2c::Error::NoAnswer)
	}
}

fn payload() -> QrPayload {
	let keys = Root::from_bytes([7; 32]).device_keys();
	QrPayload::new(keys.presence_token.clone(), keys.fingerprint())
}

fn established(payload: Option<QrPayload>) -> Known {
	let suffix = payload.as_ref().map_or(Suffix::Fault, Suffix::of);
	let (sender, receiver) = watch::channel(Some(Established {
		name: Name {
			marker: VERSION_MARKER,
			suffix,
		},
		payload,
	}));
	// A sender that is gone leaves the last value, which is all these tests need.
	drop(sender);
	receiver
}

fn step(known: Known, air: Option<u8>, eeprom: Option<Vec<u8>>) -> Arc<Label> {
	Arc::new(Label::new(
		known,
		Arc::new(Sensor(air)),
		Arc::new(Rom(eeprom)),
	))
}

/// A BME680 answering, which places the board as 4.0.
const BME680: Option<u8> = Some(0x61);

fn blank() -> Option<Vec<u8>> {
	Some(vec![0xff; 4096])
}

async fn carried_out(step: Arc<Label>) -> bliti_core::provisioning::report::StepReport {
	let rig = Rig::new(step).await;
	rig.start();
	rig.ended().await
}

fn text(result: &serde_json::Map<String, Json>, member: &str) -> String {
	result[member].as_str().expect("a string").to_owned()
}

#[tokio::test]
async fn it_is_never_done() {
	let done = step(established(Some(payload())), BME680, blank())
		.done()
		.await;
	assert_eq!(done.state, DoneState::NotDone);
}

#[tokio::test]
async fn the_result_carries_the_payload_the_version_and_the_time() {
	let code = payload();
	let report = carried_out(step(established(Some(code.clone())), BME680, blank())).await;
	assert_eq!(report.state, StepState::Passed);
	let result = report.result.expect("a result");
	assert_eq!(text(&result, "payload"), code.to_text());
	assert_eq!(text(&result, "version"), "4.0");
	let at = text(&result, "at");
	assert!(at.ends_with('Z'), "{at}");
	assert!(at.parse::<jiff::Timestamp>().is_ok(), "{at}");
	assert_eq!(result.len(), 3);
}

#[tokio::test]
async fn the_eeprom_names_the_version_before_the_hardware_does() {
	let eeprom = image(
		2,
		"7c3b8a88-ad89-4d48-b709-2bd445729867",
		4,
		1,
		b"BES International",
		b"Tamanu Iti v4.1",
		"tamanu-iti-v4",
	);
	let report = carried_out(step(established(Some(payload())), BME680, Some(eeprom))).await;
	assert_eq!(text(&report.result.unwrap(), "version"), "4.1");
}

#[tokio::test]
async fn an_eeprom_the_step_would_have_written_gives_its_version() {
	let report = carried_out(step(established(Some(payload())), None, Some(iti_4_0()))).await;
	assert_eq!(text(&report.result.unwrap(), "version"), "4.0");
}

#[tokio::test]
async fn another_hats_eeprom_leaves_the_hardware_to_place_the_board() {
	let other = image(
		2,
		"11111111-2222-3333-4444-555555555555",
		9,
		9,
		b"Someone Else",
		b"Another HAT",
		"other",
	);
	let report = carried_out(step(established(Some(payload())), BME680, Some(other))).await;
	assert_eq!(text(&report.result.unwrap(), "version"), "4.0");
}

#[tokio::test]
async fn an_eeprom_that_cannot_be_read_leaves_the_hardware_to_place_the_board() {
	let report = carried_out(step(established(Some(payload())), BME680, None)).await;
	assert_eq!(report.state, StepState::Passed);
	assert_eq!(text(&report.result.unwrap(), "version"), "4.0");
}

#[tokio::test]
async fn a_board_neither_names_nor_places_fails_with_no_payload() {
	let report = carried_out(step(established(Some(payload())), None, blank())).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("needs looking at"));
	assert!(report.result.is_none());
}

#[tokio::test]
async fn a_device_without_a_board_id_fails_with_no_payload() {
	let report = carried_out(step(established(None), BME680, blank())).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("board's ID"));
	assert!(report.result.is_none());
}

#[tokio::test]
async fn carried_out_again_it_gives_a_later_time() {
	let label = step(established(Some(payload())), BME680, blank());
	let first = carried_out(label.clone()).await;
	tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
	let second = carried_out(label).await;
	assert!(text(&second.result.unwrap(), "at") > text(&first.result.unwrap(), "at"));
}
