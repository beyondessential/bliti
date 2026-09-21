//! Temperature, throttling, and cooling.
//!
//! The temperature reading is drawn against the board's own declared trip points rather than an
//! invented scale, and it is marked as trouble only where the board is in difficulty rather than
//! merely warm. A device running hot and working is not a device in trouble, and colouring warmth as
//! failure teaches an operator to distrust a healthy reading.
//!
//! Throttling is reported as the conditions the platform can establish. The Pi firmware's throttle
//! bitmask is not among them: reaching it needs a tool that is not installed on the image we ship,
//! and there is no sysfs node for it. What is reachable is the undervoltage alarm and the processor
//! running below the speed it is capable of, which between them cover the faults an operator in the
//! field is looking for.

use std::fs;

use bliti_core::channel::readings::{Reading, State, Value};

use super::{read_number, read_trimmed};

/// The processor's thermal zone, and the hwmon tree the fan and the voltage alarm sit in.
const THERMAL_ZONE: &str = "/sys/class/thermal/thermal_zone0";
const HWMON_ROOT: &str = "/sys/class/hwmon";

/// What the processor core reads, plus any throttling and cooling the board reports.
pub fn readings() -> Vec<Reading> {
	let mut readings = Vec::new();
	readings.extend(temperature(THERMAL_ZONE));
	readings.extend(throttling());
	readings.extend(fan(HWMON_ROOT));
	readings
}

