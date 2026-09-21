//! Temperature and cooling.
//!
//! The processor temperature is drawn against the board's own declared trip points rather than an
//! invented scale, and it is `warning` or `failed` only where the board is in difficulty rather than
//! merely warm (NFO). A device running hot and working is not a device in trouble, and colouring
//! warmth as failure teaches an operator to distrust a healthy reading.
//!
//! Throttling no longer lives here: it is reported as `cpu-frequency` going to `warning` where the
//! platform reports it (see [`super::compute`]). The undervoltage alarm is gone until W1.

use std::fs;

use bliti_core::channel::readings::{Entry, kind};
use serde_json::Value as Json;

use super::{read_number, read_trimmed};

/// The processor's thermal zone, and the hwmon tree the other sensors and the fan sit in.
const THERMAL_ZONE: &str = "/sys/class/thermal/thermal_zone0";
const HWMON_ROOT: &str = "/sys/class/hwmon";

/// One `temperature` reading per sensor. The processor core is the `cpu` sensor and carries the
/// board's thresholds; every other hwmon that reports a temperature is a sensor of its own (NFO).
pub fn temperature(at: u64) -> Vec<Entry> {
	let mut entries = Vec::new();
	entries.extend(cpu_temperature(at, THERMAL_ZONE));
	entries.extend(other_sensors(at, HWMON_ROOT));
	entries
}

/// Fan speed, which says whether a hot device is hot because its cooling has stopped.
pub fn fan(at: u64) -> Vec<Entry> {
	fan_in(at, HWMON_ROOT)
}

/// The processor core temperature, against the board's own thresholds.
fn cpu_temperature(at: u64, zone: &str) -> Option<Entry> {
	// The zone being present is the sensor being fitted. One that is there and will not answer is a
	// fault nobody can see from outside the case, so it is reported as broken rather than left out.
	if fs::metadata(zone).is_err() {
		return None;
	}
	let Some(millidegrees) = read_number(&format!("{zone}/temp")) else {
		return Some(
			Entry::broken(
				at,
				"temperature",
				kind::QUANTITY,
				format!("no answer from {zone}/temp"),
			)
			.with_trait("sensor", Json::String("cpu".to_owned())),
		);
	};
	let celsius = millidegrees as f64 / 1000.0;

	let critical = trips(zone)
		.iter()
		.find(|(kind, _)| kind == "critical")
		.map(|(_, at)| *at);

	let mut entry = Entry::quantity(at, "temperature", "celsius", celsius)
		.with_trait("sensor", Json::String("cpu".to_owned()));

	for (kind, mark) in trips(zone) {
		if kind == "critical" {
			entry = entry.with_limit(mark, "Critical");
		}
	}
	// The hottest active trip is where the board itself starts working to cool down.
	if let Some(hottest) = trips(zone)
		.iter()
		.filter(|(kind, _)| kind == "active")
		.map(|(_, at)| *at)
		.fold(None, |held: Option<f64>, at| {
			Some(held.map_or(at, |held| held.max(at)))
		}) {
		entry = entry.with_limit(hottest, "Cooling");
	}

	// Trouble only where the board is in difficulty. Warmth on its own is not a fault.
	if let Some(critical) = critical {
		if celsius >= critical {
			entry = entry.failed(format!(
				"the processor is at {celsius:.1} °C, its critical limit"
			));
		}
	}
	Some(entry)
}

/// Every other hwmon that reports a temperature, as a sensor of its own. The processor core is left
/// to the thermal zone above.
fn other_sensors(at: u64, root: &str) -> Vec<Entry> {
	let Ok(entries) = fs::read_dir(root) else {
		return Vec::new();
	};
	let mut readings = Vec::new();
	for entry in entries.flatten() {
		let path = entry.path();
		let Some(name) = read_trimmed(&format!("{}/name", path.display())) else {
			continue;
		};
		if name == "cpu_thermal" {
			continue;
		}
		let Some(millidegrees) = read_number(&format!("{}/temp1_input", path.display())) else {
			continue;
		};
		readings.push(
			Entry::quantity(at, "temperature", "celsius", millidegrees as f64 / 1000.0)
				.with_trait("sensor", Json::String(name)),
		);
	}
	readings.sort_by(|a, b| {
		a.traits
			.get("sensor")
			.and_then(Json::as_str)
			.cmp(&b.traits.get("sensor").and_then(Json::as_str))
	});
	readings
}

/// The thermal zone's declared trip points, as kind and temperature.
fn trips(zone: &str) -> Vec<(String, f64)> {
	let Ok(entries) = fs::read_dir(zone) else {
		return Vec::new();
	};
	let mut found = Vec::new();
	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		let Some(index) = name
			.strip_prefix("trip_point_")
			.and_then(|rest| rest.strip_suffix("_type"))
		else {
			continue;
		};
		let Some(kind) = read_trimmed(&format!("{zone}/{name}")) else {
			continue;
		};
		let Some(at) = read_number(&format!("{zone}/trip_point_{index}_temp")) else {
			continue;
		};
		found.push((kind, at as f64 / 1000.0));
	}
	found
}

