//! Filesystem size and use, one reading per block device (NFO).
//!
//! Counted per device rather than per mount point: a device carrying several mounts would otherwise
//! be reported once for each, and its capacity counted several times over.

use std::{collections::BTreeMap, fs};

use bliti_core::channel::readings::Entry;
use serde_json::Value as Json;

/// One filesystem worth reporting: the block device, its shortest mount, and its usage.
struct Filesystem {
	device: String,
	mount: String,
	used: u64,
	total: u64,
}

impl Filesystem {
	/// The `filesystem` trait: mount and device, and the `boot` role on a boot partition. `device` and
	/// `role` qualify the filesystem, so they sit inside the one trait (NFO).
	fn trait_value(&self) -> Json {
		let mut object = serde_json::Map::new();
		object.insert("mount".to_owned(), Json::String(self.mount.clone()));
		object.insert("device".to_owned(), Json::String(self.device.clone()));
		if is_boot(&self.mount) {
			object.insert("role".to_owned(), Json::String("boot".to_owned()));
		}
		Json::Object(object)
	}
}

/// The size of each filesystem, as a fact in bytes.
pub fn totals(at: u64) -> Vec<Entry> {
	filesystems()
		.into_iter()
		.map(|fs| {
			Entry::quantity(at, "filesystem-total", "bytes", fs.total as f64)
				.with_trait("filesystem", fs.trait_value())
		})
		.collect()
}

/// How full each filesystem is, as a reading.
pub fn usage(at: u64) -> Vec<Entry> {
	filesystems()
		.into_iter()
		.map(|fs| {
			let fraction = (fs.used as f64 / fs.total as f64).clamp(0.0, 1.0);
			Entry::fraction(at, "filesystem-usage", fraction)
				.with_trait("filesystem", fs.trait_value())
		})
		.collect()
}

/// Every filesystem worth reporting, one per block device.
fn filesystems() -> Vec<Filesystem> {
	let mut found = Vec::new();
	for (device, mount) in by_device(&mounts()) {
		let Some((used, total)) = disk_usage(&mount) else {
			continue;
		};
		if total == 0 {
			continue;
		}
		found.push(Filesystem {
			device,
			mount,
			used,
			total,
		});
	}
	found.sort_by(|a, b| a.mount.cmp(&b.mount));
	found
}

/// Whether a mount is one of the boot partitions, which are small and permanently near full.
fn is_boot(mount: &str) -> bool {
	mount == "/boot" || mount.starts_with("/boot/")
}

/// One mount per device: where a device carries several, the shortest path wins, being the one an
/// operator would recognise (NFO).
fn by_device(mounts: &[(String, String)]) -> BTreeMap<String, String> {
	let mut chosen: BTreeMap<String, String> = BTreeMap::new();
	for (device, mount) in mounts {
		chosen
			.entry(device.clone())
			.and_modify(|held| {
				if mount.len() < held.len() {
					*held = mount.clone();
				}
			})
			.or_insert_with(|| mount.clone());
	}
	chosen
}

/// The mounts backed by a real block device, as (device, mount). Virtual filesystems carry no device
/// and are left out (NFO).
fn mounts() -> Vec<(String, String)> {
	let Ok(raw) = fs::read_to_string("/proc/mounts") else {
		return Vec::new();
	};
	raw.lines()
		.filter_map(|line| {
			let mut fields = line.split_whitespace();
			let device = fields.next()?;
			let mount = fields.next()?;
			device
				.starts_with("/dev/")
				.then(|| (device.to_owned(), unescape(mount)))
		})
		.collect()
}

/// `/proc/mounts` escapes spaces and other separators as octal.
fn unescape(point: &str) -> String {
	let mut out = String::with_capacity(point.len());
	let mut chars = point.chars();
	while let Some(c) = chars.next() {
		if c != '\\' {
			out.push(c);
			continue;
		}
		let octal: String = chars.clone().take(3).collect();
		match u8::from_str_radix(&octal, 8) {
			Ok(byte) if octal.len() == 3 => {
				out.push(byte as char);
				for _ in 0..3 {
					chars.next();
				}
			}
			_ => out.push(c),
		}
	}
	out
}

/// Bytes used and total for the filesystem at a path.
fn disk_usage(mount: &str) -> Option<(u64, u64)> {
	let stat = rustix::fs::statvfs(mount).ok()?;
	let block = stat.f_frsize;
	let total = stat.f_blocks.checked_mul(block)?;
	// Available to an unprivileged writer, which is what "free" means to an operator; the difference
	// from f_bfree is the reserve only root can use.
	let free = stat.f_bavail.checked_mul(block)?;
	Some((total.saturating_sub(free), total))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The case measured on the test device: root and the database directory are one block device, so
	/// a naive listing would count the same capacity twice.
	#[test]
	fn several_mounts_on_one_device_are_counted_once() {
		let mounts = vec![
			(
				"/dev/mapper/root".to_owned(),
				"/var/lib/postgresql".to_owned(),
			),
			("/dev/mapper/root".to_owned(), "/".to_owned()),
			("/dev/nvme0n1p2".to_owned(), "/boot".to_owned()),
		];
		let chosen = by_device(&mounts);
		assert_eq!(chosen.len(), 2);
		assert_eq!(chosen["/dev/mapper/root"], "/");
		assert_eq!(chosen["/dev/nvme0n1p2"], "/boot");
	}

	#[test]
	fn an_escaped_mount_point_is_read_back() {
		assert_eq!(unescape("/mnt/my\\040disk"), "/mnt/my disk");
		assert_eq!(unescape("/plain/path"), "/plain/path");
		assert_eq!(unescape("/trailing\\"), "/trailing\\");
	}

	/// Each filesystem is its own reading, carrying its own trait; a boot partition is marked (NFO).
	#[test]
	fn the_root_filesystem_is_a_reading_of_its_own() {
		let readings = usage(1);
		let root = readings
			.iter()
			.find(|e| {
				e.traits
					.get("filesystem")
					.and_then(|f| f.get("mount"))
					.and_then(Json::as_str)
					== Some("/")
			})
			.expect("every machine has a root filesystem");
		let used = root.value.as_ref().and_then(Json::as_f64).unwrap();
		assert!((0.0..=1.0).contains(&used), "{used}");
	}

	#[test]
	fn a_boot_partition_is_marked_with_the_boot_role() {
		assert!(is_boot("/boot"));
		assert!(is_boot("/boot/firmware"));
		assert!(!is_boot("/"));
		assert!(!is_boot("/bootstrap"));

		let fs = Filesystem {
			device: "/dev/mmcblk0p1".to_owned(),
			mount: "/boot/firmware".to_owned(),
			used: 10,
			total: 100,
		};
		assert_eq!(
			fs.trait_value().get("role").and_then(Json::as_str),
			Some("boot")
		);
	}
}
