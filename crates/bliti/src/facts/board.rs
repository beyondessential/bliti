//! What the machine is: its name, its board, and what it runs.
//!
//! The board sources are specific to the Raspberry Pi we ship. Every one falls back to what any
//! machine can answer for, because most of this view is built on a development laptop and a field
//! that only exists on the target hardware is a field nobody sees while building it.

use bliti_core::channel::readings::{Reading, Value};

use super::read_trimmed;

/// The name the system answers to, from the kernel rather than from a configuration file.
pub fn hostname() -> String {
	read_trimmed("/proc/sys/kernel/hostname").unwrap_or_else(|| "unknown".to_owned())
}

/// The board's model and revision, falling back to whatever identity the machine exposes.
///
/// A Pi answers from the device tree and carries a revision code; anything else answers from the
/// firmware's own description of the machine.
pub fn identity() -> Reading {
	let reading = |text: String| Reading::new("board", "Board", Value::text(text));

	if let Some(model) = read_trimmed("/proc/device-tree/model") {
		let reading = reading(model);
		return match revision() {
			Some(revision) => reading.with_detail("Revision", Value::text(revision)),
			None => reading,
		};
	}

	let vendor = read_trimmed("/sys/class/dmi/id/sys_vendor");
	let product = read_trimmed("/sys/class/dmi/id/product_name");
	match (vendor, product) {
		(Some(vendor), Some(product)) => {
			let reading = reading(format!("{vendor} {product}"));
			match read_trimmed("/sys/class/dmi/id/product_version") {
				Some(version) => reading.with_detail("Version", Value::text(version)),
				None => reading,
			}
		}
		(Some(one), None) | (None, Some(one)) => reading(one),
		(None, None) => reading(machine().unwrap_or_else(|| "Unknown".to_owned())),
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

/// The operating system and its version, from the release file every distribution carries.
pub fn os() -> Option<Reading> {
	let raw = std::fs::read_to_string("/etc/os-release").ok()?;
	let pretty = field(&raw, "PRETTY_NAME")
		.or_else(|| field(&raw, "NAME"))
		.unwrap_or_else(|| "Unknown".to_owned());

	let reading = Reading::new("os", "Operating system", Value::text(pretty));
	Some(match read_trimmed("/proc/sys/kernel/osrelease") {
		Some(kernel) => reading.with_detail("Kernel", Value::text(kernel)),
		None => reading,
	})
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

	/// Every machine answers for its board somehow, so the reading is never empty and never missing.
	/// A field that only existed on the target hardware would be invisible while the view is built.
	#[test]
	fn the_board_is_named_on_any_machine() {
		let reading = identity();
		assert!(reading.is_coherent());
		assert!(matches!(&reading.value, Some(Value::Text(text)) if !text.is_empty()));
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
}
