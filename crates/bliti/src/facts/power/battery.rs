//! One battery as a source described it, and the readings that come of it.
//!
//! Both operating-system sources produce these, so the wire shape is decided in one place: the
//! naming rules, the `battery` trait, and which readings are skipped all live here, and neither
//! reader knows anything about entries.

use bliti_core::channel::readings::{Entry, kind};
use serde_json::{Map, Value as Json};

/// What the cell is doing, as the source reported it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
	Charging,
	Discharging,
	Idle,
	/// The source reports the battery but cannot say which of the three holds (NFO).
	Unknown,
}

impl Direction {
	fn as_str(self) -> Option<&'static str> {
		match self {
			Self::Charging => Some("charging"),
			Self::Discharging => Some("discharging"),
			Self::Idle => Some("idle"),
			Self::Unknown => None,
		}
	}
}

/// One battery powering the device, before it becomes readings.
#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
	/// What the operating system knows it as: an object-path basename or a sysfs directory name.
	/// Unique across the batteries one source reports, and the name used where the model cannot be.
	pub os_name: String,
	pub vendor: Option<String>,
	pub model: Option<String>,
	pub serial: Option<String>,
	/// State of charge, from 0 to 1. Absent where the source reports the battery but no charge.
	pub charge: Option<f64>,
	/// Cell voltage. Absent where the source reports none, which a UPS commonly does.
	pub volts: Option<f64>,
	pub direction: Direction,
}

impl Battery {
	/// A battery known only by the name its source gives it.
	pub fn new(os_name: impl Into<String>) -> Self {
		Self {
			os_name: os_name.into(),
			vendor: None,
			model: None,
			serial: None,
			charge: None,
			volts: None,
			direction: Direction::Unknown,
		}
	}
}

/// Vendors write these where they have nothing to say, so they are not serials (NFO asks for the
/// serial where the device *holds* one).
const PLACEHOLDERS: &[&str] = &[
	"blank",
	"unknown",
	"none",
	"n/a",
	"invalid",
	"not available",
];

/// Whether a value a source gave is worth carrying.
fn meaningful(value: &str) -> bool {
	let trimmed = value.trim();
	!trimmed.is_empty() && !PLACEHOLDERS.contains(&trimmed.to_lowercase().as_str())
}

/// Keep a source's string where it says something.
pub fn worthwhile(value: Option<String>) -> Option<String> {
	value.filter(|value| meaningful(value))
}

/// The name each battery is reported under: the model where it has one that no other battery shares,
/// and the name the operating system knows it as otherwise (NFO).
///
/// Resolved across the whole set rather than per battery, because whether a model can name a battery
/// depends on what else is fitted: two UPSes of one model would be told apart by nothing.
fn names(batteries: &[Battery]) -> Vec<String> {
	let mut chosen: Vec<String> = batteries
		.iter()
		.map(|battery| match &battery.model {
			Some(model) if meaningful(model) => {
				let shared = batteries
					.iter()
					.filter(|other| other.model.as_deref() == Some(model.as_str()))
					.count();
				if shared > 1 {
					battery.os_name.clone()
				} else {
					model.clone()
				}
			}
			_ => battery.os_name.clone(),
		})
		.collect();

	// A model can still collide with another battery's operating-system name. The operating-system
	// names are unique among themselves, so falling back to them settles any collision that remains.
	for index in 0..chosen.len() {
		if chosen.iter().filter(|name| **name == chosen[index]).count() > 1 {
			chosen[index] = batteries[index].os_name.clone();
		}
	}
	chosen
}

/// The `battery` trait: the name that distinguishes it, and the serial, model and vendor that
/// describe the cell (NFO).
fn battery_trait(battery: &Battery, name: &str) -> Json {
	let mut object = Map::new();
	object.insert("name".to_owned(), Json::String(name.to_owned()));
	for (member, held) in [
		("serial", &battery.serial),
		("model", &battery.model),
		("vendor", &battery.vendor),
	] {
		if let Some(value) = held {
			object.insert(member.to_owned(), Json::String(value.clone()));
		}
	}
	Json::Object(object)
}

/// The three readings for every battery given, each carrying its `battery` trait (NFO).
pub fn entries(at: u64, batteries: &[Battery]) -> Vec<Entry> {
	let names = names(batteries);
	let mut entries = Vec::new();
	for (battery, name) in batteries.iter().zip(&names) {
		let about = battery_trait(battery, name);
		entries.push(charge(at, battery).with_trait("battery", about.clone()));
		entries.push(voltage(at, battery).with_trait("battery", about.clone()));
		entries.push(direction(at, battery).with_trait("battery", about));
	}
	entries
}

fn charge(at: u64, battery: &Battery) -> Entry {
	match battery.charge {
		Some(charge) => Entry::fraction(at, "battery-charge", charge.clamp(0.0, 1.0)),
		// The battery is there and the source could not say how full it is, which is a fault rather
		// than a measurement this platform cannot make (NFO).
		None => Entry::broken(
			at,
			"battery-charge",
			kind::FRACTION,
			"the battery reports no state of charge",
		),
	}
}

fn voltage(at: u64, battery: &Battery) -> Entry {
	match battery.volts {
		Some(volts) => Entry::quantity(at, "battery-voltage", "volts", round(volts, 3)),
		None => Entry::skipped(
			at,
			"battery-voltage",
			kind::QUANTITY,
			"the battery reports no cell voltage",
		),
	}
}

fn direction(at: u64, battery: &Battery) -> Entry {
	match battery.direction.as_str() {
		Some(value) => Entry::text(at, "battery-direction", value),
		None => Entry::skipped(
			at,
			"battery-direction",
			kind::TEXT,
			"the operating system does not say which way the cell is going",
		),
	}
}

