//! The machine's own storage, reached through the kernel's files and the tools the medium carries.

use std::{
	fs::{self, File},
	io::{self, Read, Seek, SeekFrom},
	path::{Path, PathBuf},
	process::Command,
	sync::Mutex,
	thread::sleep,
	time::Duration,
};

use super::{Mounted, Payload, ROOT, Storage, partition};
use crate::provision::{
	bootloader::PAYLOAD,
	exec::{RUN, failure, finish, run},
	manifest::Manifest,
};

/// The name of the dm-verity mapping the payload is read through.
const VERITY: &str = "bliti-payload";

/// The block size the payload's hash tree was made with.
const BLOCK: u64 = 4096;

/// How long the nodes of a disk's partitions are waited for after the table is read.
const NODES: Duration = Duration::from_secs(30);

/// The real thing.
pub struct Machine;

impl Storage for Machine {
	fn nvme(&self) -> Result<Vec<PathBuf>, String> {
		let mut disks = Vec::new();
		let entries =
			fs::read_dir("/sys/block").map_err(|err| format!("cannot list the disks: {err}"))?;
		for entry in entries.flatten() {
			let name = entry.file_name().to_string_lossy().into_owned();
			if is_namespace(&name) {
				disks.push(Path::new("/dev").join(name));
			}
		}
		disks.sort();
		Ok(disks)
	}

	fn size(&self, disk: &Path) -> Result<u64, String> {
		File::open(disk)
			.and_then(|mut file| file.seek(SeekFrom::End(0)))
			.map_err(|err| format!("cannot size {}: {err}", disk.display()))
	}

	fn blank(&self, disk: &Path) -> Result<bool, String> {
		let mut command = Command::new("blkid");
		command.arg("--probe").arg(disk);
		let output = finish(&mut command)?;
		// blkid exits 2 where it finds nothing to say about the device.
		match output.status.code() {
			Some(0) => Ok(false),
			Some(2) => Ok(true),
			_ => Err(failure(&command, &output)),
		}
	}

	fn labels(&self, disk: &Path) -> Result<Vec<String>, String> {
		let output = run(Command::new("lsblk")
			.args(["--noheadings", "--output", "PARTLABEL"])
			.arg(disk))?;
		Ok(String::from_utf8_lossy(&output.stdout)
			.lines()
			.map(str::trim)
			.filter(|label| !label.is_empty())
			.map(str::to_owned)
			.collect())
	}

	fn rescan(&self, disk: &Path) -> Result<(), String> {
		// The kernel refuses while a partition is in use, which a moment's wait for udev to let go of
		// a node it has just probed usually cures.
		let mut result = Err(String::new());
		for _ in 0..10 {
			result = run(Command::new("blockdev").arg("--rereadpt").arg(disk)).map(drop);
			if result.is_ok() {
				break;
			}
			sleep(Duration::from_secs(1));
		}
		result?;
		run(Command::new("udevadm").args(["settle", "--timeout=30"])).map(drop)?;
		wait_for(&partition(disk, ROOT))
	}

	fn mount(&self, device: &Path, fstype: &str, options: &str) -> Result<Mounted, String> {
		fs::create_dir_all(RUN).map_err(|err| format!("cannot make {RUN}: {err}"))?;
		let dir = tempfile::Builder::new()
			.prefix("mnt-")
			.tempdir_in(RUN)
			.map_err(|err| format!("cannot make a mount point: {err}"))?;
		run(Command::new("mount")
			.args(["-t", fstype, "-o", options])
			.arg(device)
			.arg(dir.path()))?;
		let path = dir.path().to_owned();
		Ok(Mounted::new(path.clone(), move || {
			if let Err(err) = run(Command::new("umount").arg(&path)) {
				tracing::warn!(%err, path = %path.display(), "cannot unmount");
			}
			drop(dir);
		}))
	}

	fn payload(&self, manifest: &Manifest) -> Result<Box<dyn Payload>, String> {
		let node = fs::canonicalize(PAYLOAD)
			.map_err(|err| format!("cannot find the payload partition: {err}"))?;
		// A mapping left by a run that did not close it, which a stale one would refuse.
		let _ = run(Command::new("veritysetup").args(["close", VERITY]));
		run(Command::new("veritysetup")
			.arg("open")
			.arg(format!("--hash-offset={}", manifest.hash_offset))
			.arg(format!("--data-blocks={}", manifest.size.div_ceil(BLOCK)))
			.arg(&node)
			.arg(VERITY)
			.arg(&node)
			.arg(&manifest.root_hash))
		.map_err(|err| format!("cannot open the payload through dm-verity: {err}"))?;
		let mapper = Path::new("/dev/mapper").join(VERITY);
		let file = match File::open(&mapper) {
			Ok(file) => file,
			Err(err) => {
				close_verity();
				return Err(format!("cannot read the payload: {err}"));
			}
		};
		Ok(Box::new(Verity {
			file: file.take(manifest.size),
			mapper,
			loop_device: Mutex::new(None),
		}))
	}
}

/// Whether `name` is an NVMe namespace, as `nvme0n1` is, and not a partition of one.
fn is_namespace(name: &str) -> bool {
	let Some(rest) = name.strip_prefix("nvme") else {
		return false;
	};
	let Some((controller, namespace)) = rest.split_once('n') else {
		return false;
	};
	let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
	digits(controller) && digits(namespace)
}

fn wait_for(node: &Path) -> Result<(), String> {
	let mut waited = Duration::ZERO;
	while !node.exists() {
		if waited >= NODES {
			return Err(format!("{} did not appear", node.display()));
		}
		sleep(Duration::from_millis(200));
		waited += Duration::from_millis(200);
	}
	Ok(())
}

fn close_verity() {
	if let Err(err) = run(Command::new("veritysetup").args(["close", VERITY])) {
		tracing::warn!(%err, "cannot close the payload");
	}
}

/// The payload read through its dm-verity mapping.
struct Verity {
	file: io::Take<File>,
	mapper: PathBuf,
	/// The loop device the payload's partitions are read through, once they have been.
	loop_device: Mutex<Option<PathBuf>>,
}

impl Read for Verity {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		self.file.read(buf)
	}
}

impl Payload for Verity {
	fn root(&self) -> Result<PathBuf, String> {
		let mut held = self
			.loop_device
			.lock()
			.expect("nothing panics holding the lock");
		let device = match &*held {
			Some(device) => device.clone(),
			None => {
				let output = run(Command::new("losetup")
					.args(["--find", "--show", "--read-only", "--partscan"])
					.arg(&self.mapper))?;
				let device = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
				*held = Some(device.clone());
				device
			}
		};
		run(Command::new("udevadm").args(["settle", "--timeout=30"]))?;
		let node = partition(&device, ROOT);
		wait_for(&node)?;
		Ok(node)
	}
}

impl Drop for Verity {
	fn drop(&mut self) {
		let held = self.loop_device.get_mut().ok().and_then(Option::take);
		if let Some(device) = held
			&& let Err(err) = run(Command::new("losetup").arg("-d").arg(&device))
		{
			tracing::warn!(%err, device = %device.display(), "cannot detach a loop device");
		}
		close_verity();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_namespace_is_not_a_partition_or_another_disk() {
		assert!(is_namespace("nvme0n1"));
		assert!(is_namespace("nvme12n3"));
		assert!(!is_namespace("nvme0n1p1"));
		assert!(!is_namespace("nvme0"));
		assert!(!is_namespace("mmcblk0"));
		assert!(!is_namespace("nvmen1"));
	}
}
