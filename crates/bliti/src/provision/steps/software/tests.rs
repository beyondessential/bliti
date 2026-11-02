use std::{collections::HashMap, fs::Permissions, ops::Deref, os::unix::fs::symlink, sync::Mutex};

use bliti_core::provisioning::report::{StepReport, StepState};

use super::{
	network::{Action, NOT_FOUND},
	*,
};
use crate::provision::{fake_system::Bench, steps::testing::Rig};

/// The software the medium carries in the tests: a path under the tree, what it holds, its mode.
const SOFTWARE: [(&str, &[u8], u32); 3] = [
	("usr/local/bin/bliti", b"the daemon", 0o755),
	(SERVICE, b"[Unit]\n", 0o644),
	("etc/bliti/daemon.env", b"BLITI_DAEMON_ARGS=\n", 0o644),
];

/// The packages the medium carries in the tests, by their files: the two the network needs, and a
/// library they need that the image lacks.
const HOSTAPD: &str = "hostapd_2%3a2.11-1_arm64.deb";
const IWD: &str = "iwd_3.10-1_arm64.deb";
const LIBELL: &str = "libell0_0.80-1_arm64.deb";

/// The packages of the image as it ships, which lacks what the network needs.
const SHIPPED: [&str; 2] = ["bluez", "wpasupplicant"];

/// The status of a wholly installed package.
const INSTALLED: &str = "install ok installed";

/// A call made of the installer.
#[derive(Debug, Clone, PartialEq)]
enum Call {
	Install(Vec<String>),
	Set(String, Action),
}

/// What the fake installer holds: the state of each unit, and the calls made of it.
#[derive(Default)]
struct Held {
	units: HashMap<String, String>,
	calls: Vec<Call>,
	/// Whether the next install stops after unpacking its first package.
	interrupt: bool,
	/// Why the next install fails, where it does.
	fails: Option<String>,
}

/// A package manager and service manager that keep the package database in the root filesystem, as
/// dpkg does, and the units' states in memory.
struct FakeInstaller(Arc<Mutex<Held>>);

impl Installer for FakeInstaller {
	fn install(&self, root: &Path, debs: &Path, names: &[String]) -> Result<(), String> {
		let mut held = self.0.lock().unwrap();
		held.calls.push(Call::Install(names.to_vec()));
		if let Some(why) = held.fails.take() {
			return Err(why);
		}
		for (package, _) in statuses(root) {
			set_status(root, &package, INSTALLED);
		}
		let _ = fs::remove_dir_all(root.join("var/lib/dpkg/updates"));
		for name in names {
			assert!(debs.join(name).is_file(), "{name} is not carried");
			let package = name.split_once('_').unwrap().0;
			if std::mem::take(&mut held.interrupt) {
				set_status(root, package, "install ok unpacked");
				return Err("dpkg was interrupted".to_owned());
			}
			set_status(root, package, INSTALLED);
			// As the packages' own scripts leave their units.
			if let Some(unit) = match package {
				"iwd" => Some("iwd.service"),
				"hostapd" => Some("hostapd.service"),
				_ => None,
			} {
				held.units.insert(unit.to_owned(), "enabled".to_owned());
			}
		}
		Ok(())
	}

	fn state(&self, _root: &Path, unit: &str) -> Result<String, String> {
		let held = self.0.lock().unwrap();
		Ok(held
			.units
			.get(unit)
			.cloned()
			.unwrap_or_else(|| NOT_FOUND.to_owned()))
	}

	fn set(&self, _root: &Path, unit: &str, action: Action) -> Result<(), String> {
		let mut held = self.0.lock().unwrap();
		held.calls.push(Call::Set(unit.to_owned(), action));
		let state = match action {
			Action::Disable | Action::Unmask => "disabled",
			Action::Mask => "masked",
		};
		held.units.insert(unit.to_owned(), state.to_owned());
		Ok(())
	}
}

/// The packages the database in the root filesystem at `root` records, with their statuses.
fn statuses(root: &Path) -> Vec<(String, String)> {
	let text = fs::read_to_string(root.join("var/lib/dpkg/status")).unwrap_or_default();
	text.split("\n\n")
		.filter_map(|stanza| {
			let field = |name: &str| {
				stanza
					.lines()
					.find_map(|line| line.strip_prefix(name))
					.map(str::to_owned)
			};
			Some((field("Package: ")?, field("Status: ")?))
		})
		.collect()
}

/// Record `package` with `status` in the database in the root filesystem at `root`.
fn set_status(root: &Path, package: &str, status: &str) {
	let mut held = statuses(root);
	match held.iter_mut().find(|(name, _)| name == package) {
		Some(entry) => entry.1 = status.to_owned(),
		None => held.push((package.to_owned(), status.to_owned())),
	}
	let text: Vec<String> = held
		.iter()
		.map(|(name, status)| {
			format!(
				"Package: {name}\nStatus: {status}\nArchitecture: arm64\nDescription: a package\n .\n more\n"
			)
		})
		.collect();
	let path = root.join("var/lib/dpkg/status");
	fs::create_dir_all(path.parent().unwrap()).unwrap();
	fs::write(path, text.join("\n")).unwrap();
}

