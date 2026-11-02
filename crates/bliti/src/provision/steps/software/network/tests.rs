use super::*;

fn root_with(status: &str) -> tempfile::TempDir {
	let root = tempfile::tempdir().unwrap();
	let path = root.path().join(STATUS);
	fs::create_dir_all(path.parent().unwrap()).unwrap();
	fs::write(path, status).unwrap();
	root
}

#[test]
fn an_installed_architecture_of_a_package_counts_whatever_order_they_come_in() {
	let root = root_with(
		"Package: iwd\nStatus: install ok installed\nArchitecture: arm64\nDescription: wireless\n daemon\n\n\
		 Package: iwd\nStatus: deinstall ok config-files\nArchitecture: armhf\n\n\
		 Package: hostapd\nStatus: deinstall ok config-files\nArchitecture: armhf\n\n\
		 Package: hostapd\nStatus: install ok installed\nArchitecture: arm64\n",
	);
	let status = Status::read(root.path()).unwrap();
	assert!(status.installed("iwd"));
	assert!(status.installed("hostapd"));
	assert!(status.lacks().is_empty());
}

#[test]
fn a_package_not_wholly_installed_or_absent_is_lacking() {
	let root = root_with(
		"Package: iwd\nStatus: install ok unpacked\n\nPackage: bluez\nStatus: install ok installed\n",
	);
	let status = Status::read(root.path()).unwrap();
	assert!(!status.installed("iwd"));
	assert!(status.installed("bluez"));
	assert_eq!(
		status.lacks(),
		[
			"iwd is not wholly installed (install ok unpacked)",
			"hostapd is not installed"
		]
	);
}

#[test]
fn a_journalled_change_marks_the_package_manager_interrupted() {
	let root = root_with(
		"Package: iwd\nStatus: install ok installed\n\nPackage: hostapd\nStatus: install ok installed\n",
	);
	let journal = root.path().join(JOURNAL);
	fs::create_dir_all(&journal).unwrap();
	fs::write(journal.join("tmp.i"), "").unwrap();
	assert!(Status::read(root.path()).unwrap().lacks().is_empty());
	fs::write(journal.join("0000"), "Package: iwd\n").unwrap();
	assert_eq!(
		Status::read(root.path()).unwrap().lacks(),
		["the system's package manager was interrupted"]
	);
}

#[test]
fn a_system_with_no_package_database_cannot_be_read() {
	let root = tempfile::tempdir().unwrap();
	assert!(
		Status::read(root.path())
			.err()
			.unwrap()
			.contains("cannot read the system's package database")
	);
}

#[test]
fn the_carried_packages_are_named_by_their_files_and_other_files_are_passed_over() {
	let debs = tempfile::tempdir().unwrap();
	for file in [
		"iwd_3.10-1_arm64.deb",
		"hostapd_2%3a2.11-1_arm64.deb",
		"README",
		"partial.deb",
	] {
		fs::write(debs.path().join(file), "").unwrap();
	}
	assert_eq!(
		carried(debs.path()).unwrap(),
		[
			(
				"hostapd".to_owned(),
				"hostapd_2%3a2.11-1_arm64.deb".to_owned()
			),
			("iwd".to_owned(), "iwd_3.10-1_arm64.deb".to_owned()),
		]
	);
	assert_eq!(
		carried(&debs.path().join("none")).unwrap_err(),
		"the medium carries no packages to install"
	);
}

#[test]
fn each_want_holds_for_its_states() {
	assert!(Want::Disabled.holds("disabled"));
	for state in ["enabled", "masked", NOT_FOUND, "static"] {
		assert!(!Want::Disabled.holds(state), "{state}");
	}
	for state in ["disabled", "masked", NOT_FOUND] {
		assert!(Want::Off.holds(state), "{state}");
	}
	assert!(!Want::Off.holds("enabled"));
	assert!(Want::Masked.holds("masked"));
	assert!(!Want::Masked.holds("disabled"));
}

#[test]
fn a_unit_is_present_where_any_unit_directory_holds_a_file_or_a_mask_for_it() {
	let root = tempfile::tempdir().unwrap();
	assert!(!unit_file_present(root.path(), "iwd.service"));

	let lib = root.path().join("usr/lib/systemd/system");
	fs::create_dir_all(&lib).unwrap();
	fs::write(lib.join("iwd.service"), "[Unit]\n").unwrap();
	assert!(unit_file_present(root.path(), "iwd.service"));

	// A mask is a link to /dev/null, which does not resolve inside the root.
	let etc = root.path().join("etc/systemd/system");
	fs::create_dir_all(&etc).unwrap();
	std::os::unix::fs::symlink("/dev/null", etc.join("hostapd.service")).unwrap();
	assert!(unit_file_present(root.path(), "hostapd.service"));
	assert!(!unit_file_present(root.path(), "wpa_supplicant.service"));
}
