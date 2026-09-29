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
//! The board is looked at every ten seconds by the record thread, whether or not the device is
//! sampling, into the one [`Supply`]; the readings here report what it last saw. The charge reported
//! is CHG's estimate from the cell voltage rather than the gauge's own figure.
//!
//! Where no gauge answers there is no backup board, and the battery comes from the operating system
//! instead: upower where it can be reached, and `/sys/class/power_supply` where it cannot. That path
//! reports no `power-source`, because an operating system cannot tell a device fed through an
//! external supply from one fed around it (NFO).

use std::time::{Duration, Instant};

use bliti_core::channel::readings::{Entry, kind};
use serde_json::{Map, Value as Json};

use self::supply::{Reading, Seen};

mod battery;
pub mod curve;
mod gpio;
mod i2c;
mod record;
mod supply;
mod sysfs;
mod upower;

pub use record::record_supply;
pub use supply::Supply;

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

/// Below these, a battery carrying the device is low: `warning`, then `failed` (NFO).
const LOW_WARNING: f64 = 0.2;
const LOW_FAILED: f64 = 0.05;

/// The battery readings, from what the supply's record thread last saw of the backup board, or from
/// the operating system where there is none.
#[derive(Debug, Default)]
pub struct Watch {
	supply: Supply,
}

/// The cell voltage over the last [`WATCH`], which is what tells a cell carrying the device from one
/// doing nothing.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Recent {
	seen: Vec<(Instant, f64)>,
}

/// What the gauge answered.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gauge {
	pub volts: f64,
	/// State of charge, in percent.
	pub charge: f64,
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
	/// Battery readings from what `supply` last saw.
	pub fn new(supply: Supply) -> Self {
		Self { supply }
	}

	/// The supply this reports from.
	pub fn supply(&self) -> &Supply {
		&self.supply
	}

	/// The power-source and battery readings.
	///
	/// The gauge is the device we ship and comes first. Where it does not answer there is no backup
	/// board, and the battery is whatever the operating system reports instead; where neither has one,
	/// nothing is reported, because an operator standing at the device can see no battery is fitted
	/// (NFO). The backup board's readings carry the time the record thread took them, rather than
	/// `at`.
	pub fn readings(&self, at: u64) -> Vec<Entry> {
		match self.supply.seen() {
			Seen::Nothing => Vec::new(),
			Seen::NoGauge => battery::entries(at, &os_batteries()),
			// The bus is there and the gauge did not answer: a fault nobody can see from outside.
			Seen::Unanswered { at, reason } => vec![
				Entry::broken(
					at,
					"battery-charge",
					kind::FRACTION,
					format!("the gauge at {GAUGE:#04x} did not answer: {reason}"),
				)
				.with_trait("battery", built_in()),
			],
			Seen::Gauge(reading) => gauge_entries(&reading),
		}
	}
}

/// The backup board's readings.
///
/// The power line is read only once the gauge has answered. It has a pull-up on a Pi, so an
/// unconnected pin reads as external power present, and a machine with no backup board would
/// otherwise report itself confidently running on mains.
fn gauge_entries(reading: &Reading) -> Vec<Entry> {
	let at = reading.at;
	let recent = &reading.recent;
	let source = recent.source(reading.external);
	let about = built_in();
	let direction = battery_direction(at, recent, source, reading.full);

	let mut readings = Vec::new();
	if let Some(source) = source {
		readings.push(power_source(at, source));
	}
	let carrying = source == Some(Source::Battery)
		|| direction
			.value
			.as_ref()
			.is_some_and(|value| value == "discharging");
	readings.push(
		battery_charge(at, reading.charge, recent, source, carrying)
			.with_trait("battery", about.clone()),
	);
	readings.push(
		Entry::quantity(
			at,
			"battery-voltage",
			"volts",
			round(reading.gauge.volts, 3),
		)
		.with_trait("battery", about.clone()),
	);
	readings.push(direction.with_trait("battery", about));
	readings
}

impl Recent {
	pub fn new(seen: Vec<(Instant, f64)>) -> Self {
		Self { seen }
	}