/// A bench, and the installer the step is given.
struct Disk {
	bench: Bench,
	held: Arc<Mutex<Held>>,
}

impl Deref for Disk {
	type Target = Bench;

	fn deref(&self) -> &Bench {
		&self.bench
	}
}

impl Disk {
	fn calls(&self) -> Vec<Call> {
		self.held.lock().unwrap().calls.clone()
	}

	fn unit(&self, unit: &str) -> String {
		self.held
			.lock()
			.unwrap()
			.units
			.get(unit)
			.cloned()
			.unwrap_or_else(|| NOT_FOUND.to_owned())
	}

	fn set_unit(&self, unit: &str, state: &str) {
		self.held
			.lock()
			.unwrap()
			.units
			.insert(unit.to_owned(), state.to_owned());
	}

	fn status(&self, package: &str) -> Option<String> {
		statuses(&self.root)
			.into_iter()
			.find(|(name, _)| name == package)
			.map(|(_, status)| status)
	}
}

fn step(bench: &Disk) -> Software {
	Software::with(
		bench.storage(),
		bench.luks(),
		Arc::new(FakeInstaller(bench.held.clone())),
		&bench.images,
	)
}

/// A bench with a system on its disk, whose root filesystem has the directories and packages the
/// image ships with, and a medium carrying the software and the packages.
fn system() -> Disk {
	let bench = Bench::new();
	bench.shipped();
	fs::create_dir_all(bench.root.join("etc/systemd/system")).unwrap();
	for package in SHIPPED {
		set_status(&bench.root, package, INSTALLED);
	}
	for (path, content, mode) in SOFTWARE {
		write(&bench.images.join(TREE).join(path), content, mode);
	}
	for deb in [HOSTAPD, IWD, LIBELL] {
		write(&bench.images.join(DEBS).join(deb), b"a package", 0o644);
	}
	let held = Held {
		units: HashMap::from([("wpa_supplicant.service".to_owned(), "enabled".to_owned())]),
		..Held::default()
	};
	Disk {
		bench,
		held: Arc::new(Mutex::new(held)),
	}
}

fn write(path: &Path, content: &[u8], mode: u32) {
	fs::create_dir_all(path.parent().unwrap()).unwrap();
	fs::write(path, content).unwrap();
	fs::set_permissions(path, Permissions::from_mode(mode)).unwrap();
}

/// Install the software and the packages into the bench's root filesystem, and set the units, as the
/// step would.
fn installed(bench: &Disk) {
	for (path, content, mode) in SOFTWARE {
		write(&bench.root.join(path), content, mode);
	}
	let link = bench.root.join(ENABLED);
	fs::create_dir_all(link.parent().unwrap()).unwrap();
	symlink(Path::new("/").join(SERVICE), link).unwrap();
	for package in ["iwd", "hostapd", "libell0"] {
		set_status(&bench.root, package, INSTALLED);
	}
	for (unit, state) in [
		("iwd.service", "disabled"),
		("wpa_supplicant.service", "disabled"),
		("hostapd.service", "masked"),
	] {
		bench.set_unit(unit, state);
	}
}

fn mode(path: &Path) -> u32 {
	fs::metadata(path).unwrap().permissions().mode() & 0o7777
}

