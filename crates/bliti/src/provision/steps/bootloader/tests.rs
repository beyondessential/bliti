use std::sync::atomic::Ordering;

use bliti_core::provisioning::report::StepState;
use jiff::civil::date;

use super::*;
use crate::provision::{
	bootloader::{
		date as date_of,
		testing::{Fake, image},
	},
	steps::testing::{Rig, open},
};

/// The settings the step writes, as the board's configuration is read back.
const IT: &str = "[all]\nBOOT_ORDER=0xf61\nPOWER_OFF_ON_HALT=1\nPSU_MAX_CURRENT=5000\nPCIE_PROBE=1\nBOOT_UART=1\nHDMI_DELAY=0\n";
const STOCK: &str = "[all]\nBOOT_UART=1\nBOOT_ORDER=0xf461\nNET_INSTALL_AT_POWER_ON=1\n";

const MEDIUMS: Date = date(2026, 9, 25);

/// A step over a flash holding `held`, and a medium carrying a bootloader dated `MEDIUMS`.
fn step(held: Vec<u8>) -> (Arc<Fake>, Bootloader, tempfile::TempDir) {
	let dir = tempfile::tempdir().unwrap();
	fs::write(dir.path().join("pieeprom.bin"), image(MEDIUMS, STOCK)).unwrap();
	fs::write(
		dir.path().join("manifest"),
		format!("BUILT=2026-09-25T01:49:00Z\nBOOTLOADER_VERSION={MEDIUMS}\n"),
	)
	.unwrap();
	let flash = Arc::new(Fake::holding(held));
	let step = Bootloader::with_images(flash.clone(), dir.path());
	(flash, step, dir)
}

fn configuration(flash: &Fake) -> String {
	flash.config(&flash.contents()).unwrap()
}

// spec: PBL#done-state
#[tokio::test]
async fn a_board_with_the_itis_configuration_and_the_mediums_bootloader_is_done() {
	let (_, step, _dir) = step(image(MEDIUMS, IT));
	let done = step.done().await;
	assert_eq!(done.state, DoneState::Done);
}

// spec: PBL#done-state
#[tokio::test]
async fn a_newer_bootloader_is_done() {
	let (_, step, _dir) = step(image(date(2026, 10, 20), IT));
	assert_eq!(step.done().await.state, DoneState::Done);
}

// spec: PBL#done-state
#[tokio::test]
async fn another_configuration_is_not_done_and_says_what_differs() {
	let (_, step, _dir) = step(image(MEDIUMS, STOCK));
	let done = step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	let why = done.why.unwrap();
	assert!(why.contains("BOOT_ORDER is 0xf461"), "{why}");
	assert!(why.contains("NET_INSTALL_AT_POWER_ON is set"), "{why}");
}

// spec: PBL#done-state
#[tokio::test]
async fn an_older_bootloader_is_not_done() {
	let (_, step, _dir) = step(image(date(2026, 5, 26), IT));
	let done = step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("2026-05-26"));
}

// spec: PBL#done-state
#[tokio::test]
async fn a_board_that_cannot_be_read_is_unknown() {
	let (flash, step, _dir) = step(image(MEDIUMS, IT));
	flash.unreadable.store(true, Ordering::SeqCst);
	assert_eq!(step.done().await.state, DoneState::Unknown);
}

// spec: PBL#done-state
#[tokio::test]
async fn a_medium_with_no_bootloader_or_no_date_for_it_is_unknown() {
	let (_, step, dir) = step(image(MEDIUMS, IT));
	fs::write(dir.path().join("manifest"), "BUILT=2026-09-25T01:49:00Z\n").unwrap();
	let done = step.done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("no date"));

	fs::remove_file(dir.path().join("pieeprom.bin")).unwrap();
	let done = step.done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("no bootloader"));
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn an_older_bootloader_is_replaced_by_the_mediums_with_the_configuration_set() {
	let (flash, step, _dir) = step(image(date(2026, 5, 26), STOCK));
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Passed);
	assert_eq!(date_of(&flash.contents()).unwrap(), MEDIUMS);
	assert_eq!(configuration(&flash), IT);
}

// spec: PBL#the-bootloader
#[tokio::test]
async fn a_newer_bootloader_is_not_replaced_by_the_mediums() {
	let newer = date(2026, 10, 20);
	let (flash, step, _dir) = step(image(newer, STOCK));
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(date_of(&flash.contents()).unwrap(), newer);
	assert_eq!(configuration(&flash), IT);
}

// spec: PBL#the-bootloader
#[tokio::test]
async fn a_bootloader_with_the_same_date_is_kept() {
	let (flash, step, _dir) = step(image(MEDIUMS, STOCK));
	let held = flash.contents();
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(date_of(&flash.contents()).unwrap(), MEDIUMS);
	assert_ne!(flash.contents(), held);
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn a_board_already_done_passes_without_writing() {
	let (flash, step, _dir) = step(image(MEDIUMS, IT));
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(flash.writes(), 0);
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn the_step_asks_the_operator_nothing() {
	let (_, step, _dir) = step(image(date(2026, 5, 26), STOCK));
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	rig.ended().await;
	assert!(open(&rig.provisioner.watch().borrow()).is_empty());
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn a_write_that_did_not_take_fails_the_step() {
	let (flash, step, _dir) = step(image(date(2026, 5, 26), STOCK));
	flash.forgetful.store(true, Ordering::SeqCst);
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Failed);
	let reason = report.reason.unwrap();
	assert!(reason.contains("could not be set"), "{reason}");
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn a_medium_with_no_bootloader_fails_the_step_naming_it() {
	let (flash, step, dir) = step(image(date(2026, 5, 26), STOCK));
	fs::remove_file(dir.path().join("pieeprom.bin")).unwrap();
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no bootloader"));
	assert_eq!(flash.writes(), 0);
}

// spec: PBL#carrying-out-the-step
#[tokio::test]
async fn a_board_that_cannot_be_read_fails_the_step() {
	let (flash, step, _dir) = step(image(MEDIUMS, IT));
	flash.unreadable.store(true, Ordering::SeqCst);
	let rig = Rig::new(Arc::new(step)).await;
	rig.start();
	assert_eq!(rig.ended().await.state, StepState::Failed);
}
