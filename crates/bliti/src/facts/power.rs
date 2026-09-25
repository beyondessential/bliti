//! The battery and where the device's power is coming from.
//!
//! Behaviour is specified in NFO, "Power source and battery". The hardware is a Geekworm X120x
//! backup board: a Maxim gauge on I2C reporting cell voltage and state of charge, and a line the
//! board pulls high while external power reaches it.
//!
//! Three states are told apart, and the third is the one worth having. External power present is the
//! ordinary case (`via-backup`). Absent with the cell draining is running on `battery`. Absent with
//! the cell doing nothing at all, while the device plainly still runs, means the device is being fed
//! around the backup board through the Pi's own socket (`bypassing-backup`): the battery is charged,
//! the board answers, and a power cut stops the device dead rather than switching it over.
//!
//! The distinction is in the movement and not the level. An idle cell does not move at all; a cell
//! carrying the device drifts down a few millivolts every ten seconds. State of charge is no use
//! here, taking about eighty seconds to move where the voltage is unambiguous within twenty or thirty.
//!
//! Where no gauge answers there is no backup board, and the battery comes from the operating system
//! instead: upower where it can be reached, and `/sys/class/power_supply` where it cannot. That path
//! reports no `power-source`, because an operating system cannot tell a device fed through an
//! external supply from one fed around it (NFO).

use std::{
	collections::VecDeque,
	time::{Duration, Instant},
};

use bliti_core::channel::readings::{Entry, kind};
use serde_json::{Map, Value as Json};

mod battery;
mod gpio;
mod i2c;
mod record;
mod sysfs;
mod upower;

pub use record::record_supply;

/// Where the gauge sits: bus 1, address 0x36, across the whole X120x family.
const I2C_BUS: &str = "/dev/i2c-1";
const GAUGE: u16 = 0x36;

/// The gauge's registers. Cell voltage is the top twelve bits at 1.25 mV a step; state of charge is
/// a whole percent in the high byte and a fraction of one in the low.
const REG_VCELL: u8 = 0x02;
const REG_SOC: u8 = 0x04;
const VCELL_STEP_MV: f64 = 1.25;

/// The line the backup board pulls high while external power reaches it, by the name the kernel gives
/// it on the pin header. Resolved by name because the header is `gpiochip0` on some kernels and
/// `gpiochip4` on others.
const POWER_LINE: &str = "GPIO6";

/// The backup board's own cell, which nothing reports a name for, so the device supplies one. Named
/// for sitting inside the case rather than for the board managing it, which is what tells it from an
/// external supply (NFO). The cell itself is from an unknowable third party, so only the board's
/// maker is carried.
const BUILT_IN: &str = "built-in";
const BOARD_VENDOR: &str = "SupTronics";

/// How far back the cell voltage is watched to tell a drifting cell from a still one.
const WATCH: Duration = Duration::from_secs(45);

/// How long the cell must have been watched before a still one is believed. Under this, the device
/// reports running on battery rather than asserting a bypass, and reports the direction as skipped.
const SETTLED: Duration = Duration::from_secs(25);

/// Recent cell voltages, which is what tells a cell carrying the device from one doing nothing.
#[derive(Debug, Default)]
pub struct Watch {
	seen: VecDeque<(Instant, f64)>,
}

/// What the gauge answered.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Gauge {
	volts: f64,
	charge: f64,
}

/// Where the power is coming from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
	/// Through the backup board, the ordinary case and the only one a power cut is survived in.
	External,
	/// The backup board is carrying the device.
	Battery,
	/// The device is fed directly and the backup board is idle, so it is not protected at all.
	Bypassed,
}

impl Source {
	/// The `power-source` wire value.
	fn as_str(self) -> &'static str {
		match self {
			Self::External => "via-backup",
			Self::Battery => "battery",
			Self::Bypassed => "bypassing-backup",
		}
	}
}

fn read_gauge() -> Result<Gauge, i2c::Error> {
	let mut bus = i2c::Bus::open(I2C_BUS, GAUGE)?;
	let vcell = bus.read_word(REG_VCELL)?;
	let soc = bus.read_word(REG_SOC)?;
	Ok(Gauge {
		volts: f64::from(vcell >> 4) * VCELL_STEP_MV / 1000.0,
		charge: f64::from(soc >> 8) + f64::from(soc & 0xff) / 256.0,
	})
}