fn assert_installed(bench: &Disk) {
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

async fn ran(bench: &Disk) -> StepReport {
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
	assert_eq!(bench.calls(), []);
	assert_eq!(bench.status("iwd"), None);
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
	assert_eq!(bench.calls(), []);
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

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn packages_the_network_needs_that_are_missing_are_not_done_and_say_which() {
	let bench = system();
	installed(&bench);
	set_status(&bench.root, "iwd", "deinstall ok config-files");
	set_status(&bench.root, "hostapd", "install ok half-configured");
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(
		done.why.unwrap(),
		"iwd is not wholly installed (deinstall ok config-files); \
		 hostapd is not wholly installed (install ok half-configured)"
	);

	let bench = system();
	for (path, content, mode) in SOFTWARE {
		write(&bench.root.join(path), content, mode);
	}
	let link = bench.root.join(ENABLED);
	fs::create_dir_all(link.parent().unwrap()).unwrap();
	symlink(Path::new("/").join(SERVICE), link).unwrap();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(
		done.why.unwrap(),
		"iwd is not installed; hostapd is not installed; iwd.service is not installed; \
		 wpa_supplicant.service is enabled, not disabled; hostapd.service is not installed"
	);
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn an_interrupted_package_manager_is_not_done() {
	let bench = system();
	installed(&bench);
	write(
		&bench.root.join("var/lib/dpkg/updates/0003"),
		b"Package: iwd\n",
		0o644,
	);
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert_eq!(
		done.why.unwrap(),
		"the system's package manager was interrupted"
	);

	fs::remove_file(bench.root.join("var/lib/dpkg/updates/0003")).unwrap();
	write(&bench.root.join("var/lib/dpkg/updates/tmp.i"), b"", 0o644);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_unit_in_another_state_is_not_done_and_says_which() {
	for (unit, state, why) in [
		(
			"iwd.service",
			"enabled",
			"iwd.service is enabled, not disabled",
		),
		(
			"iwd.service",
			"masked",
			"iwd.service is masked, not disabled",
		),
		(
			"wpa_supplicant.service",
			"enabled",
			"wpa_supplicant.service is enabled, not disabled",
		),
		(
			"hostapd.service",
			"disabled",
			"hostapd.service is disabled, not masked",
		),
	] {
		let bench = system();
		installed(&bench);
		bench.set_unit(unit, state);
		let done = step(&bench).done().await;
		assert_eq!(done.state, DoneState::NotDone, "{unit} {state}");
		assert_eq!(done.why.unwrap(), why);
	}
}

// spec: PDSK#done-state-of-the-step-software
#[tokio::test]
async fn a_system_without_wpa_supplicant_or_with_it_masked_is_done() {
	for state in [NOT_FOUND, "masked"] {
		let bench = system();
		installed(&bench);
		bench.set_unit("wpa_supplicant.service", state);
		assert_eq!(step(&bench).done().await.state, DoneState::Done, "{state}");
	}
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn the_packages_the_network_needs_are_installed_from_the_medium_and_their_units_set() {
	let bench = system();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_eq!(
		bench.calls(),
		[
			Call::Install(vec![HOSTAPD.to_owned(), IWD.to_owned(), LIBELL.to_owned()]),
			Call::Set("iwd.service".to_owned(), Action::Disable),
			Call::Set("wpa_supplicant.service".to_owned(), Action::Disable),
			Call::Set("hostapd.service".to_owned(), Action::Mask),
		]
	);
	for package in ["iwd", "hostapd", "libell0"] {
		assert_eq!(
			bench.status(package).as_deref(),
			Some(INSTALLED),
			"{package}"
		);
	}
	assert_eq!(bench.unit("iwd.service"), "disabled");
	assert_eq!(bench.unit("wpa_supplicant.service"), "disabled");
	assert_eq!(bench.unit("hostapd.service"), "masked");
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn only_the_packages_the_system_lacks_are_installed() {
	let bench = system();
	set_status(&bench.root, "libell0", INSTALLED);
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_eq!(
		bench.calls()[0],
		Call::Install(vec![HOSTAPD.to_owned(), IWD.to_owned()])
	);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_system_holding_the_packages_needs_none_from_the_medium_and_only_its_units_set() {
	let bench = system();
	installed(&bench);
	fs::remove_dir_all(bench.images.join(DEBS)).unwrap();
	bench.set_unit("iwd.service", "masked");
	bench.set_unit("hostapd.service", "enabled");
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_eq!(
		bench.calls(),
		[
			Call::Set("iwd.service".to_owned(), Action::Unmask),
			Call::Set("iwd.service".to_owned(), Action::Disable),
			Call::Set("hostapd.service".to_owned(), Action::Mask),
		]
	);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn an_interrupted_install_is_finished_by_running_the_step_again() {
	let bench = system();
	bench.held.lock().unwrap().interrupt = true;
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("dpkg was interrupted"));
	assert_eq!(
		bench.status("hostapd").as_deref(),
		Some("install ok unpacked")
	);
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(
		done.why
			.unwrap()
			.contains("hostapd is not wholly installed (install ok unpacked)")
	);

	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_eq!(
		bench.calls()[1],
		Call::Install(vec![HOSTAPD.to_owned(), IWD.to_owned(), LIBELL.to_owned()])
	);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_package_manager_interrupted_after_installing_is_finished() {
	let bench = system();
	installed(&bench);
	write(
		&bench.root.join("var/lib/dpkg/updates/0003"),
		b"Package: iwd\n",
		0o644,
	);
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_eq!(bench.calls(), [Call::Install(Vec::new())]);
	assert_eq!(step(&bench).done().await.state, DoneState::Done);
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_medium_without_the_packages_a_system_lacks_fails_the_step() {
	let bench = system();
	fs::remove_dir_all(bench.images.join(DEBS)).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(
		report
			.reason
			.unwrap()
			.contains("the medium carries no packages to install")
	);

	let bench = system();
	fs::remove_file(bench.images.join(DEBS).join(IWD)).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	let reason = report.reason.unwrap();
	assert!(reason.contains("did not read back"), "{reason}");
	assert!(reason.contains("iwd is not installed"), "{reason}");
}

// spec: PDSK#installing-the-itis-software
#[tokio::test]
async fn a_failed_install_fails_the_step_and_leaves_nothing_open() {
	let bench = system();
	bench.held.lock().unwrap().fails = Some("dpkg failed with exit status: 1".to_owned());
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("dpkg failed"));
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
}
