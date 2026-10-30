use super::*;
use crate::provision::fake_system::Bench;

/// A system ready to be rewired: a root with a kernel and a keyfile crypttab, and a firmware
/// partition with a current boot slot.
fn bench() -> Bench {
	Bench::new()
}

fn go(bench: &Bench) -> Result<(), String> {
	rewire(&bench.root, &bench.firmware, &*bench.initramfs())
}

fn crypttab(bench: &Bench) -> String {
	fs::read_to_string(bench.root.join(CRYPTTAB_FILE)).unwrap()
}

// spec: PENC#what-the-step-leaves
#[test]
fn the_system_is_made_to_unlock_from_the_tpm() {
	let bench = bench();
	go(&bench).unwrap();

	assert!(crypttab(&bench).contains("tpm2-device=auto"));
	assert!(!crypttab(&bench).contains("empty-keyfile"));
	let embedded = fs::read_to_string(bench.root.join(DRACUT_KEYFILE_CONF)).unwrap();
	assert!(embedded.contains("/etc/crypttab"));
	assert!(!embedded.contains("empty-keyfile"));
	let tpm = fs::read_to_string(bench.root.join(DRACUT_TPM_CONF)).unwrap();
	assert!(tpm.contains("tpm2-tss"));

	let initrd = fs::read_to_string(bench.firmware.join("current/initrd.img")).unwrap();
	assert_eq!(
		initrd,
		"initramfs for 6.17.0-test; unlocks from the TPM: true"
	);
	assert_eq!(bench.state.lock().unwrap().initramfs_made, ["6.17.0-test"]);
}

// spec: PENC#what-the-step-leaves
#[test]
fn the_current_slots_other_files_and_overlays_are_kept_and_nothing_is_left_over() {
	let bench = bench();
	fs::write(
		bench.firmware.join("current/overlays/tamanu-iti-v4.dtbo"),
		"overlay",
	)
	.unwrap();
	fs::write(bench.firmware.join("current/vmlinuz"), "kernel").unwrap();
	go(&bench).unwrap();
	assert_eq!(
		fs::read(bench.firmware.join("current/overlays/tamanu-iti-v4.dtbo")).unwrap(),
		b"overlay"
	);
	assert_eq!(
		fs::read(bench.firmware.join("current/vmlinuz")).unwrap(),
		b"kernel"
	);
	assert!(!bench.root.join("initrd.new").exists());
	let litter = fs::read_dir(bench.firmware.join("current"))
		.unwrap()
		.flatten()
		.filter(|entry| entry.file_name().to_string_lossy().starts_with(".bliti"))
		.count();
	assert_eq!(litter, 0);
}

// spec: PENC#what-the-step-leaves
#[test]
fn a_staged_boot_slot_is_removed_so_that_no_rollback_unlocks_by_the_empty_passphrase() {
	let bench = bench();
	fs::create_dir_all(bench.firmware.join("new/overlays")).unwrap();
	fs::write(bench.firmware.join("new/initrd.img"), "staged").unwrap();
	go(&bench).unwrap();
	assert!(!bench.firmware.join("new").exists());
}

// spec: PENC#what-the-step-leaves
#[test]
fn doing_it_again_gives_the_same_system() {
	let bench = bench();
	go(&bench).unwrap();
	let once = (
		crypttab(&bench),
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
	);
	go(&bench).unwrap();
	assert_eq!(
		once,
		(
			crypttab(&bench),
			fs::read(bench.firmware.join("current/initrd.img")).unwrap()
		)
	);
}

// spec: PENC#carrying-out-the-step
#[test]
fn a_system_that_cannot_unlock_from_the_tpm_is_left_exactly_as_it_was() {
	let bench = bench();
	bench.state.lock().unwrap().can_unlock_from_tpm = false;
	let before = crypttab(&bench);
	let err = go(&bench).unwrap_err();
	assert!(
		err.contains("cannot make an initramfs that unlocks from the TPM"),
		"{err}"
	);
	assert_eq!(crypttab(&bench), before);
	assert_eq!(
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
		b"old initramfs"
	);
}

// spec: PENC#carrying-out-the-step
#[test]
fn a_disk_with_no_current_boot_slot_is_refused() {
	let bench = bench();
	fs::remove_dir_all(bench.firmware.join("current")).unwrap();
	assert!(go(&bench).unwrap_err().contains("no current boot slot"));
}

// spec: PENC#carrying-out-the-step
#[test]
fn a_system_with_no_kernel_or_two_is_refused() {
	let bench = bench();
	fs::remove_dir_all(bench.root.join(MODULES).join("6.17.0-test")).unwrap();
	assert!(go(&bench).unwrap_err().contains("no kernel"));
	fs::create_dir_all(bench.root.join(MODULES).join("6.17.0-a")).unwrap();
	fs::create_dir_all(bench.root.join(MODULES).join("6.17.0-b")).unwrap();
	assert!(go(&bench).unwrap_err().contains("more than one kernel"));
}

// spec: PENC#what-the-step-leaves
#[test]
fn an_initramfs_that_fails_to_be_made_leaves_the_current_slot_as_it_was() {
	let bench = bench();
	bench.state.lock().unwrap().budget = Some(0);
	assert!(go(&bench).is_err());
	assert_eq!(
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
		b"old initramfs"
	);
}

// spec: PENC#what-the-step-leaves
#[test]
fn an_empty_initramfs_is_not_installed() {
	struct Empty;
	impl Initramfs for Empty {
		fn can_unlock_from_tpm(&self, _: &Path) -> bool {
			true
		}
		fn make(&self, root: &Path, _: &str) -> Result<PathBuf, String> {
			let out = root.join("initrd.new");
			fs::write(&out, "").unwrap();
			Ok(out)
		}
	}
	let bench = bench();
	let err = rewire(&bench.root, &bench.firmware, &Empty).unwrap_err();
	assert!(err.contains("empty"), "{err}");
	assert_eq!(
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
		b"old initramfs"
	);
}