/// The processor core temperature, against the board's own thresholds.
fn temperature(zone: &str) -> Option<Reading> {
	// The zone being present is the sensor being fitted. One that is there and will not answer is a
	// fault nobody can see from outside the case, so it is reported rather than left out (NFO).
	if fs::metadata(zone).is_err() {
		return None;
	}
	let Some(millidegrees) = read_number(&format!("{zone}/temp")) else {
		return Some(Reading::failed(
			"temperature",
			"Temperature",
			format_args!("no answer from {zone}/temp"),
		));
	};
	let celsius = millidegrees as f64 / 1000.0;

	let critical = trips(zone)
		.iter()
		.find(|(kind, _)| kind == "critical")
		.map(|(_, at)| *at);

	let mut reading = Reading::new(
		"temperature",
		"Temperature",
		// Drawn against what the board declares hot, never against a ceiling we invented: a board
		// naming no critical trip gives the reading no scale, so it carries none (NFO).
		match critical {
			Some(critical) => Value::scaled(round(celsius), "°C", critical),
			None => Value::quantity(round(celsius), "°C"),
		},
	)
	.with_note(
		"This is the CPU core, not the case or the room. \
		 Above 70 °C is normal under load, and the board slows itself down well before anything is \
		 at risk.",
	);

	for (kind, at) in trips(zone) {
		if kind == "critical" {
			reading = reading.with_limit(at, "Critical");
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
		reading = reading.with_limit(hottest, "Cooling");
	}

	// Trouble only where the board is in difficulty. Warmth on its own is not a fault.
	if let Some(critical) = critical {
		if celsius >= critical {
			reading = reading.with_state(State::Fault);
		}
	}

	for (name, label) in [("hwmon2", "Disk"), ("hwmon3", "Board")] {
		if let Some(other) = hwmon_temperature(name) {
			reading = reading.with_detail(label, Value::quantity(round(other), "°C"));
		}
	}

	Some(reading)
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

/// A temperature from a named hwmon, where that hwmon exists and reports one.
fn hwmon_temperature(hwmon: &str) -> Option<f64> {
	let path = format!("/sys/class/hwmon/{hwmon}");
	// Only report a sensor that is not the processor core, which is already the headline.
	let name = read_trimmed(&format!("{path}/name"))?;
	if name == "cpu_thermal" {
		return None;
	}
	Some(read_number(&format!("{path}/temp1_input"))? as f64 / 1000.0)
}

/// What is currently limiting the board, if anything.
fn throttling() -> Option<Reading> {
	let undervolted = undervoltage_alarm();
	let capped = frequency_cap();

	// A board with neither source reports nothing rather than claiming all is well.
	if undervolted.is_none() && capped.is_none() {
		return None;
	}

	let undervolted = undervolted.unwrap_or(false);
	let limited = capped.is_some_and(|(current, max)| current < max);

	let summary = match (undervolted, limited) {
		(true, true) => "Undervolted and slowed",
		(true, false) => "Undervolted",
		(false, true) => "Slowed",
		(false, false) => "None",
	};

	let mut reading = Reading::new("throttling", "Throttling", Value::text(summary));
	if undervolted || limited {
		reading = reading.with_state(State::Warn);
	}
	if let Some((current, max)) = capped {
		reading = reading
			.with_detail("Speed", gigahertz(current))
			.with_detail("Full speed", gigahertz(max));
	}
	Some(reading)
}

/// Whether the supply voltage has dropped below what the board needs.
fn undervoltage_alarm() -> Option<bool> {
	for index in 0..8 {
		let path = format!("/sys/class/hwmon/hwmon{index}");
		if read_trimmed(&format!("{path}/name")).as_deref() == Some("rpi_volt") {
			return Some(read_number(&format!("{path}/in0_lcrit_alarm"))? != 0);
		}
	}
	None
}

/// The processor's current and maximum speed, in kilohertz.
fn frequency_cap() -> Option<(i64, i64)> {
	let base = "/sys/devices/system/cpu/cpu0/cpufreq";
	let current = read_number(&format!("{base}/scaling_cur_freq"))?;
	let max = read_number(&format!("{base}/cpuinfo_max_freq"))?;
	Some((current, max))
}

fn gigahertz(kilohertz: i64) -> Value {
	Value::quantity(round(kilohertz as f64 / 1_000_000.0), "GHz")
}

/// Fan speed, which says whether a hot device is hot because its cooling has stopped.
fn fan(root: &str) -> Option<Reading> {
	for index in 0..8 {
		let path = format!("{root}/hwmon{index}");
		if read_trimmed(&format!("{path}/name")).as_deref() != Some("pwmfan") {
			continue;
		}
		// The hwmon is the fan being fitted. One that is there and will not answer is a fault rather
		// than an absence, so it is reported rather than left out (NFO).
		let Some(rpm) = read_number(&format!("{path}/fan1_input")) else {
			return Some(Reading::failed(
				"fan",
				"Fan",
				format_args!("no answer from {path}/fan1_input"),
			));
		};
		let mut reading = Reading::new("fan", "Fan", Value::quantity(rpm as f64, "rpm"));
		if rpm == 0 {
			reading = reading.with_state(State::Warn);
		}
		return Some(reading);
	}
	None
}

fn round(value: f64) -> f64 {
	(value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The reading that prompted the note: warm is not a fault. A device in the seventies is working
	/// correctly, and marking it as trouble is what teaches an operator to distrust the number.
	#[test]
	fn a_warm_board_is_not_reported_as_trouble() {
		let Some(reading) = temperature(THERMAL_ZONE) else {
			return; // No thermal zone on this machine.
		};
		let Some(Value::Quantity { number, .. }) = reading.value else {
			panic!("temperature is a quantity")
		};
		if number < 100.0 {
			assert!(
				!reading.state.is_trouble(),
				"{number} °C is warm at most, not trouble"
			);
		}
	}

	/// Without it the number is routinely read as a fault on a device that is working correctly.
	#[test]
	fn the_temperature_carries_its_note() {
		let Some(reading) = temperature(THERMAL_ZONE) else {
			return;
		};
		if reading.error.is_some() {
			return; // a sensor that did not answer carries its reason instead
		}
		let note = reading.note.expect("temperature explains itself");
		assert!(note.contains("CPU core"), "{note}");
		assert!(note.contains("70"), "{note}");
	}

	#[test]
	fn the_temperature_is_drawn_against_a_declared_ceiling_or_against_none() {
		let Some(reading) = temperature(THERMAL_ZONE) else {
			return;
		};
		let Some(value) = reading.value.as_ref() else {
			return;
		};
		let declared = trips(THERMAL_ZONE)
			.iter()
			.find(|(kind, _)| kind == "critical")
			.map(|(_, at)| *at);

		assert_eq!(
			value.has_scale(),
			declared.is_some(),
			"a scale exists exactly where the board declared one"
		);
		if let (Value::Quantity { max: Some(max), .. }, Some(critical)) = (value, declared) {
			assert_eq!(
				*max, critical,
				"the ceiling is the board's own critical trip"
			);
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
			fs::write(self.0.join(name), contents).unwrap();
			self
		}
	}

	impl Drop for Tree {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	/// Hardware that is not fitted is left out entirely; hardware that is fitted and will not answer
	/// is reported as failing. An operator can see the first from where they stand and cannot see the
	/// second at all, so conflating them hides a real fault (NFO).
	#[test]
	fn a_thermal_zone_that_is_absent_and_one_that_will_not_answer_are_different_things() {
		let tree = Tree::new();

		// Not fitted: no zone at all.
		assert!(temperature(&tree.at("nonexistent")).is_none());

		// Fitted and silent: the zone is there, `temp` is not.
		let reading = temperature(&tree.at(".")).expect("a zone that is present is reported");
		assert!(reading.value.is_none());
		assert_eq!(reading.state, State::Fault);
		assert!(
			reading
				.error
				.as_deref()
				.is_some_and(|why| why.contains("temp")),
			"{reading:?}"
		);
		assert!(reading.is_coherent());
	}

	#[test]
	fn a_fan_that_is_absent_and_one_that_will_not_answer_are_different_things() {
		let tree = Tree::new();
		let root = tree.at(".");

		// Not fitted: no hwmon names itself pwmfan.
		assert!(fan(&root).is_none());

		// Fitted and silent: the hwmon is there, `fan1_input` is not.
		fs::create_dir_all(tree.0.join("hwmon0")).unwrap();
		tree.write("hwmon0/name", "pwmfan\n");
		let reading = fan(&root).expect("a fan that is present is reported");
		assert!(reading.value.is_none());
		assert_eq!(reading.state, State::Fault);
		assert!(
			reading
				.error
				.as_deref()
				.is_some_and(|why| why.contains("fan1_input")),
			"{reading:?}"
		);
		assert!(reading.is_coherent());

		// And one that answers reports its speed.
		tree.write("hwmon0/fan1_input", "2400\n");
		let reading = fan(&root).expect("fitted");
		assert_eq!(reading.value, Some(Value::quantity(2400.0, "rpm")));
	}

	/// The ceiling is the board's own critical trip, never one we invented: a zone declaring none
	/// leaves the reading with no scale rather than inventing one to draw against.
	#[test]
	fn a_zone_declaring_no_critical_trip_gives_the_reading_no_scale() {
		let tree = Tree::new();
		let zone = tree.at(".");
		tree.write("temp", "48500\n");

		let reading = temperature(&zone).expect("fitted and answering");
		assert_eq!(reading.value, Some(Value::quantity(48.5, "°C")));
		assert!(!reading.value.as_ref().unwrap().has_scale());

		// Declare one, and it becomes the ceiling.
		tree.write("trip_point_0_type", "critical\n");
		tree.write("trip_point_0_temp", "85000\n");
		let reading = temperature(&zone).expect("fitted and answering");
		assert_eq!(reading.value, Some(Value::scaled(48.5, "°C", 85.0)));
	}

	#[test]
	fn a_speed_is_reported_in_gigahertz() {
		assert_eq!(gigahertz(2_400_000), Value::quantity(2.4, "GHz"));
		assert_eq!(gigahertz(1_500_000), Value::quantity(1.5, "GHz"));
	}

	/// Both conditions are reported independently, and neither is claimed when both are clear.
	#[test]
	fn throttling_names_each_condition_it_finds() {
		let Some(reading) = throttling() else { return };
		let Some(Value::Text(summary)) = &reading.value else {
			panic!("throttling is text")
		};
		assert!(
			["None", "Undervolted", "Slowed", "Undervolted and slowed"].contains(&summary.as_str()),
			"{summary}"
		);
		assert_eq!(reading.state.is_trouble(), summary != "None");
	}
}
