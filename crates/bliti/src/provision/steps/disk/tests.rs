use bliti_core::provisioning::report::StepState;

use super::*;
use crate::provision::{
	fake_system::{Bench, PAYLOAD_SIZE},
	steps::testing::{Rig, open},
};

fn step(bench: &Bench) -> Disk {
	Disk::with_images(bench.storage(), &bench.images)
}

fn marker(bench: &Bench, sha256: &str) {
	fs::write(
		bench.firmware.join(MARKER),
		format!("PI_IMAGE_VERSION=v0.9\nPI_IMAGE_SHA256={sha256}\n"),
	)
	.unwrap();
}

fn junk(bench: &Bench) {
	let mut bytes = fs::read(&bench.disk).unwrap();
	bytes[..4096].fill(b'x');
	fs::write(&bench.disk, bytes).unwrap();
}

/// What the disk holds of the payload, from its first byte.
fn written(bench: &Bench) -> &[u8] {
	&bench.payload
}

fn disk_head(bench: &Bench, len: usize) -> Vec<u8> {
	fs::read(&bench.disk).unwrap()[..len].to_vec()
}

async fn ran(bench: &Bench) -> bliti_core::provisioning::report::StepReport {
	let rig = Rig::new(Arc::new(step(bench))).await;
	rig.start();
	rig.ended().await
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn a_disk_with_the_mediums_marker_is_done() {
	let bench = Bench::new();
	marker(&bench, &bench.manifest().sha256);
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::Done);
	assert!(done.why.unwrap().contains("v0.9"));
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn a_disk_with_another_systems_marker_is_not_done_and_says_which() {
	let bench = Bench::new();
	marker(&bench, "deadbeef");
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("an earlier system, v0.9"));
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn a_blank_disk_is_not_done() {
	let bench = Bench::new();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("blank"));
}

// spec: PDSK#what-the-disk-holds
#[tokio::test]
async fn partitions_labelled_as_the_images_are_an_earlier_system_without_a_marker() {
	let bench = Bench::new();
	bench.write_payload(4096);
	bench.state.lock().unwrap().firmware_mountable = false;
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("earlier system"));
}

