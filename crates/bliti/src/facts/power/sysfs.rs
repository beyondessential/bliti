//! The batteries the kernel reports, read from `/sys/class/power_supply`.
//!
//! The fallback for a machine with no upower: a headless board with a cell and no daemon still says
//! how full it is. It cannot stand in for upower generally, because a USB UPS has no sysfs power
//! supply to find under any filter.

use std::{fs, path::Path};

use super::battery::{Battery, Direction, worthwhile};

const POWER_SUPPLY: &str = "/sys/class/power_supply";

/// Every battery the kernel reports as powering this machine.
pub fn batteries() -> Vec<Battery> {
	batteries_in(Path::new(POWER_SUPPLY))
}

/// Whether a supply is a battery powering the machine.
///
/// `scope` is what marks a peripheral's cell: a wireless mouse reports `Device`, while the machine's
/// own battery reports `System` or says nothing at all (NFO).
fn powers_the_device(kind: &str, scope: Option<&str>) -> bool {
	kind == "Battery" && scope != Some("Device")
}

/// The cell's direction of travel, as the kernel words it.
fn direction(status: Option<&str>) -> Direction {
	match status {
		Some("Charging") => Direction::Charging,
		Some("Discharging") => Direction::Discharging,
		Some("Full" | "Not charging") => Direction::Idle,
		_ => Direction::Unknown,
	}
}

fn batteries_in(root: &Path) -> Vec<Battery> {
	let Ok(entries) = fs::read_dir(root) else {
		return Vec::new();
	};
	let mut supplies: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
	// A stable order, so the same battery is reported in the same place on every sample.
	supplies.sort();

	supplies
		.iter()
		.filter_map(|supply| from_directory(supply))
		.collect()
}

/// One power supply as a battery, or nothing where it is not one that powers the machine.
fn from_directory(supply: &Path) -> Option<Battery> {
	let read = |name: &str| read_trimmed(&supply.join(name));
	if !powers_the_device(read("type")?.as_str(), read("scope").as_deref()) {
		return None;
	}
	// A bay with no cell in it is hardware that is not fitted (NFO).
	if read("present").as_deref() == Some("0") {
		return None;
	}

	let os_name = supply.file_name()?.to_str()?.to_owned();
	let mut battery = Battery::new(os_name);
	battery.vendor = worthwhile(read("manufacturer"));
	battery.model = worthwhile(read("model_name"));
	battery.serial = worthwhile(read("serial_number"));
	battery.charge = charge(supply);
	// Microvolts, and an unreported voltage is written as zero.
	battery.volts = number(supply, "voltage_now")
		.map(|micro| micro / 1_000_000.0)
		.filter(|volts| *volts > 0.0);
	battery.direction = direction(read("status").as_deref());
	Some(battery)
}

/// State of charge, from whichever pair of counters this battery keeps. `capacity` is already the
/// percentage; the others are a level against a full one.
fn charge(supply: &Path) -> Option<f64> {
	if let Some(percent) = number(supply, "capacity") {
		return Some(percent / 100.0);
	}
	for (now, full) in [("energy_now", "energy_full"), ("charge_now", "charge_full")] {
		if let (Some(now), Some(full)) = (number(supply, now), number(supply, full))
			&& full > 0.0
		{
			return Some(now / full);
		}
	}
	None
}

fn number(supply: &Path, name: &str) -> Option<f64> {
	read_trimmed(&supply.join(name))?.parse().ok()
}

