use std::{fs::Permissions, os::unix::fs::symlink};

use bliti_core::provisioning::report::{StepReport, StepState};

use super::*;
use crate::provision::{fake_system::Bench, steps::testing::Rig};

/// The software the medium carries in the tests: a path under the tree, what it holds, its mode.
const SOFTWARE: [(&str, &[u8], u32); 3] = [
	("usr/local/bin/bliti", b"the daemon", 0o755),
	(SERVICE, b"[Unit]\n", 0o644),
	("etc/bliti/daemon.env", b"BLITI_DAEMON_ARGS=\n", 0o644),
];

fn step(bench: &Bench) -> Software {
	Software::with_images(bench.storage(), bench.luks(), &bench.images)
}

/// A bench with a system on its disk, whose root filesystem has the directories a system has, and a
/// medium carrying the software.
fn system() -> Bench {
	let bench = Bench::new();
	bench.shipped();
	fs::create_dir_all(bench.root.join("etc/systemd/system")).unwrap();
	for (path, content, mode) in SOFTWARE {
		write(&bench.images.join(TREE).join(path), content, mode);
	}
	bench
}

fn write(path: &Path, content: &[u8], mode: u32) {
	fs::create_dir_all(path.parent().unwrap()).unwrap();
	fs::write(path, content).unwrap();
	fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
}

/// Install the software into the bench's root filesystem, as the step would.
fn installed(bench: &Bench) {
	for (path, content, mode) in SOFTWARE {
		write(&bench.root.join(path), content, mode);
	}
	let link = bench.root.join(ENABLED);
	fs::create_dir_all(link.parent().unwrap()).unwrap();
	symlink(Path::new("/").join(SERVICE), link).unwrap();
}

fn mode(path: &Path) -> u32 {
	fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn assert_installed(bench: &Bench) {
	for (path, content, want) in SOFTWARE {
		let place = bench.root.join(path);
		assert_eq!(fs::read(&place).unwrap(), content, "{path}");
		assert_eq!(mode(&place), want, "{path}");
	}
	assert_eq!(
		fs::read_link(bench.root.join(ENABLED)).unwrap(),
		Path::new("/etc/systemd/system/bliti.service")
	);
}

async fn ran(bench: &Bench) -> StepReport {
	let rig = Rig::new(Arc::new(step(bench))).await;
	rig.start();
	rig.ended().await
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn software_installed_and_enabled_is_done() {
	let bench = system();
	installed(&bench);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_missing_file_is_not_done_and_says_which() {
	let bench = system();
	installed(&bench);
	fs::remove_file(bench.root.join("etc/bliti/daemon.env")).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(done.why.unwrap(), "/etc/bliti/daemon.env is missing");
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_file_that_differs_or_has_another_mode_is_not_done() {
	let bench = system();
	installed(&bench);
	fs::write(bench.root.join(SERVICE), b"an older unit").unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("bliti.service differs"));

	let bench = system();
	installed(&bench);
	let binary = bench.root.join("usr/local/bin/bliti");
	fs::set_permissions(&binary, Permissions::from_mode(0o644)).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("has mode 644, not 755"));
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_service_not_enabled_is_not_done() {
	let bench = system();
	installed(&bench);
	fs::remove_file(bench.root.join(ENABLED)).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(done.why.unwrap(), "the Iti's service is not enabled");

	fs::write(bench.root.join(ENABLED), b"not a link").unwrap();
	assert_eq!(step(&bench).done().await.state, DoneState::NotDone);

	fs::remove_file(bench.root.join(ENABLED)).unwrap();
	symlink("/dev/null", bench.root.join(ENABLED)).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("points elsewhere"));
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_system_that_cannot_be_looked_into_is_unknown() {
	let bench = system();
	bench.state.lock().unwrap().disks.clear();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);

	let bench = system();
	bench.state.lock().unwrap().volume = None;
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);

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
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("could not be opened"));
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_medium_without_the_software_makes_the_done_state_unknown() {
	let bench = system();
	fs::remove_dir_all(bench.images.join(TREE)).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("no software to install"));

	fs::create_dir_all(bench.images.join(TREE).join("etc")).unwrap();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn working_out_the_done_state_leaves_nothing_open_and_changes_nothing() {
	let bench = system();
	step(&bench).done().await;
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
	drop(state);
	for (path, ..) in SOFTWARE {
		assert!(!bench.root.join(path).exists(), "{path}");
	}
	assert!(fs::symlink_metadata(bench.root.join(ENABLED)).is_err());
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn the_software_is_installed_with_its_modes_and_enabled_and_nothing_is_left_open() {
	let bench = system();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_installed(&bench);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
	drop(state);
	let litter: Vec<_> = fs::read_dir(bench.root.join("etc/systemd/system"))
		.unwrap()
		.map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
		.filter(|name| name.starts_with('.'))
		.collect();
	assert!(litter.is_empty(), "{litter:?}");
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_file_added_to_the_medium_is_installed_with_no_change_to_the_step() {
	let bench = system();
	write(
		&bench
			.images
			.join(TREE)
			.join("etc/systemd/journald.conf.d/10-bliti.conf"),
		b"[Journal]\n",
		0o644,
	);
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert_eq!(
		fs::read(bench.root.join("etc/systemd/journald.conf.d/10-bliti.conf")).unwrap(),
		b"[Journal]\n"
	);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_file_that_differs_a_wrong_mode_and_a_wrong_link_are_replaced() {
	let bench = system();
	installed(&bench);
	fs::write(bench.root.join(SERVICE), b"an older unit").unwrap();
	fs::set_permissions(
		bench.root.join("usr/local/bin/bliti"),
		Permissions::from_mode(0o600),
	)
	.unwrap();
	fs::remove_file(bench.root.join(ENABLED)).unwrap();
	symlink("/dev/null", bench.root.join(ENABLED)).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_installed(&bench);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_system_already_holding_the_software_is_left_unwritten() {
	let bench = system();
	installed(&bench);
	let dirs = [
		bench.root.join("usr/local/bin"),
		bench.root.join("etc/systemd/system"),
		bench
			.root
			.join("etc/systemd/system/multi-user.target.wants"),
		bench.root.join("etc/bliti"),
	];
	for dir in &dirs {
		fs::set_permissions(dir, Permissions::from_mode(0o555)).unwrap();
	}
	let report = ran(&bench).await;
	for dir in &dirs {
		fs::set_permissions(dir, Permissions::from_mode(0o755)).unwrap();
	}
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_disk_with_no_root_filesystem_fails_the_step() {
	let bench = system();
	fs::remove_dir_all(bench.root.join("etc")).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no root filesystem"));
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_medium_without_the_software_fails_the_step() {
	let bench = system();
	fs::remove_dir_all(bench.images.join(TREE)).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no software to install"));
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_file_that_cannot_be_written_fails_the_step_saying_which() {
	let bench = system();
	installed(&bench);
	fs::remove_file(bench.root.join(SERVICE)).unwrap();
	fs::create_dir_all(bench.root.join(SERVICE).join("inside")).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	let reason = report.reason.unwrap();
	assert!(
		reason.contains("cannot write /etc/systemd/system/bliti.service"),
		"{reason}"
	);
}
