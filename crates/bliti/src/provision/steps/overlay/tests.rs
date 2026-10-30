use bliti_core::provisioning::report::StepReport;
use bliti_core::provisioning::report::StepState;

use super::*;
use crate::provision::{
	fake_system::{Bench, Unlock},
	steps::testing::Rig,
};

const OVERLAY: &[u8] = b"the HAT overlay";

fn step(bench: &Bench) -> Overlay {
	Overlay::with_images(bench.storage(), bench.luks(), &bench.images)
}

/// A bench with a system on its disk: a volume as the image ships it, whose root filesystem has the
/// directories a system has.
fn system() -> Bench {
	let bench = Bench::new();
	bench.shipped();
	fs::create_dir_all(bench.root.join("etc")).unwrap();
	bench
}

fn place(bench: &Bench, content: &[u8]) -> [PathBuf; 2] {
	let places = places(&bench.firmware, &bench.root);
	for place in &places {
		fs::create_dir_all(place.parent().unwrap()).unwrap();
		fs::write(place, content).unwrap();
	}
	places
}

async fn ran(bench: &Bench) -> StepReport {
	let rig = Rig::new(Arc::new(step(bench))).await;
	rig.start();
	rig.ended().await
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn an_overlay_in_both_places_is_done() {
	let bench = system();
	place(&bench, OVERLAY);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn an_overlay_missing_from_either_place_is_not_done_and_says_where() {
	let bench = system();
	let [slot, updates] = place(&bench, OVERLAY);

	fs::remove_file(&slot).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(
		done.why
			.unwrap()
			.contains("missing from the current boot slot")
	);

	fs::write(&slot, OVERLAY).unwrap();
	fs::remove_file(&updates).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("kernel updates"));
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn an_overlay_that_differs_is_not_done() {
	let bench = system();
	let [slot, _] = place(&bench, OVERLAY);
	fs::write(slot, b"an older overlay").unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("differs"));
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn a_system_that_cannot_be_looked_into_is_unknown() {
	let bench = system();
	bench.state.lock().unwrap().disks.clear();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);

	let bench = system();
	bench.state.lock().unwrap().firmware_mountable = false;
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);

	let bench = Bench::new();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::Unknown);

	let bench = system();
	bench
		.state
		.lock()
		.unwrap()
		.volume
		.as_mut()
		.unwrap()
		.slots
		.clear();
	assert!(
		step(&bench)
			.done()
			.await
			.why
			.unwrap()
			.contains("could not be opened")
	);
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn a_medium_without_the_overlay_makes_the_done_state_unknown() {
	let bench = system();
	fs::remove_file(bench.images.join(NAME)).unwrap();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
}

// spec: PDSK#done-state-of-the-step-overlay
#[tokio::test]
async fn working_out_the_done_state_leaves_nothing_open_and_changes_nothing() {
	let bench = system();
	step(&bench).done().await;
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
	drop(state);
	assert!(!places(&bench.firmware, &bench.root)[0].exists());
	assert!(!places(&bench.firmware, &bench.root)[1].exists());
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn the_overlay_is_installed_in_both_places_and_nothing_is_left_open() {
	let bench = system();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	for place in places(&bench.firmware, &bench.root) {
		assert_eq!(fs::read(place).unwrap(), OVERLAY);
	}
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
	drop(state);
	let slot: Vec<_> = fs::read_dir(bench.firmware.join(SLOT))
		.unwrap()
		.map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
		.collect();
	assert_eq!(slot.len(), 2, "{slot:?}");
	assert!(slot.contains(&"README".to_owned()));
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn an_older_overlay_is_replaced() {
	let bench = system();
	place(&bench, b"an older overlay");
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	for place in places(&bench.firmware, &bench.root) {
		assert_eq!(fs::read(place).unwrap(), OVERLAY);
	}
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn a_system_already_holding_the_overlay_is_left_unwritten() {
	use std::os::unix::fs::PermissionsExt as _;

	let bench = system();
	let [slot, updates] = place(&bench, OVERLAY);
	let dirs = [
		slot.parent().unwrap().to_owned(),
		updates.parent().unwrap().to_owned(),
	];
	for dir in &dirs {
		fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
	}
	let report = ran(&bench).await;
	for dir in &dirs {
		fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
	}
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn a_disk_with_no_current_boot_slot_fails_the_step() {
	let bench = system();
	fs::remove_dir_all(bench.firmware.join("current")).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no current boot slot"));
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn a_disk_with_no_root_filesystem_fails_the_step() {
	let bench = system();
	fs::remove_dir_all(bench.root.join("etc")).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no root filesystem"));
}

// spec: PDSK#installing-the-overlay
#[tokio::test]
async fn a_disk_with_no_system_fails_the_step_saying_so() {
	let bench = system();
	bench.state.lock().unwrap().firmware_mountable = false;
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no firmware partition"));
}

// spec: PDSK#opening-the-root-volume
#[tokio::test]
async fn the_root_volume_is_opened_by_the_tpm_where_the_empty_passphrase_does_not_open_it() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots = vec![(
		3,
		Unlock::Tpm {
			ours: true,
			pcrs: Vec::new(),
		},
	)];
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	for place in places(&bench.firmware, &bench.root) {
		assert_eq!(fs::read(place).unwrap(), OVERLAY);
	}
}

// spec: PDSK#opening-the-root-volume
#[tokio::test]
async fn a_volume_neither_the_empty_passphrase_nor_the_tpm_opens_fails_the_step() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots = vec![(
		3,
		Unlock::Tpm {
			ours: false,
			pcrs: Vec::new(),
		},
	)];
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("could not be opened"));
}