fn round(value: f64, places: i32) -> f64 {
	let scale = 10f64.powi(places);
	(value * scale).round() / scale
}

#[cfg(test)]
mod tests {
	use super::*;

	fn named(os_name: &str, model: Option<&str>) -> Battery {
		let mut battery = Battery::new(os_name);
		battery.model = model.map(ToOwned::to_owned);
		battery.charge = Some(1.0);
		battery
	}

	fn about(entry: &Entry) -> &Json {
		entry.traits.get("battery").unwrap()
	}

	fn name_of(entry: &Entry) -> &str {
		about(entry).get("name").unwrap().as_str().unwrap()
	}

	/// An operating system's battery is named by its model (NFO).
	#[test]
	fn a_battery_is_named_by_its_model() {
		let batteries = vec![
			named("BAT0", Some("DELL T453X")),
			named("hiddev5", Some("Eaton 3S")),
		];
		assert_eq!(names(&batteries), ["DELL T453X", "Eaton 3S"]);
	}

	/// With no model there is nothing to name it but what the operating system knows it as (NFO).
	#[test]
	fn a_battery_with_no_model_is_named_by_the_system() {
		let batteries = vec![named("BAT0", None), named("hiddev5", Some(""))];
		assert_eq!(names(&batteries), ["BAT0", "hiddev5"]);
	}

	/// Two of one model must still be told apart, so neither takes the model (NFO).
	#[test]
	fn batteries_sharing_a_model_are_told_apart() {
		let batteries = vec![
			named("hiddev5", Some("Eaton 3S")),
			named("hiddev6", Some("Eaton 3S")),
		];
		assert_eq!(names(&batteries), ["hiddev5", "hiddev6"]);
	}

	/// A model colliding with another battery's system name is settled the same way.
	#[test]
	fn a_model_colliding_with_a_system_name_falls_back() {
		let batteries = vec![named("BAT0", Some("hiddev5")), named("hiddev5", None)];
		assert_eq!(names(&batteries), ["BAT0", "hiddev5"]);
	}

	/// Every battery carries its own trait, so a reader can tell the readings apart (NFO).
	#[test]
	fn each_battery_carries_its_own_trait() {
		let batteries = vec![
			named("BAT0", Some("DELL T453X")),
			named("hiddev5", Some("Eaton 3S")),
		];
		let entries = entries(1, &batteries);
		assert_eq!(entries.len(), 6);
		for entry in &entries {
			assert!(entry.traits.contains_key("battery"), "{entry:?}");
		}
		let charges: Vec<_> = entries
			.iter()
			.filter(|entry| entry.name == "battery-charge")
			.map(name_of)
			.collect();
		assert_eq!(charges, ["DELL T453X", "Eaton 3S"]);
	}

	/// A UPS reports how full it is and no voltage at all, which is a skip and not a fault (NFO).
	#[test]
	fn no_voltage_is_skipped_while_the_charge_stands() {
		let mut ups = named("hiddev5", Some("Eaton 3S"));
		ups.charge = Some(1.0);
		ups.volts = None;
		ups.direction = Direction::Idle;
		let entries = entries(1, &[ups]);
		let voltage = entries
			.iter()
			.find(|e| e.name == "battery-voltage")
			.unwrap();
		assert_eq!(voltage.status(), Some("skipped"));
		assert!(voltage.reason().is_some());
		let charge = entries.iter().find(|e| e.name == "battery-charge").unwrap();
		assert_eq!(charge.status(), Some("passed"));
	}

	/// A source that reports the battery but not its direction skips rather than guessing (NFO).
	#[test]
	fn an_unknown_direction_is_skipped() {
		let battery = named("BAT0", Some("DELL T453X"));
		let entries = entries(1, &[battery]);
		let direction = entries
			.iter()
			.find(|e| e.name == "battery-direction")
			.unwrap();
		assert_eq!(direction.status(), Some("skipped"));
	}

	/// Placeholder serials say nothing, so they are not carried (NFO asks for what the device holds).
	#[test]
	fn a_placeholder_serial_is_not_carried() {
		let mut battery = named("hiddev5", Some("Eaton 3S"));
		battery.serial = worthwhile(Some("Blank".to_owned()));
		battery.vendor = worthwhile(Some("Eaton".to_owned()));
		let entries = entries(1, &[battery]);
		let about = about(&entries[0]);
		assert!(about.get("serial").is_none(), "{about:?}");
		assert_eq!(about.get("vendor").unwrap().as_str(), Some("Eaton"));
	}

	#[test]
	fn an_empty_or_unknown_value_is_not_carried() {
		assert_eq!(worthwhile(Some("  ".to_owned())), None);
		assert_eq!(worthwhile(Some("unknown".to_owned())), None);
		assert_eq!(worthwhile(Some("109".to_owned())), Some("109".to_owned()));
	}

	/// A battery that is there and cannot say how full it is has broken, rather than being a
	/// measurement this platform cannot make (NFO).
	#[test]
	fn a_battery_with_no_charge_is_broken() {
		let mut battery = named("BAT0", Some("DELL T453X"));
		battery.charge = None;
		let entries = entries(1, &[battery]);
		let charge = entries.iter().find(|e| e.name == "battery-charge").unwrap();
		assert_eq!(charge.status(), Some("broken"));
		assert!(charge.value.is_none());
	}

	#[test]
	fn no_batteries_is_no_entries() {
		assert!(entries(1, &[]).is_empty());
	}
}
