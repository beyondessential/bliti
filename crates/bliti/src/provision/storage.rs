//! The board's disk and the payload on the provisioning medium, as the steps that write the system
//! reach them (PDSK).
//!
//! The steps work through [`Storage`], so that they are tested on files and directories. [`Machine`]
//! is the real one.

use std::{
	io::Read,
	path::{Path, PathBuf},
};

use crate::provision::manifest::Manifest;

pub use self::machine::Machine;

mod machine;

/// The partition of the `pi` image that holds the firmware files and the boot slots.
pub const FIRMWARE: u32 = 1;

/// The partition of the `pi` image that holds the encrypted root volume.
pub const ROOT: u32 = 3;

/// The machine's storage, each call blocking and saying why it failed.
pub trait Storage: Send + Sync {
	/// The board's NVMe drives, as device nodes.
	fn nvme(&self) -> Result<Vec<PathBuf>, String>;

	/// The size of `disk` in bytes.
	fn size(&self, disk: &Path) -> Result<u64, String>;

	/// Whether `disk` holds no partition table and no filesystem signature.
	fn blank(&self, disk: &Path) -> Result<bool, String>;

	/// The labels of the partitions of `disk`.
	fn labels(&self, disk: &Path) -> Result<Vec<String>, String>;

	/// Have the kernel read `disk`'s partition table again, and wait for the nodes of its partitions.
	fn rescan(&self, disk: &Path) -> Result<(), String>;

	/// Mount `device` as `fstype` with `options`, at a directory of its own, until the [`Mounted`] is
	/// dropped.
	fn mount(&self, device: &Path, fstype: &str, options: &str) -> Result<Mounted, String>;

	/// The payload, opened read-only through dm-verity against the manifest's root hash.
	fn payload(&self, manifest: &Manifest) -> Result<Box<dyn Payload>, String>;
}

/// The payload as the medium holds it, read through dm-verity, closed when dropped.
pub trait Payload: Read + Send {
	/// The partition of the payload's root volume, as a device node.
	fn root(&self) -> Result<PathBuf, String>;
}

/// A filesystem mounted for as long as this is held.
pub struct Mounted {
	path: PathBuf,
	unmount: Option<Box<dyn FnOnce() + Send>>,
}

impl Mounted {
	/// A mount at `path` that `unmount` undoes.
	pub fn new(path: PathBuf, unmount: impl FnOnce() + Send + 'static) -> Self {
		Self {
			path,
			unmount: Some(Box::new(unmount)),
		}
	}

	/// Where the filesystem is.
	pub fn path(&self) -> &Path {
		&self.path
	}
}

impl Drop for Mounted {
	fn drop(&mut self) {
		if let Some(unmount) = self.unmount.take() {
			unmount();
		}
	}
}

/// The device node of partition `number` of `disk`: `p` goes between a name that ends in a digit and
/// the number.
pub fn partition(disk: &Path, number: u32) -> PathBuf {
	let name = disk.as_os_str().to_string_lossy();
	let sep = if name.ends_with(|c: char| c.is_ascii_digit()) {
		"p"
	} else {
		""
	};
	PathBuf::from(format!("{name}{sep}{number}"))
}

/// The one NVMe drive the board has (PDSK, "The disk").
pub fn single_disk(storage: &dyn Storage) -> Result<PathBuf, String> {
	let mut disks = storage.nvme()?;
	match disks.len() {
		0 => Err("the board has no NVMe drive".to_owned()),
		1 => Ok(disks.remove(0)),
		_ => Err("the board has more than one NVMe drive".to_owned()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_name_that_ends_in_a_digit_takes_a_p() {
		assert_eq!(
			partition(Path::new("/dev/nvme0n1"), 3),
			Path::new("/dev/nvme0n1p3")
		);
		assert_eq!(partition(Path::new("/dev/sda"), 1), Path::new("/dev/sda1"));
		assert_eq!(
			partition(Path::new("/dev/loop7"), 3),
			Path::new("/dev/loop7p3")
		);
	}
}