	/// How long the cell has been watched without a gap.
	fn watched(&self) -> Duration {
		match (self.seen.first(), self.seen.last()) {
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
		let (Some((_, first)), Some((_, last))) = (self.seen.first(), self.seen.last()) else {
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
}

/// The `power-source` reading. A bypass is a `warning`, because the device is unprotected (NFO).
fn power_source(at: u64, source: Source) -> Entry {
	let entry = Entry::text(at, "power-source", source.as_str());
	match source {
		Source::Bypassed => entry.warning(
			"the backup supply is being bypassed; a power cut will stop the device, so move the \
			 supply to the backup board's own input",
		),
		_ => entry,
	}
}

/// The charge as CHG estimates it.
///
/// Where the hardware's power source and the cell's direction of travel disagree, this is a
/// `warning` and the direction is reported as the source gives it. That is only ever on mains, where
/// the battery is not carrying the device, so it never meets the low-battery statuses (NFO).
fn battery_charge(
	at: u64,
	charge: f64,
	recent: &Recent,
	source: Option<Source>,
	carrying: bool,
) -> Entry {
	let entry = Entry::fraction(at, "battery-charge", charge.clamp(0.0, 1.0));
	if source == Some(Source::External) && recent.watched() >= SETTLED && recent.draining() {
		return entry.warning(
			"the cell's direction of travel disagrees with the power source: external power is \
			 reported but the cell is draining",
		);
	}
	when_low(entry, charge, carrying)
}

/// A battery carrying the device is `warning` below a fifth and `failed` below a twentieth, for the
/// backup board's cell and the operating system's batteries alike (NFO).
fn when_low(entry: Entry, charge: f64, carrying: bool) -> Entry {
	if !carrying {
		entry
	} else if charge < LOW_FAILED {
		entry.failed("the battery is low, and nearly empty")
	} else if charge < LOW_WARNING {
		entry.warning("the battery is low")
	} else {
		entry
	}
}

/// The cell's direction of travel: charging, discharging or idle. Kept consistent with the power
/// source where one exists, charging on mains until the charge has finished and idle after (CHG);
/// where none does, worked out from the voltage and skipped until the voltage has been watched long
/// enough (NFO).
fn battery_direction(at: u64, recent: &Recent, source: Option<Source>, full: bool) -> Entry {
	let value = match source {
		Some(Source::External) if full => "idle",
		Some(Source::External) => "charging",
		Some(Source::Battery) => "discharging",
		Some(Source::Bypassed) => "idle",
		None if recent.watched() < SETTLED => {
			return Entry::skipped(
				at,
				"battery-direction",
				kind::TEXT,
				"the cell voltage has not been watched long enough to establish its direction",
			);
		}
		None if recent.still() => "idle",
		None => "discharging",
	};
	Entry::text(at, "battery-direction", value)
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

	/// The cell as the record thread saw it, readings `apart`, the newest now.
	fn recent(volts: &[f64], apart: Duration) -> Recent {
		let start = Instant::now() - apart * volts.len() as u32;
		Recent::new(
			volts
				.iter()
				.enumerate()
				.map(|(index, value)| (start + apart * index as u32, *value))
				.collect(),
		)
	}

	fn draining() -> Recent {
		recent(
			&[4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139],
			Duration::from_secs(4),
		)
	}

	fn reading(recent: Recent, external: Option<bool>, charge: f64, full: bool) -> Reading {
		Reading {
			at: 1,
			gauge: Gauge {
				volts: 3.6,
				charge: 50.0,
			},
			external,
			charge,
			full,
			recent,
		}
	}

	fn named<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
		entries.iter().find(|entry| entry.name == name).unwrap()
	}

	/// The case the whole three-state distinction exists for: still, no external power, watched long
	/// enough, is a bypass (NFO).
	#[test]
	fn a_still_cell_with_no_external_power_is_a_bypass() {
		let recent = recent(&[4.156; 12], Duration::from_secs(3));
		assert_eq!(recent.source(Some(false)), Some(Source::Bypassed));
	}

	#[test]
	fn a_draining_cell_with_no_external_power_is_running_on_battery() {
		let recent = recent(
			&[
				4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139, 4.136, 4.134,
			],
			Duration::from_secs(4),
		);
		assert_eq!(recent.source(Some(false)), Some(Source::Battery));
	}

	/// Asserting a bypass early would send someone to move a plug that is already correct.
	#[test]
	fn a_still_cell_is_not_a_bypass_until_watched_long_enough() {
		let recent = recent(&[4.156; 3], Duration::from_secs(2));
		assert!(recent.watched() < SETTLED);
		assert_eq!(recent.source(Some(false)), Some(Source::Battery));
	}

	/// Ten seconds apart, as the record thread looks, a still cell over the watch window is a bypass
	/// and a draining one is not.
	#[test]
	fn the_record_threads_cadence_tells_the_states_apart() {
		let every = Duration::from_secs(10);
		assert_eq!(
			recent(&[4.156; 5], every).source(Some(false)),
			Some(Source::Bypassed)
		);
		assert_eq!(
			recent(&[4.159, 4.152, 4.146, 4.139, 4.133], every).source(Some(false)),
			Some(Source::Battery)
		);
	}

	/// The three power-source wire values, and a bypass as a warning (NFO).
	#[test]
	fn power_source_reports_the_three_values_and_warns_on_a_bypass() {
		assert_eq!(
			power_source(1, Source::External).value.as_ref().unwrap(),
			"via-backup"
		);
		assert_eq!(
			power_source(1, Source::Battery).value.as_ref().unwrap(),
			"battery"
		);
		let bypass = power_source(1, Source::Bypassed);
		assert_eq!(bypass.value.as_ref().unwrap(), "bypassing-backup");
		assert_eq!(bypass.status(), Some("warning"));
	}

	/// Direction follows the power source where one exists (NFO): on mains, charging until the charge
	/// has finished and idle after, whatever the gauge's figure (CHG).
	#[test]
	fn direction_follows_the_power_source() {
		let still = recent(&[4.156; 12], Duration::from_secs(3));
		let direction = |source, full| {
			battery_direction(1, &still, Some(source), full)
				.value
				.unwrap()
		};
		assert_eq!(direction(Source::External, false), "charging");
		assert_eq!(direction(Source::External, true), "idle");
		assert_eq!(direction(Source::Battery, false), "discharging");
		assert_eq!(direction(Source::Bypassed, false), "idle");
	}

	/// A full cell on mains is idle once its charge has finished, and charging until then, whatever
	/// the charge estimated (CHG, NFO).
	#[test]
	fn on_mains_the_direction_follows_the_finished_charge() {
		let still = recent(&[4.1875; 5], Duration::from_secs(10));
		let charging = gauge_entries(&reading(still.clone(), Some(true), 1.0, false));
		assert_eq!(
			named(&charging, "battery-direction")
				.value
				.as_ref()
				.unwrap(),
			"charging"
		);
		let full = gauge_entries(&reading(still, Some(true), 0.93, true));
		assert_eq!(
			named(&full, "battery-direction").value.as_ref().unwrap(),
			"idle"
		);
	}

	/// With no power line, direction is skipped until the voltage settles, then reported plainly with
	/// no standing warning (NFO).
	#[test]
	fn direction_is_skipped_until_settled_then_reported_plainly() {
		let early = recent(&[4.156; 3], Duration::from_secs(2));
		let skipped = battery_direction(1, &early, None, false);
		assert_eq!(skipped.status(), Some("skipped"));
		assert!(skipped.reason().is_some());

		let idle = recent(&[4.156; 12], Duration::from_secs(3));
		let direction = battery_direction(1, &idle, None, false);
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
		let charge = battery_charge(1, 0.96, &draining(), Some(Source::External), false);
		assert_eq!(charge.status(), Some("warning"));
		assert!(charge.reason().is_some_and(|why| why.contains("disagree")));
	}

	/// A disagreement is on mains, where the battery is not carrying the device, so even a charge
	/// that would be low off mains reports the disagreement (NFO).
	#[test]
	fn a_disagreement_is_reported_over_a_low_charge_on_mains() {
		let entries = gauge_entries(&reading(draining(), Some(true), 0.03, false));
		let charge = named(&entries, "battery-charge");
		assert_eq!(charge.status(), Some("warning"));
		assert!(charge.reason().is_some_and(|why| why.contains("disagree")));
	}

	/// While the cell carries the device, the charge is a warning below 0.2 and failed below 0.05,
	/// saying the battery is low (NFO).
	#[test]
	fn a_low_charge_on_battery_warns_then_fails() {
		let status = |charge| {
			let entries = gauge_entries(&reading(draining(), Some(false), charge, false));
			let entry = named(&entries, "battery-charge").clone();
			assert_eq!(entry.value.as_ref().unwrap().as_f64(), Some(charge));
			(
				entry.status().unwrap().to_owned(),
				entry.reason().map(ToOwned::to_owned),
			)
		};
		assert_eq!(status(0.2), ("passed".to_owned(), None));
		let (warning, why) = status(0.19);
		assert_eq!(warning, "warning");
		assert!(why.unwrap().contains("battery is low"));
		assert_eq!(status(0.05).0, "warning");
		let (failed, why) = status(0.04);
		assert_eq!(failed, "failed");
		assert!(why.unwrap().contains("battery is low"));
	}

	/// A low charge says nothing where the battery is not carrying the device: on mains, and fed
	/// around the backup board (NFO).
	#[test]
	fn a_low_charge_off_battery_is_passed() {
		let still = recent(&[3.6; 5], Duration::from_secs(10));
		for external in [true, false] {
			let entries = gauge_entries(&reading(still.clone(), Some(external), 0.03, false));
			assert_eq!(named(&entries, "battery-charge").status(), Some("passed"));
		}
	}

	/// With no power line, a cell whose voltage shows it carrying the device is low as well (NFO).
	#[test]
	fn a_low_charge_discharging_with_no_power_line_fails() {
		let entries = gauge_entries(&reading(draining(), None, 0.03, false));
		assert!(!entries.iter().any(|entry| entry.name == "power-source"));
		assert_eq!(named(&entries, "battery-charge").status(), Some("failed"));
	}

	/// Distinguishing on the level rather than the movement would get both of these wrong: the loaded
	/// cell here sits below the idle one, because their charges differ.
	#[test]
	fn the_movement_distinguishes_the_states_not_the_level() {
		let idle = recent(&[4.199; 12], Duration::from_secs(3));
		assert_eq!(draining().source(Some(false)), Some(Source::Battery));
		assert_eq!(idle.source(Some(false)), Some(Source::Bypassed));
	}

	/// Before the record thread's first look, there is nothing to report from.
	#[test]
	fn nothing_is_reported_before_the_first_look() {
		assert!(Watch::default().readings(1).is_empty());
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