/// Fan speed under a given hwmon root, so a scratch tree can stand in for sysfs.
fn fan_in(at: u64, root: &str) -> Vec<Entry> {
	for index in 0..8 {
		let path = format!("{root}/hwmon{index}");
		if read_trimmed(&format!("{path}/name")).as_deref() != Some("pwmfan") {
			continue;
		}
		// The hwmon is the fan being fitted. One that is there and will not answer is a fault rather
		// than an absence, so it is reported as broken rather than left out (NFO).
		let entry = match read_number(&format!("{path}/fan1_input")) {
			Some(rpm) => {
				let entry = Entry::quantity(at, "fan-speed", "revolutions/minute", rpm as f64)
					.with_trait("fan", Json::String("cpu".to_owned()));
				if rpm == 0 {
					entry.warning("the fan has stopped")
				} else {
					entry
				}
			}
			None => Entry::broken(
				at,
				"fan-speed",
				kind::QUANTITY,
				format!("no answer from {path}/fan1_input"),
			)
			.with_trait("fan", Json::String("cpu".to_owned())),
		};
		return vec![entry];
	}
	Vec::new()
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Warm is not a fault. A device in the seventies is working correctly, and marking it as trouble
	/// is what teaches an operator to distrust the number (NFO).
	#[test]
	fn a_warm_board_is_not_reported_as_trouble() {
		let Some(cpu) = cpu_temperature(1, THERMAL_ZONE) else {
			return; // No thermal zone on this machine.
		};
		let Some(celsius) = cpu.value.as_ref().and_then(Json::as_f64) else {
			return; // Broken, which carries no value.
		};
		if celsius < 100.0 {
			assert_eq!(cpu.status(), Some("passed"), "{celsius} °C is warm at most");
		}
	}

	/// A scratch directory standing in for a sysfs tree, cleaned up on drop.
	struct Tree(std::path::PathBuf);

	impl Tree {
		fn new() -> Self {
			use std::sync::atomic::{AtomicU32, Ordering};
			static COUNTER: AtomicU32 = AtomicU32::new(0);
			let n = COUNTER.fetch_add(1, Ordering::Relaxed);
			let path =
				std::env::temp_dir().join(format!("bliti-thermal-{}-{n}", std::process::id()));
			fs::create_dir_all(&path).unwrap();
			Self(path)
		}

		fn at(&self, name: &str) -> String {
			self.0.join(name).to_string_lossy().into_owned()
		}

		fn write(&self, name: &str, contents: &str) -> &Self {
			fs::create_dir_all(self.0.join(name).parent().unwrap()).unwrap();
			fs::write(self.0.join(name), contents).unwrap();
			self
		}
	}

	impl Drop for Tree {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	/// Not fitted and fitted-but-silent are different things: the first is left out, the second is
	/// reported broken with its reason (NFO).
	#[test]
	fn a_zone_absent_and_one_that_will_not_answer_are_different() {
		let tree = Tree::new();
		assert!(cpu_temperature(1, &tree.at("nonexistent")).is_none());

		let zone = tree.at("zone");
		fs::create_dir_all(&zone).unwrap();
		let entry = cpu_temperature(1, &zone).expect("a present zone is reported");
		assert_eq!(entry.status(), Some("broken"));
		assert!(entry.value.is_none());
		assert!(entry.reason().is_some_and(|why| why.contains("temp")));
	}

	/// The ceiling is the board's own critical trip, never one invented: a zone declaring none leaves
	/// the reading with no scale (NFO).
	#[test]
	fn a_zone_declaring_a_critical_trip_carries_it_as_a_limit() {
		let tree = Tree::new();
		let zone = tree.at("zone");
		tree.write("zone/temp", "48500\n");
		let entry = cpu_temperature(1, &zone).expect("fitted and answering");
		assert_eq!(entry.value.as_ref().and_then(Json::as_f64), Some(48.5));
		assert!(
			entry.traits.get("limits").is_none(),
			"no trips declared, no limits"
		);

		tree.write("zone/trip_point_0_type", "critical\n");
		tree.write("zone/trip_point_0_temp", "85000\n");
		let entry = cpu_temperature(1, &zone).expect("fitted and answering");
		let limits = entry.traits.get("limits").and_then(Json::as_array).unwrap();
		assert_eq!(limits[0].get("at").and_then(Json::as_f64), Some(85.0));
	}

	#[test]
	fn a_fan_absent_and_one_that_will_not_answer_are_different() {
		let tree = Tree::new();
		let root = tree.at("hwmon");
		fs::create_dir_all(&root).unwrap();
		assert!(fan_in(1, &root).is_empty(), "no pwmfan, nothing reported");

		tree.write("hwmon/hwmon0/name", "pwmfan\n");
		let broken = &fan_in(1, &root)[0];
		assert_eq!(broken.status(), Some("broken"));
		assert!(broken.value.is_none());

		tree.write("hwmon/hwmon0/fan1_input", "2400\n");
		let ok = &fan_in(1, &root)[0];
		assert_eq!(ok.status(), Some("passed"));
		assert_eq!(ok.value.as_ref().and_then(Json::as_f64), Some(2400.0));
	}
}
