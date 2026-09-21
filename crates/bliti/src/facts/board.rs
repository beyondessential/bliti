//! What the machine is: its name, its board, what it runs, and when it booted.
//!
//! The board sources are specific to the Raspberry Pi we ship. Every one falls back to what any
//! machine can answer for, because most of this view is built on a development laptop and a field
//! that only exists on the target hardware is a field nobody sees while building it.

use std::time::{SystemTime, UNIX_EPOCH};

use bliti_core::channel::readings::Entry;

use super::{read_trimmed, uptime};

/// The name the system answers to, from the kernel rather than from a configuration file.
pub fn hostname() -> String {
	read_trimmed("/proc/sys/kernel/hostname").unwrap_or_else(|| "unknown".to_owned())
}

/// The board's model, and its revision as a second fact where the board carries one.
///
/// A Pi answers from the device tree and carries a revision code; anything else answers from the
/// firmware's own description of the machine.
pub fn identity(at: u64) -> Vec<Entry> {
	if let Some(model) = read_trimmed("/proc/device-tree/model") {
		let mut entries = vec![Entry::text(at, "board", model)];
		if let Some(revision) = revision() {
			entries.push(Entry::text(at, "board-revision", revision));
		}
		return entries;
	}

	let vendor = read_trimmed("/sys/class/dmi/id/sys_vendor");
	let product = read_trimmed("/sys/class/dmi/id/product_name");
	match (vendor, product) {
		(Some(vendor), Some(product)) => {
			let mut entries = vec![Entry::text(at, "board", format!("{vendor} {product}"))];
			if let Some(version) = read_trimmed("/sys/class/dmi/id/product_version") {
				entries.push(Entry::text(at, "board-revision", version));
			}
			entries
		}
		(Some(one), None) | (None, Some(one)) => vec![Entry::text(at, "board", one)],
		(None, None) => vec![Entry::text(
			at,
			"board",
			machine().unwrap_or_else(|| "Unknown".to_owned()),
		)],
	}
}

/// The Pi's revision code, from the line `/proc/cpuinfo` carries for it.
fn revision() -> Option<String> {
	let raw = std::fs::read_to_string("/proc/cpuinfo").ok()?;
	raw.lines()
		.find_map(|line| line.strip_prefix("Revision"))
		.and_then(|rest| rest.split(':').nth(1))
		.map(|value| value.trim().to_owned())
		.filter(|value| !value.is_empty())
}

/// The architecture, as a last resort where the machine describes itself no other way.
fn machine() -> Option<String> {
	read_trimmed("/proc/sys/kernel/arch")
		.or_else(|| read_trimmed("/sys/firmware/devicetree/base/compatible"))
}

/// The operating system and the kernel, as two facts.
pub fn os(at: u64) -> Vec<Entry> {
	let mut entries = Vec::new();
	if let Ok(raw) = std::fs::read_to_string("/etc/os-release") {
		let pretty = field(&raw, "PRETTY_NAME")
			.or_else(|| field(&raw, "NAME"))
			.unwrap_or_else(|| "Unknown".to_owned());
		entries.push(Entry::text(at, "os", pretty));
	}
	if let Some(kernel) = read_trimmed("/proc/sys/kernel/osrelease") {
		entries.push(Entry::text(at, "kernel", kernel));
	}
	entries
}

/// The instant the device booted, as RFC 3339 in UTC.
///
/// Worked out from the wall clock less uptime, so it needs a clock that is set. A device in the field
/// may have none, in which case it cannot answer for the instant and the fact is omitted (NFO).
pub fn last_boot(at: u64) -> Option<Entry> {
	let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
	// A clock that has not been set sits near the epoch. There is no honest boot instant to report
	// from it, so the fact is omitted rather than reported as some time in 1970.
	if now < CLOCK_SET_THRESHOLD {
		return None;
	}
	let booted = now.saturating_sub(uptime()?.as_secs());
	Some(Entry::datetime(at, "last-boot", rfc3339_utc(booted)))
}

/// Wall times below this are a clock that was never set (2021-01-01 UTC).
const CLOCK_SET_THRESHOLD: u64 = 1_609_459_200;

/// Format seconds since the Unix epoch as RFC 3339 in UTC.
///
/// A civil-time conversion by the days-from-civil algorithm, so no calendar dependency is pulled in
/// for the one datetime this protocol carries.
fn rfc3339_utc(secs: u64) -> String {
	let days = (secs / 86_400) as i64;
	let rem = secs % 86_400;
	let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);

	// Howard Hinnant's civil_from_days, with the epoch shifted so the era arithmetic stays positive.
	let z = days + 719_468;
	let era = z.div_euclid(146_097);
	let doe = z.rem_euclid(146_097);
	let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
	let year = yoe + era * 400;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let day = doy - (153 * mp + 2) / 5 + 1;
	let month = if mp < 10 { mp + 3 } else { mp - 9 };
	let year = if month <= 2 { year + 1 } else { year };

	format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// One field of a release file, unquoted.
fn field(raw: &str, name: &str) -> Option<String> {
	raw.lines()
		.find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
		.map(|value| value.trim().trim_matches('"').to_owned())
		.filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_hostname_comes_from_the_kernel() {
		let name = hostname();
		assert!(!name.is_empty());
		assert!(!name.contains('\n'));
	}

	/// Every machine answers for its board somehow, so the board fact is never empty and never missing.
	#[test]
	fn the_board_is_named_on_any_machine() {
		let entries = identity(1);
		let board = entries.iter().find(|e| e.name == "board").expect("a board");
		assert_eq!(board.status(), Some("passed"));
		assert!(board.value.is_some());
	}

	#[test]
	fn a_release_file_is_read_without_its_quotes() {
		let raw = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 26.04 LTS\"\nVERSION_ID=\"26.04\"\n";
		assert_eq!(
			field(raw, "PRETTY_NAME").as_deref(),
			Some("Ubuntu 26.04 LTS")
		);
		assert_eq!(field(raw, "NAME").as_deref(), Some("Ubuntu"));
		assert_eq!(field(raw, "MISSING"), None);
	}

	/// A prefix match must not let `VERSION_ID` answer for `VERSION`.
	#[test]
	fn a_field_name_matches_whole_and_not_as_a_prefix() {
		let raw = "VERSION_ID=\"26.04\"\n";
		assert_eq!(field(raw, "VERSION"), None);
	}

	/// The civil conversion against instants whose UTC rendering is known, including a leap day.
	#[test]
	fn the_epoch_formats_as_rfc_3339_in_utc() {
		assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
		assert_eq!(rfc3339_utc(1_609_459_200), "2021-01-01T00:00:00Z");
		assert_eq!(rfc3339_utc(1_582_934_400), "2020-02-29T00:00:00Z");
		assert_eq!(rfc3339_utc(1_700_000_000), "2023-11-14T22:13:20Z");
	}
}