impl Watch {
	/// The power-source and battery readings.
	///
	/// The gauge is the device we ship and comes first. Where it does not answer there is no backup
	/// board, and the battery is whatever the operating system reports instead; where neither has one,
	/// nothing is reported, because an operator standing at the device can see no battery is fitted
	/// (NFO).
	///
	/// The power line is read only once the gauge has answered. It has a pull-up on a Pi, so an
	/// unconnected pin reads as external power present, and a machine with no backup board would
	/// otherwise report itself confidently running on mains.
	pub fn readings(&mut self, at: u64) -> Vec<Entry> {
		let gauge = match read_gauge() {
			Ok(gauge) => gauge,
			Err(i2c::Error::NoDevice) => {
				self.seen.clear();
				return battery::entries(at, &os_batteries());
			}
			// The bus is there and the gauge did not answer: a fault nobody can see from outside.
			Err(err) => {
				return vec![
					Entry::broken(
						at,
						"battery-charge",
						kind::FRACTION,
						format!("the gauge at {GAUGE:#04x} did not answer: {err}"),
					)
					.with_trait("battery", built_in()),
				];
			}
		};

		self.remember(gauge.volts);
		let external = gpio::read_by_name(POWER_LINE).ok();
		let source = self.source(external);
		let about = built_in();

		let mut readings = Vec::new();
		if let Some(source) = source {
			readings.push(self.power_source(at, source));
		}
		readings.push(
			self.battery_charge(at, gauge, source)
				.with_trait("battery", about.clone()),
		);
		readings.push(
			Entry::quantity(at, "battery-voltage", "volts", round(gauge.volts, 3))
				.with_trait("battery", about.clone()),
		);
		readings.push(
			self.battery_direction(at, gauge, source)
				.with_trait("battery", about),
		);
		readings
	}

	fn remember(&mut self, volts: f64) {
		let now = Instant::now();
		self.seen.push_back((now, volts));
		while self
			.seen
			.front()
			.is_some_and(|(at, _)| now.duration_since(*at) > WATCH)
		{
			self.seen.pop_front();
		}
	}

	/// How long the cell has been watched without a gap.
	fn watched(&self) -> Duration {
		match (self.seen.front(), self.seen.back()) {
			(Some((first, _)), Some((last, _))) => last.duration_since(*first),
			_ => Duration::ZERO,
		}
	}

	/// Whether the cell has moved at all across the window.
	fn still(&self) -> bool {
		let mut lowest = f64::MAX;
		let mut highest = f64::MIN;
		for (_, volts) in &self.seen {
			lowest = lowest.min(*volts);
			highest = highest.max(*volts);
		}
		highest - lowest < VCELL_STEP_MV / 1000.0
	}

	/// Whether the cell has fallen across the window, rather than merely wobbled.
	fn draining(&self) -> bool {
		let (Some((_, first)), Some((_, last))) = (self.seen.front(), self.seen.back()) else {
			return false;
		};
		first - last >= VCELL_STEP_MV * 2.0 / 1000.0
	}

	/// Which of the three states holds, or nothing where the board offers no power line to read.
	fn source(&self, external: Option<bool>) -> Option<Source> {
		match external? {
			true => Some(Source::External),
			// Still, and watched long enough to believe it: the cell is neither charging nor carrying
			// the device, so something else is.
			false if self.watched() >= SETTLED && self.still() => Some(Source::Bypassed),
			false => Some(Source::Battery),
		}
	}

	/// The `power-source` reading. A bypass is a `warning`, because the device is unprotected (NFO).
	fn power_source(&self, at: u64, source: Source) -> Entry {
		let entry = Entry::text(at, "power-source", source.as_str());
		match source {
			Source::Bypassed => entry.warning(
				"the backup supply is being bypassed; a power cut will stop the device, so move the \
				 supply to the backup board's own input",
			),
			_ => entry,
		}
	}

