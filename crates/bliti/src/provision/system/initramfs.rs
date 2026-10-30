//! Making the system on the disk unlock its root volume from the TPM at boot (PENC, "What the step
//! leaves").

use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};

use super::{read, replace};
use crate::provision::exec::run;

/// What `crypttab` says of the root volume where it unlocks from the TPM: the volume by its
/// partition label, which the first boot's rotation of identifiers leaves alone.
const CRYPTTAB: &str = "# <name> <device>                    <keyfile>  <options>\n\
	root     /dev/disk/by-partlabel/root none       force,luks,discard,tpm2-device=auto,token-timeout=30\n";

/// The dracut configuration that embeds `crypttab` in the initramfs, and no keyfile, as the image
/// ships it but for the empty keyfile.
const DRACUT_CRYPTTAB: &str = "# In generic (non-hostonly) mode, the crypt module's install() doesn't\n\
	# auto-embed /etc/crypttab, because that branch is gated on hostonly. Force it\n\
	# in so systemd-cryptsetup-generator at boot has something to act on.\n\
	install_items+=\" /etc/crypttab \"\n";

/// The dracut configuration that adds what unlocking from the TPM takes.
const DRACUT_TPM: &str =
	"# Unlock the root volume from the TPM.\nadd_dracutmodules+=\" tpm2-tss \"\n";

/// The files, under the root, the step writes.
const CRYPTTAB_FILE: &str = "etc/crypttab";
const DRACUT_KEYFILE_CONF: &str = "etc/dracut.conf.d/02-luks-keyfile.conf";
const DRACUT_TPM_CONF: &str = "etc/dracut.conf.d/06-bliti-tpm.conf";

/// The kernels the system has modules for, under the root.
const MODULES: &str = "usr/lib/modules";

/// The initramfs of a system on disk, made by the system's own tools.
pub trait Initramfs: Send + Sync {
	/// Whether the system at `root` can make an initramfs that unlocks from the TPM.
	fn can_unlock_from_tpm(&self, root: &Path) -> bool;

	/// Make the initramfs for the kernel `kernel` of the system at `root`, and give where it is.
	fn make(&self, root: &Path, kernel: &str) -> Result<PathBuf, String>;
}

/// Whether the system at `root`, whose firmware partition is mounted at `firmware`, can be made to
/// unlock from the TPM, and the one kernel it has.
pub fn check(root: &Path, firmware: &Path, initramfs: &dyn Initramfs) -> Result<String, String> {
	if !initramfs.can_unlock_from_tpm(root) {
		return Err(
			"the system on the disk cannot make an initramfs that unlocks from the TPM".to_owned(),
		);
	}
	if !firmware.join("current").is_dir() {
		return Err("the disk's firmware partition has no current boot slot".to_owned());
	}
	kernel(root)
}

/// Make the system at `root`, whose firmware partition is mounted at `firmware`, unlock its root
/// volume from the TPM keyslot at boot, with the empty passphrase's keyslot still in place.
///
/// Rewrites its `crypttab` and the dracut configuration that embeds it, makes a new initramfs, and
/// puts it in the current boot slot. Every write leaves a system that still unlocks by the empty
/// passphrase, so an interruption anywhere leaves one the next try finishes.
// spec: PENC#what-the-step-leaves
pub fn rewire(root: &Path, firmware: &Path, initramfs: &dyn Initramfs) -> Result<(), String> {
	let kernel = check(root, firmware, initramfs)?;

	let write = |file: &str, content: &str| {
		replace(&root.join(file), content.as_bytes())
			.map_err(|err| format!("cannot write /{file}: {err}"))
	};
	write(CRYPTTAB_FILE, CRYPTTAB)?;
	write(DRACUT_KEYFILE_CONF, DRACUT_CRYPTTAB)?;
	write(DRACUT_TPM_CONF, DRACUT_TPM)?;

	let made = initramfs.make(root, &kernel)?;
	let image = read(&made)
		.map_err(|err| format!("cannot read the initramfs that was made: {err}"))?
		.filter(|image| !image.is_empty())
		.ok_or("the initramfs that was made is empty")?;
	let _ = fs::remove_file(&made);
	replace(&firmware.join("current/initrd.img"), &image)
		.map_err(|err| format!("cannot install the initramfs: {err}"))?;

	// A slot staged by an update, which would be booted as a trial and rolled back to this one.
	let staged = firmware.join("new");
	if staged.exists() {
		fs::remove_dir_all(&staged)
			.map_err(|err| format!("cannot remove the staged boot slot: {err}"))?;
	}
	Ok(())
}

/// The one kernel the system at `root` has modules for.
fn kernel(root: &Path) -> Result<String, String> {
	let mut kernels: Vec<String> = fs::read_dir(root.join(MODULES))
		.map_err(|err| format!("cannot list the system's kernels: {err}"))?
		.flatten()
		.filter(|entry| entry.path().is_dir())
		.map(|entry| entry.file_name().to_string_lossy().into_owned())
		.collect();
	match kernels.len() {
		0 => Err("the system on the disk has no kernel".to_owned()),
		1 => Ok(kernels.remove(0)),
		_ => Err("the system on the disk has more than one kernel".to_owned()),
	}
}

/// dracut, run inside the system on the disk.
pub struct Dracut;

impl Initramfs for Dracut {
	fn can_unlock_from_tpm(&self, root: &Path) -> bool {
		fs::read_dir(root.join("usr/lib/dracut/modules.d"))
			.into_iter()
			.flatten()
			.flatten()
			.any(|module| module.file_name().to_string_lossy().ends_with("tpm2-tss"))
	}

	fn make(&self, root: &Path, kernel: &str) -> Result<PathBuf, String> {
		const OUT: &str = "var/tmp/bliti-initrd.img";
		let mut binds = Vec::new();
		for dir in ["dev", "proc", "sys"] {
			let target = root.join(dir);
			run(Command::new("mount")
				.args(["--bind", "--make-private"])
				.arg(Path::new("/").join(dir))
				.arg(&target))?;
			binds.push(target);
		}
		let _unbind = Unbind(binds);
		run(Command::new("chroot")
			.arg(root)
			.args(["dracut", "--force", "--kver", kernel])
			.arg(Path::new("/").join(OUT)))?;
		Ok(root.join(OUT))
	}
}

/// Unmounts the bind mounts, last first, when dropped.
struct Unbind(Vec<PathBuf>);

impl Drop for Unbind {
	fn drop(&mut self) {
		for target in self.0.iter().rev() {
			if let Err(err) = run(Command::new("umount").arg("--lazy").arg(target)) {
				tracing::warn!(%err, path = %target.display(), "cannot unmount");
			}
		}
	}
}

#[cfg(test)]
mod tests;