fn read_trimmed(path: &Path) -> Option<String> {
	let raw = fs::read_to_string(path).ok()?;
	let text = raw.trim();
	(!text.is_empty()).then(|| text.to_owned())
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;

	use super::*;

	/// A power supply directory as the kernel lays one out.
	fn supply(root: &Path, name: &str, files: BTreeMap<&str, &str>) {
		let dir = root.join(name);
		fs::create_dir_all(&dir).unwrap();
		for (file, contents) in files {
			fs::write(dir.join(file), contents).unwrap();
		}
	}

	fn tree(name: &str) -> std::path::PathBuf {
		let root = std::env::temp_dir().join(format!("bliti-power-{name}-{}", std::process::id()));
		let _ = fs::remove_dir_all(&root);
		fs::create_dir_all(&root).unwrap();
		root
	}

	/// This laptop's own layout: mains, an internal cell, a wireless mouse, and a USB-C source.
	fn this_laptop(root: &Path) {
		supply(
			root,
			"AC",
			BTreeMap::from([("type", "Mains"), ("online", "1")]),
		);
		supply(
			root,
			"BAT0",
			BTreeMap::from([
				("type", "Battery"),
				("status", "Full"),
				("capacity", "100"),
				("voltage_now", "12887000"),
				("manufacturer", "LGC-LGC3.67"),
				("model_name", "DELL T453X"),
				("serial_number", "109"),
				("present", "1"),
			]),
		);
		supply(
			root,
			"hidpp_battery_0",
			BTreeMap::from([
				("type", "Battery"),
				("status", "Discharging"),
				("scope", "Device"),
				("model_name", "MX Vertical Advanced Ergonomic Mouse"),
			]),
		);
		supply(
			root,
			"ucsi-source-psy-USBC000:001",
			BTreeMap::from([
				("type", "USB"),
				("scope", "System"),
				("voltage_now", "5000000"),
			]),
		);
	}

	/// The machine's own cell is reported, and nothing else on this laptop is (NFO).
	#[test]
	fn only_the_machines_own_cell_is_reported() {
		let root = tree("own-cell");
		this_laptop(&root);
		let found = batteries_in(&root);
		assert_eq!(found.len(), 1, "{found:?}");
		let battery = &found[0];
		assert_eq!(battery.os_name, "BAT0");
		assert_eq!(battery.model.as_deref(), Some("DELL T453X"));
		assert_eq!(battery.serial.as_deref(), Some("109"));
		assert_eq!(battery.charge, Some(1.0));
		assert_eq!(battery.volts, Some(12.887));
		assert_eq!(battery.direction, Direction::Idle);
		fs::remove_dir_all(&root).unwrap();
	}

	/// A peripheral's cell is told by its scope, which is the only marker sysfs gives (NFO).
	#[test]
	fn a_peripheral_is_told_by_its_scope() {
		assert!(powers_the_device("Battery", None));
		assert!(powers_the_device("Battery", Some("System")));
		assert!(!powers_the_device("Battery", Some("Device")));
		assert!(!powers_the_device("Mains", None));
		assert!(!powers_the_device("USB", Some("System")));
	}

	/// The UPS this laptop has attached is not here to be found, whatever the filter (NFO).
	#[test]
	fn a_usb_ups_is_not_in_sysfs_to_find() {
		let root = tree("no-ups");
		this_laptop(&root);
		let found = batteries_in(&root);
		assert!(
			!found
				.iter()
				.any(|battery| battery.os_name.contains("hiddev")),
			"a HID UPS has no sysfs power supply, which is why upower is the source"
		);
		fs::remove_dir_all(&root).unwrap();
	}

	/// Not every battery keeps a `capacity`, so the level is taken against a full one.
	#[test]
	fn charge_falls_back_to_the_counters() {
		let root = tree("counters");
		supply(
			&root,
			"BAT0",
			BTreeMap::from([
				("type", "Battery"),
				("status", "Discharging"),
				("energy_now", "52000000"),
				("energy_full", "104000000"),
			]),
		);
		let found = batteries_in(&root);
		assert_eq!(found[0].charge, Some(0.5));
		assert_eq!(found[0].direction, Direction::Discharging);
		assert_eq!(found[0].volts, None, "no voltage reported is no voltage");
		fs::remove_dir_all(&root).unwrap();
	}

	#[test]
	fn the_statuses_map_onto_the_three_directions() {
		assert_eq!(direction(Some("Charging")), Direction::Charging);
		assert_eq!(direction(Some("Discharging")), Direction::Discharging);
		assert_eq!(direction(Some("Full")), Direction::Idle);
		assert_eq!(direction(Some("Not charging")), Direction::Idle);
		assert_eq!(direction(Some("Unknown")), Direction::Unknown);
		assert_eq!(direction(None), Direction::Unknown);
	}

	/// A machine with no power supplies at all reports no battery (NFO).
	#[test]
	fn a_machine_with_no_supplies_reports_none() {
		assert!(batteries_in(Path::new("/nonexistent/power_supply")).is_empty());
	}
}