	/// State of charge. Where the hardware's power source and the cell's direction of travel disagree,
	/// this is a `warning` and the direction is reported as the source gives it (NFO).
	fn battery_charge(&self, at: u64, gauge: Gauge, source: Option<Source>) -> Entry {
		let entry = Entry::fraction(at, "battery-charge", (gauge.charge / 100.0).clamp(0.0, 1.0));
		if source == Some(Source::External) && self.watched() >= SETTLED && self.draining() {
			return entry.warning(
				"the cell's direction of travel disagrees with the power source: external power is \
				 reported but the cell is draining",
			);
		}
		entry
	}

	/// The cell's direction of travel: charging, discharging or idle. Kept consistent with the power
	/// source where one exists; where none does, worked out from the voltage and skipped until the
	/// voltage has been watched long enough (NFO).
	fn battery_direction(&self, at: u64, gauge: Gauge, source: Option<Source>) -> Entry {
		let charge = (gauge.charge / 100.0).clamp(0.0, 1.0);
		let value = match source {
			Some(Source::External) if charge <= 0.99 => "charging",
			Some(Source::External) => "idle",
			Some(Source::Battery) => "discharging",
			Some(Source::Bypassed) => "idle",
			None if self.watched() < SETTLED => {
				return Entry::skipped(
					at,
					"battery-direction",
					kind::TEXT,
					"the cell voltage has not been watched long enough to establish its direction",
				);
			}
			None if self.still() => "idle",
			None => "discharging",
		};
		Entry::text(at, "battery-direction", value)
	}
}

/// The batteries the operating system reports.
///
/// upower answering with none is an answer: this machine has no battery. Only a failure to reach
/// upower at all falls through to sysfs, which cannot see a USB UPS and so is a fallback rather than
/// a second opinion.
fn os_batteries() -> Vec<battery::Battery> {
	match upower::batteries() {
		Ok(batteries) => batteries,
		Err(err) => {
			tracing::debug!(%err, "upower could not be reached; reading power supplies directly");
			sysfs::batteries()
		}
	}
}

/// The `battery` trait for the backup board's own cell.
fn built_in() -> Json {
	let mut object = Map::new();
	object.insert("name".to_owned(), Json::String(BUILT_IN.to_owned()));
	object.insert("vendor".to_owned(), Json::String(BOARD_VENDOR.to_owned()));
	Json::Object(object)
}

fn round(value: f64, places: i32) -> f64 {
	let scale = 10f64.powi(places);
	(value * scale).round() / scale
}

#[cfg(test)]
mod tests {
	use super::*;

	fn watch(volts: &[f64], apart: Duration) -> Watch {
		let mut seen = VecDeque::new();
		let start = Instant::now() - apart * volts.len() as u32;
		for (index, value) in volts.iter().enumerate() {
			seen.push_back((start + apart * index as u32, *value));
		}
		Watch { seen }
	}

	fn gauge(volts: f64, charge: f64) -> Gauge {
		Gauge { volts, charge }
	}

	/// The case the whole three-state distinction exists for: still, no external power, watched long
	/// enough, is a bypass (NFO).
	#[test]
	fn a_still_cell_with_no_external_power_is_a_bypass() {
		let watch = watch(&[4.156; 12], Duration::from_secs(3));
		assert_eq!(watch.source(Some(false)), Some(Source::Bypassed));
	}

	#[test]
	fn a_draining_cell_with_no_external_power_is_running_on_battery() {
		let watch = watch(
			&[
				4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139, 4.136, 4.134,
			],
			Duration::from_secs(4),
		);
		assert_eq!(watch.source(Some(false)), Some(Source::Battery));
	}

	/// Asserting a bypass early would send someone to move a plug that is already correct.
	#[test]
	fn a_still_cell_is_not_a_bypass_until_watched_long_enough() {
		let watch = watch(&[4.156; 3], Duration::from_secs(2));
		assert!(watch.watched() < SETTLED);
		assert_eq!(watch.source(Some(false)), Some(Source::Battery));
	}

