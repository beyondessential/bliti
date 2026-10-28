//! The air and light inside the case, from the sensors on the HAT, as NFO "Air and light" has it.
//!
//! Which sensors are fitted comes from the version the HAT EEPROM records, not from what answers on
//! the bus, so a sensor that stops answering shows as broken rather than disappearing. bliti reads
//! both chips itself: the overlay binds no kernel drivers to them, which leaves provisioning's
//! hardware check free to read them too.

use bliti_core::channel::readings::Entry;

use self::{bme680::Bme680, ltr308::Ltr308};
use super::{i2c, product::Version};

mod bme680;
mod ltr308;

/// The bus the HAT's sensors sit on. The Pi 5 numbers its buses by fixed aliases, so I2C3 is always
/// this node.
const BUS: &str = "/dev/i2c-3";

/// The HAT sensors a device has fitted, each holding its open driver between ticks.
#[derive(Debug, Default)]
pub struct Sensors {
	bme680: Option<Bme680>,
	ltr308: Option<Ltr308>,
}

impl Sensors {
	pub fn new(version: Option<Version>) -> Self {
		Self::fitted(version, BUS)
	}

	// spec: NFO
	fn fitted(version: Option<Version>, bus: &'static str) -> Self {
		match version {
			Some(Version { major: 4, minor: 0 }) => Self {
				bme680: Some(Bme680::new(bus)),
				ltr308: Some(Ltr308::new(bus)),
			},
			_ => Self::default(),
		}
	}

	/// One reading of each thing the fitted sensors measure.
	pub fn readings(&mut self, at: u64) -> Vec<Entry> {
		let mut entries = Vec::new();
		if let Some(bme680) = &mut self.bme680 {
			entries.extend(bme680.readings(at));
		}
		if let Some(ltr308) = &mut self.ltr308 {
			entries.extend(ltr308.readings(at));
		}
		entries
	}
}

/// Why a fitted sensor gave nothing, worded for the entries' reason.
#[derive(Debug, Clone, PartialEq)]
enum Problem {
	/// The bus may not be opened from here: a precondition not met (NFO).
	Skipped(String),
	/// The sensor is fitted and could not be read.
	Broken(String),
}

impl Problem {
	fn bus(chip: &str, bus: &str, err: &i2c::Error) -> Self {
		match err {
			i2c::Error::Denied => Self::Skipped(format!("not permitted to open {bus}")),
			i2c::Error::NoBus => Self::Broken(format!("the {chip} can't be reached: no {bus}")),
			i2c::Error::NoAnswer => Self::Broken(format!("the {chip} does not answer")),
			i2c::Error::Failed(err) => Self::Broken(format!("the {chip} failed: {err}")),
		}
	}

	fn wrong_part(chip: &str, id: u8) -> Self {
		Self::Broken(format!(
			"the {chip}'s address answers as another part, ID 0x{id:02x}"
		))
	}

	fn unfinished(chip: &str) -> Self {
		Self::Broken(format!("the {chip} did not finish a measurement"))
	}

	fn entry(&self, at: u64, name: &str, kind: &str) -> Entry {
		match self {
			Self::Skipped(reason) => Entry::skipped(at, name, kind, reason),
			Self::Broken(reason) => Entry::broken(at, name, kind, reason),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const NO_BUS: &str = "/nonexistent/bliti/i2c-3";

	fn version(major: u16, minor: u16) -> Option<Version> {
		Some(Version { major, minor })
	}

	// spec: NFO
	#[test]
	fn only_a_v4_0_has_the_sensors_fitted() {
		for unfitted in [None, version(3, 0), version(4, 1), version(5, 0)] {
			let mut sensors = Sensors::fitted(unfitted, NO_BUS);
			assert!(sensors.readings(1).is_empty(), "{unfitted:?}");
		}
	}

	/// A fitted sensor that does not answer shows as broken rather than disappearing (NFO).
	// spec: NFO
	#[test]
	fn a_v4_0_whose_sensors_do_not_answer_reports_every_reading_broken() {
		let mut sensors = Sensors::fitted(version(4, 0), NO_BUS);
		for _ in 0..2 {
			let entries = sensors.readings(1);
			let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
			assert_eq!(names, ["temperature", "humidity", "air-pressure", "light"]);
			for entry in &entries {
				assert_eq!(entry.status(), Some("broken"), "{entry:?}");
				assert!(entry.value.is_none());
			}
			assert!(entries[0].reason().unwrap().contains("BME680"));
			assert!(entries[3].reason().unwrap().contains("LTR-308ALS"));
		}
	}

	// spec: NFO
	#[test]
	fn a_denied_bus_is_skipped_and_a_failure_is_broken() {
		let denied = Problem::bus("BME680", BUS, &i2c::Error::Denied);
		assert_eq!(
			denied.entry(1, "humidity", "fraction").status(),
			Some("skipped")
		);

		for err in [i2c::Error::NoBus, i2c::Error::NoAnswer] {
			let problem = Problem::bus("BME680", BUS, &err);
			let entry = problem.entry(1, "humidity", "fraction");
			assert_eq!(entry.status(), Some("broken"), "{err:?}");
			assert!(entry.reason().unwrap().contains("BME680"), "{err:?}");
		}
	}
}
