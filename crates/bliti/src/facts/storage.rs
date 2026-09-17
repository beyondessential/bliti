//! Filesystem use, one reading per block device.
//!
//! Counted per device rather than per mount point: a device carrying several mounts would otherwise
//! be reported once for each, and its capacity counted several times over.

use std::{collections::BTreeMap, fs};

use bliti_core::channel::readings::{Reading, Value};

use super::compute::bytes;

/// One mount worth reporting, and the device behind it.
struct Mount {
	device: String,
	point: String,
}

/// How full the device is: the fullest block device, with each of them behind it.
///
/// One number, because the question at a glance is whether anything is about to run out, and that is
/// answered by whichever is closest to doing so.
pub fn disks() -> Option<Reading> {
	let mut found: Vec<(String, String, u64, u64)> = Vec::new();
	for (device, point) in by_device(&mounts()) {
		let Some((used, total)) = usage(&point) else {
			continue;
		};
		if total == 0 {
			continue;
		}
		found.push((point, device, used, total));
	}
	if found.is_empty() {
		return None;
	}
	found.sort_by(|a, b| a.0.cmp(&b.0));

	// The boot partitions are small, written once at imaging, and sit near full for the life of the
	// device. Letting one set the headline would show every device as nearly out of space.
	let fullest = found
		.iter()
		.filter(|(point, ..)| !is_boot(point))
		.map(|(_, _, used, total)| *used as f64 / *total as f64)
		.fold(f64::NAN, f64::max);
	let headline = if fullest.is_nan() {
		// Nothing but boot partitions, which is not a machine we ship, but reporting nothing at all
		// would be worse than reporting what there is.
		found
			.iter()
			.map(|(_, _, used, total)| *used as f64 / *total as f64)
			.fold(0.0, f64::max)
	} else {
		fullest
	};

	let mut reading = Reading::new("disk", "Disk", Value::Fraction(headline.clamp(0.0, 1.0)))
		// A filesystem does not move fast enough for a graph to say anything, so the reveal keeps its
		// space for the figures instead.
		.ungraphed();
	for (point, device, used, total) in &found {
		reading = reading
			.with_detail(
				point,
				Value::Fraction((*used as f64 / *total as f64).clamp(0.0, 1.0)),
			)
			.with_detail("  free", bytes(total.saturating_sub(*used)))
			.with_detail("  of", bytes(*total))
			.with_detail("  on", Value::text(device));
	}
	Some(reading)
}

/// Whether a mount is one of the boot partitions, which are small and permanently near full.
fn is_boot(point: &str) -> bool {
	point == "/boot" || point.starts_with("/boot/")
}

/// One mount per device: where a device carries several, the shortest path wins, being the one an
/// operator would recognise.
fn by_device(mounts: &[Mount]) -> BTreeMap<String, String> {
	let mut chosen: BTreeMap<String, String> = BTreeMap::new();
	for mount in mounts {
		chosen
			.entry(mount.device.clone())
			.and_modify(|held| {
				if mount.point.len() < held.len() {
					*held = mount.point.clone();
				}
			})
			.or_insert_with(|| mount.point.clone());
	}
	chosen
}

/// The mounts backed by a real block device. Virtual filesystems carry no device and are left out.
fn mounts() -> Vec<Mount> {
	let Ok(raw) = fs::read_to_string("/proc/mounts") else {
		return Vec::new();
	};
	raw.lines()
		.filter_map(|line| {
			let mut fields = line.split_whitespace();
			let device = fields.next()?;
			let point = fields.next()?;
			device.starts_with("/dev/").then(|| Mount {
				device: device.to_owned(),
				point: unescape(point),
			})
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
fn usage(point: &str) -> Option<(u64, u64)> {
	let stat = rustix::fs::statvfs(point).ok()?;
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
			Mount {
				device: "/dev/mapper/root".to_owned(),
				point: "/var/lib/postgresql".to_owned(),
			},
			Mount {
				device: "/dev/mapper/root".to_owned(),
				point: "/".to_owned(),
			},
			Mount {
				device: "/dev/nvme0n1p2".to_owned(),
				point: "/boot".to_owned(),
			},
		];
		let chosen = by_device(&mounts);
		assert_eq!(chosen.len(), 2);
		// The shortest path wins, being the one an operator would recognise.
		assert_eq!(chosen["/dev/mapper/root"], "/");
		assert_eq!(chosen["/dev/nvme0n1p2"], "/boot");
	}

	#[test]
	fn an_escaped_mount_point_is_read_back() {
		assert_eq!(unescape("/mnt/my\\040disk"), "/mnt/my disk");
		assert_eq!(unescape("/plain/path"), "/plain/path");
		assert_eq!(unescape("/trailing\\"), "/trailing\\");
	}

	#[test]
	fn the_root_filesystem_is_reported() {
		let reading = disks().expect("every machine has a root filesystem");
		assert!(reading.is_coherent());
		assert!(
			!reading.graph,
			"a filesystem does not move fast enough to draw"
		);
		let Some(Value::Fraction(used)) = reading.value else {
			panic!("disk use is a fraction")
		};
		assert!((0.0..=1.0).contains(&used), "{used}");
		assert!(
			!reading.detail.is_empty(),
			"every filesystem is behind the headline"
		);
	}

	/// Boot partitions are small, written once at imaging, and sit near full forever. One setting the
	/// headline would show every device we ship as nearly out of space.
	#[test]
	fn a_boot_partition_is_not_the_headline() {
		assert!(is_boot("/boot"));
		assert!(is_boot("/boot/firmware"));
		assert!(!is_boot("/"));
		assert!(!is_boot("/bootstrap"));
		assert!(!is_boot("/var/lib/postgresql"));
	}
}