// spec: PDSK#what-the-disk-holds
#[tokio::test]
async fn anything_else_is_another_system() {
	let bench = Bench::new();
	junk(&bench);
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("other than the Iti's"));
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn no_disk_or_two_make_the_done_state_unknown() {
	let bench = Bench::new();
	bench.state.lock().unwrap().disks.clear();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
	bench.state.lock().unwrap().disks = vec![bench.disk.clone(), bench.disk.clone()];
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn a_medium_without_a_manifest_makes_the_done_state_unknown() {
	let bench = Bench::new();
	fs::remove_file(bench.images.join("manifest")).unwrap();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
}

// spec: PDSK#done-state-of-the-step-disk
#[tokio::test]
async fn working_out_the_done_state_changes_nothing() {
	let bench = Bench::new();
	marker(&bench, "deadbeef");
	let before = fs::read(&bench.disk).unwrap();
	step(&bench).done().await;
	assert_eq!(fs::read(&bench.disk).unwrap(), before);
	assert!(bench.firmware.join(MARKER).exists());
	assert_eq!(bench.state.lock().unwrap().mounts, 0);
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn a_blank_disk_is_written_read_back_and_marked_without_asking() {
	let bench = Bench::new();
	let rig = Rig::new(Arc::new(step(&bench))).await;
	rig.start();
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert!(open(&rig.provisioner.watch().borrow()).is_empty());
	assert_eq!(disk_head(&bench, PAYLOAD_SIZE), written(&bench));
	let manifest = bench.manifest();
	assert_eq!(
		bench.marker().unwrap(),
		format!(
			"PI_IMAGE_VERSION={}\nPI_IMAGE_SHA256={}\n",
			manifest.version, manifest.sha256
		)
	);
	assert_eq!(bench.state.lock().unwrap().mounts, 0);
}

// spec: PDSK#writing-the-system
#[test]
fn progress_runs_from_nothing_to_all_over_the_write_and_the_read_back() {
	use std::cell::RefCell;

	let bench = Bench::new();
	let seen = RefCell::new(Vec::new());
	write_and_verify(
		&*bench.storage(),
		&bench.manifest(),
		&bench.disk,
		&|fraction| {
			seen.borrow_mut().push(fraction);
		},
	)
	.unwrap();
	let seen = seen.into_inner();
	assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
	assert!(seen.iter().any(|f| (*f - 0.5).abs() < 1e-9), "{seen:?}");
	assert!((seen.last().unwrap() - 1.0).abs() < 1e-9, "{seen:?}");
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn the_bytes_after_the_system_are_left_alone() {
	let bench = Bench::new();
	let mut bytes = fs::read(&bench.disk).unwrap();
	bytes[PAYLOAD_SIZE..].fill(7);
	fs::write(&bench.disk, &bytes).unwrap();
	bench.write_payload(0);
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert!(
		fs::read(&bench.disk).unwrap()[PAYLOAD_SIZE..]
			.iter()
			.all(|b| *b == 7)
	);
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn another_system_is_written_over_only_where_the_operator_confirms() {
	let bench = Bench::new();
	junk(&bench);
	let rig = Rig::new(Arc::new(step(&bench))).await;
	rig.start();
	let held = rig.prompted("overwrite").await;
	let prompt = held.prompts.iter().find(|p| p.id == "overwrite").unwrap();
	assert!(
		prompt.text.contains("other than the Iti's"),
		"{}",
		prompt.text
	);
	assert_eq!(disk_head(&bench, 4), b"xxxx");
	rig.answer("overwrite", true);
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(disk_head(&bench, PAYLOAD_SIZE), written(&bench));
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn declining_leaves_the_disk_as_it_was_and_fails_the_step() {
	let bench = Bench::new();
	junk(&bench);
	let before = fs::read(&bench.disk).unwrap();
	let rig = Rig::new(Arc::new(step(&bench))).await;
	rig.start();
	rig.prompted("overwrite").await;
	rig.answer("overwrite", false);
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("left as it was"));
	assert_eq!(fs::read(&bench.disk).unwrap(), before);
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn the_mediums_own_system_and_an_earlier_one_are_written_again_without_asking() {
	for mediums in [true, false] {
		let bench = Bench::new();
		junk(&bench);
		let sha256 = if mediums {
			bench.manifest().sha256
		} else {
			"deadbeef".to_owned()
		};
		marker(&bench, &sha256);
		let rig = Rig::new(Arc::new(step(&bench))).await;
		rig.start();
		assert_eq!(rig.ended().await.state, StepState::Passed);
		assert!(open(&rig.provisioner.watch().borrow()).is_empty());
		assert_eq!(disk_head(&bench, PAYLOAD_SIZE), written(&bench));
	}
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn a_failed_read_of_the_payload_fails_the_step_and_leaves_no_marker() {
	let bench = Bench::new();
	marker(&bench, "deadbeef");
	bench.state.lock().unwrap().payload_fails_at = Some(8192);
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("provisioning medium"));
	assert_eq!(bench.marker(), None);
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn a_disk_written_in_part_is_written_again_without_asking() {
	let bench = Bench::new();
	bench.state.lock().unwrap().payload_fails_at = Some(8192);
	assert_eq!(ran(&bench).await.state, StepState::Failed);
	assert_eq!(disk_head(&bench, 8192), bench.payload[..8192]);

	bench.state.lock().unwrap().payload_fails_at = None;
	bench.state.lock().unwrap().firmware_mountable = true;
	let rig = Rig::new(Arc::new(step(&bench))).await;
	rig.start();
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert!(open(&rig.provisioner.watch().borrow()).is_empty());
	assert_eq!(disk_head(&bench, PAYLOAD_SIZE), written(&bench));
}

// spec: PDSK#writing-the-system
#[tokio::test]
async fn what_reads_back_differently_fails_the_step_and_leaves_no_marker() {
	let bench = Bench::new();
	let manifest = fs::read_to_string(bench.images.join("manifest")).unwrap();
	let sha256 = bench.manifest().sha256;
	fs::write(
		bench.images.join("manifest"),
		manifest.replace(&sha256, &"0".repeat(64)),
	)
	.unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("read back"));
	assert_eq!(bench.marker(), None);
}

// spec: PDSK#the-disk
#[tokio::test]
async fn a_drive_smaller_than_the_system_fails_the_step_and_is_not_written() {
	let bench = Bench::new();
	fs::write(&bench.disk, vec![0u8; PAYLOAD_SIZE - 1]).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("smaller"));
	assert!(fs::read(&bench.disk).unwrap().iter().all(|b| *b == 0));
}

// spec: PDSK#the-disk
#[tokio::test]
async fn no_drive_or_two_fail_the_step_saying_so() {
	let bench = Bench::new();
	bench.state.lock().unwrap().disks.clear();
	assert!(ran(&bench).await.reason.unwrap().contains("no NVMe drive"));

	let bench = Bench::new();
	bench.state.lock().unwrap().disks = vec![bench.disk.clone(), bench.disk.clone()];
	assert!(ran(&bench).await.reason.unwrap().contains("more than one"));
}