	/// The three power-source wire values, and a bypass as a warning (NFO).
	#[test]
	fn power_source_reports_the_three_values_and_warns_on_a_bypass() {
		let watch = watch(&[4.156; 12], Duration::from_secs(3));
		assert_eq!(
			watch
				.power_source(1, Source::External)
				.value
				.as_ref()
				.unwrap(),
			"via-backup"
		);
		assert_eq!(
			watch
				.power_source(1, Source::Battery)
				.value
				.as_ref()
				.unwrap(),
			"battery"
		);
		let bypass = watch.power_source(1, Source::Bypassed);
		assert_eq!(bypass.value.as_ref().unwrap(), "bypassing-backup");
		assert_eq!(bypass.status(), Some("warning"));
	}

	/// Direction follows the power source where one exists (NFO).
	#[test]
	fn direction_follows_the_power_source() {
		let watch = watch(&[4.156; 12], Duration::from_secs(3));
		let charging = watch.battery_direction(1, gauge(4.1, 60.0), Some(Source::External));
		assert_eq!(charging.value.as_ref().unwrap(), "charging");
		let full = watch.battery_direction(1, gauge(4.2, 100.0), Some(Source::External));
		assert_eq!(full.value.as_ref().unwrap(), "idle");
		let discharging = watch.battery_direction(1, gauge(4.1, 60.0), Some(Source::Battery));
		assert_eq!(discharging.value.as_ref().unwrap(), "discharging");
		let bypass = watch.battery_direction(1, gauge(4.15, 96.0), Some(Source::Bypassed));
		assert_eq!(bypass.value.as_ref().unwrap(), "idle");
	}

	/// With no power line, direction is skipped until the voltage settles, then reported plainly with
	/// no standing warning (NFO).
	#[test]
	fn direction_is_skipped_until_settled_then_reported_plainly() {
		let early = watch(&[4.156; 3], Duration::from_secs(2));
		let skipped = early.battery_direction(1, gauge(4.156, 96.0), None);
		assert_eq!(skipped.status(), Some("skipped"));
		assert!(skipped.reason().is_some());

		let idle = watch(&[4.156; 12], Duration::from_secs(3));
		let direction = idle.battery_direction(1, gauge(4.156, 96.0), None);
		assert_eq!(
			direction.status(),
			Some("passed"),
			"no standing warning once settled"
		);
		assert_eq!(direction.value.as_ref().unwrap(), "idle");
	}

	/// External power present while the cell drains is the disagreement case: power-source as the
	/// hardware gives it, battery-charge a warning (NFO).
	#[test]
	fn a_disagreement_warns_on_the_charge() {
		let draining = watch(
			&[4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139],
			Duration::from_secs(4),
		);
		let charge = draining.battery_charge(1, gauge(4.139, 96.0), Some(Source::External));
		assert_eq!(charge.status(), Some("warning"));
		assert!(charge.reason().is_some_and(|why| why.contains("disagree")));
	}

	/// Distinguishing on the level rather than the movement would get both of these wrong: the loaded
	/// cell here sits below the idle one, because their charges differ.
	#[test]
	fn the_movement_distinguishes_the_states_not_the_level() {
		let loaded = watch(
			&[4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139],
			Duration::from_secs(4),
		);
		let idle = watch(&[4.199; 12], Duration::from_secs(3));
		assert_eq!(loaded.source(Some(false)), Some(Source::Battery));
		assert_eq!(idle.source(Some(false)), Some(Source::Bypassed));
	}

	/// The backup board's cell is named by the device, since nothing reports a name for it, and is the
	/// only battery called `built-in` (NFO).
	#[test]
	fn the_backup_boards_cell_names_itself() {
		let about = built_in();
		assert_eq!(about.get("name").unwrap().as_str(), Some("built-in"));
		assert_eq!(about.get("vendor").unwrap().as_str(), Some("SupTronics"));
		assert!(
			about.get("serial").is_none() && about.get("model").is_none(),
			"the cell is from an unknowable third party"
		);
	}

	#[test]
	fn the_gauge_maths_matches_what_the_hardware_reported() {
		let vcell: u16 = 0xd160;
		let soc: u16 = 0x6459;
		let volts = f64::from(vcell >> 4) * VCELL_STEP_MV / 1000.0;
		let charge = f64::from(soc >> 8) + f64::from(soc & 0xff) / 256.0;
		assert!((volts - 4.1875).abs() < 0.001, "{volts}");
		assert!((charge - 100.35).abs() < 0.01, "{charge}");
	}
}
