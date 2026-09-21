//! The battery and where the device's power is coming from.
//!
//! Behaviour is specified in NFO, "Power source and battery". The hardware is a Geekworm X120x
//! backup board: a Maxim gauge on I2C reporting cell voltage and state of charge, and a line the
//! board pulls high while external power reaches it.
//!
//! Three states are told apart, and the third is the one worth having. External power present is the
//! ordinary case. Absent with the cell draining is running on battery. Absent with the cell doing
//! nothing at all, while the device plainly still runs, means the device is being fed around the
//! backup board through the Pi's own socket: the battery is charged, the board answers, and a power
//! cut stops the device dead rather than switching it over. Nothing about that is visible from
//! outside the case, and it is the state an operator reaches by plugging into the more obvious of the
//! two inputs.
//!
//! The distinction is in the movement and not the level. Measured on the test device: a loaded cell
//! settled to 4.149 V, below the 4.156 V it held while idle and bypassed, because the charge differed
//! between the two runs. An idle cell does not move at all; a cell carrying the device drifts down a
//! few millivolts every ten seconds. State of charge is no use here, taking about eighty seconds to
//! move at all where the voltage is unambiguous within twenty or thirty.

use std::{
	collections::VecDeque,
	time::{Duration, Instant},
};

use bliti_core::channel::readings::{Reading, State, Value};

mod gpio;
mod i2c;

/// Where the gauge sits: bus 1, address 0x36, across the whole X120x family.
const I2C_BUS: &str = "/dev/i2c-1";
const GAUGE: u16 = 0x36;

/// The gauge's registers. Cell voltage is the top twelve bits at 1.25 mV a step; state of charge is
/// a whole percent in the high byte and a fraction of one in the low.
const REG_VCELL: u8 = 0x02;
const REG_SOC: u8 = 0x04;
const VCELL_STEP_MV: f64 = 1.25;

/// The line the backup board pulls high while external power reaches it, by the name the kernel
/// gives it on the pin header. Resolved by name rather than by number: the header is `gpiochip0` on
/// some kernels and `gpiochip4` on others.
const POWER_LINE: &str = "GPIO6";

/// How far back the cell voltage is watched to tell a drifting cell from a still one.
const WATCH: Duration = Duration::from_secs(45);

/// How long the cell must have been watched before a still one is believed. Under this, the device
/// reports running on battery rather than asserting a bypass: sending someone to move a plug that is
/// already right is worse than saying nothing new.
const SETTLED: Duration = Duration::from_secs(25);

/// Recent cell voltages, which is what tells a cell carrying the device from one doing nothing.
#[derive(Debug, Default)]
pub struct Watch {
	seen: VecDeque<(Instant, f64)>,
}

/// What the gauge answered.
#[derive(Debug, Clone, Copy)]
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

impl Watch {
	/// The battery and power-source readings, or nothing at all where no backup board is fitted.
	///
	/// Everything here is gated on the gauge answering. The power line is never read otherwise: it has
	/// a pull-up on a Pi, so an unconnected pin reads as external power present, and a machine with no
	/// backup board would report itself confidently running on mains.
	pub fn readings(&mut self) -> Vec<Reading> {
		let gauge = match self.gauge() {
			Ok(gauge) => gauge,
			// No gauge means no backup board. An operator standing at the device can see that, so it
			// is left out rather than reported absent.
			Err(i2c::Error::NoDevice) => {
				self.seen.clear();
				return Vec::new();
			}
			// The bus is there and the gauge did not answer. That is a fault nobody can see from
			// outside the case.
			Err(err) => {
				return vec![Reading::failed(
					"battery",
					"Battery",
					format_args!("the gauge at {GAUGE:#04x} did not answer: {err}"),
				)];
			}
		};

		self.remember(gauge.volts);
		let external = gpio::read_by_name(POWER_LINE).ok();
		let source = self.source(external);

		let mut readings = Vec::new();
		if let Some(source) = source {
			readings.push(describe_source(source));
		}
		readings.push(self.battery(gauge, source));
		readings
	}

	fn gauge(&self) -> Result<Gauge, i2c::Error> {
		let mut bus = i2c::Bus::open(I2C_BUS, GAUGE)?;
		let vcell = bus.read_word(REG_VCELL)?;
		let soc = bus.read_word(REG_SOC)?;
		Ok(Gauge {
			volts: f64::from(vcell >> 4) * VCELL_STEP_MV / 1000.0,
			charge: f64::from(soc >> 8) + f64::from(soc & 0xff) / 256.0,
		})
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

	/// Whether the cell has moved at all across the window. A cell carrying the device drifts down
	/// continuously; one that is merely sitting there does not move by even a single step of the
	/// gauge.
	fn still(&self) -> bool {
		let mut lowest = f64::MAX;
		let mut highest = f64::MIN;
		for (_, volts) in &self.seen {
			lowest = lowest.min(*volts);
			highest = highest.max(*volts);
		}
		highest - lowest < VCELL_STEP_MV / 1000.0
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

	fn battery(&self, gauge: Gauge, source: Option<Source>) -> Reading {
		let charge = (gauge.charge / 100.0).clamp(0.0, 1.0);
		let mut reading = Reading::new("battery", "Battery", Value::Fraction(charge))
			.with_detail("Charge", Value::quantity(round(gauge.charge, 2), "%"))
			.with_detail("Voltage", Value::quantity(round(gauge.volts, 3), "V"));

		let (direction, note) = match source {
			Some(Source::External) if charge > 0.99 => ("Full", None),
			Some(Source::External) => ("Charging", None),
			Some(Source::Battery) => ("Discharging", None),
			Some(Source::Bypassed) => (
				"Idle",
				Some(
					"The backup board is not carrying the device, so the cell is neither charging nor draining.",
				),
			),
			// No power line to read, so the only thing to go on is whether the cell is moving. It is
			// worth saying that this is worked out rather than measured.
			None if self.watched() < SETTLED => ("Not yet known", None),
			None if self.still() => (
				"Steady",
				Some("Worked out from the cell voltage, not measured."),
			),
			None => (
				"Draining",
				Some("Worked out from the cell voltage, not measured."),
			),
		};
		reading = reading.with_detail("Direction", Value::text(direction));
		if let Some(note) = note {
			reading = reading.with_note(note);
		}

		// External power reported present while the cell drains is a fault in the supply or in the
		// board's own sensing, and an operator cannot see either from outside the case.
		if source == Some(Source::External) && self.watched() >= SETTLED && self.draining() {
			reading = reading.with_state(State::Warn).with_note(
				"External power is reported but the cell is draining. The supply may not be reaching \
				 the board.",
			);
		}
		reading
	}

	/// Whether the cell has fallen across the window, rather than merely wobbled.
	fn draining(&self) -> bool {
		let (Some((_, first)), Some((_, last))) = (self.seen.front(), self.seen.back()) else {
			return false;
		};
		first - last >= VCELL_STEP_MV * 2.0 / 1000.0
	}
}

fn describe_source(source: Source) -> Reading {
	let reading = Reading::new(
		"power-source",
		"Power",
		Value::text(match source {
			Source::External => "External",
			Source::Battery => "Battery",
			Source::Bypassed => "Backup bypassed",
		}),
	);
	match source {
		Source::Bypassed => reading.with_state(State::Warn).with_note(
			"Power is reaching the Pi directly, so the backup cannot take over and a power cut will \
			 stop the device. Move the supply to the backup board's own input.",
		),
		_ => reading,
	}
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

	/// The case the whole three-state distinction exists for. Measured on the test device: bypassed,
	/// the cell did not move by one step of the gauge across a minute.
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
	fn a_still_cell_is_not_a_bypass_until_it_has_been_watched_long_enough() {
		let watch = watch(&[4.156; 3], Duration::from_secs(2));
		assert!(watch.watched() < SETTLED);
		assert_eq!(watch.source(Some(false)), Some(Source::Battery));
	}

	#[test]
	fn external_power_is_reported_whatever_the_cell_is_doing() {
		let still = watch(&[4.23; 12], Duration::from_secs(3));
		assert_eq!(still.source(Some(true)), Some(Source::External));
	}

	/// A board offering no power line to read gets no power-source reading at all, rather than a
	/// guess. The pin has a pull-up, so guessing would read as external power present.
	#[test]
	fn no_power_line_means_no_power_source_reading() {
		let watch = watch(&[4.156; 12], Duration::from_secs(3));
		assert_eq!(watch.source(None), None);

		let reading = watch.battery(
			Gauge {
				volts: 4.156,
				charge: 96.5,
			},
			None,
		);
		// The direction is still worked out, and says that it was.
		assert!(reading.note.is_some());
	}

	/// Distinguishing on the level rather than the movement would get both of these wrong: the loaded
	/// cell here sits below the idle one, because their charges differ.
	#[test]
	fn the_level_does_not_distinguish_the_states_but_the_movement_does() {
		let loaded = watch(
			&[4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139],
			Duration::from_secs(4),
		);
		let idle = watch(&[4.199; 12], Duration::from_secs(3));

		// The loaded cell is lower than the idle one throughout, and yet is the one on battery.
		assert_eq!(loaded.source(Some(false)), Some(Source::Battery));
		assert_eq!(idle.source(Some(false)), Some(Source::Bypassed));
	}

	#[test]
	fn a_bypass_warns_and_says_how_to_fix_it() {
		let reading = describe_source(Source::Bypassed);
		assert_eq!(reading.state, State::Warn);
		let note = reading.note.expect("a bypass explains itself");
		assert!(note.contains("power cut"), "{note}");
		assert!(note.contains("own input"), "{note}");
	}

	#[test]
	fn the_ordinary_states_do_not_warn() {
		assert!(!describe_source(Source::External).state.is_trouble());
		assert!(!describe_source(Source::Battery).state.is_trouble());
	}

	/// External power present while the cell drains is a fault in the supply or in the board's own
	/// sensing. A poor pogo-pin contact reads as AC present with the plug out, which is documented on
	/// this board rather than hypothetical.
	#[test]
	fn external_power_while_the_cell_drains_is_flagged() {
		let draining = watch(
			&[4.159, 4.155, 4.151, 4.149, 4.146, 4.144, 4.141, 4.139],
			Duration::from_secs(4),
		);
		let reading = draining.battery(
			Gauge {
				volts: 4.139,
				charge: 96.0,
			},
			Some(Source::External),
		);
		assert_eq!(reading.state, State::Warn);
		assert!(reading.note.expect("it says why").contains("draining"));
	}

	#[test]
	fn the_battery_headline_is_state_of_charge_with_the_rest_behind_it() {
		let watch = watch(&[4.23; 12], Duration::from_secs(3));
		let reading = watch.battery(
			Gauge {
				volts: 4.231,
				charge: 97.66,
			},
			Some(Source::External),
		);
		assert!(reading.is_coherent());
		assert!(
			matches!(reading.value, Some(Value::Fraction(charge)) if (charge - 0.9766).abs() < 1e-6)
		);
		let labels: Vec<&str> = reading
			.detail
			.iter()
			.map(|each| each.label.as_str())
			.collect();
		assert_eq!(labels, ["Charge", "Voltage", "Direction"]);
	}

	/// A full cell on external power is full rather than charging, which is what an operator reads.
	#[test]
	fn a_full_cell_on_external_power_says_so() {
		let watch = watch(&[4.23; 12], Duration::from_secs(3));
		let reading = watch.battery(
			Gauge {
				volts: 4.23,
				charge: 99.8,
			},
			Some(Source::External),
		);
		let direction = reading.detail.last().expect("direction");
		assert_eq!(direction.value, Value::text("Full"));
	}

	/// Reads the hardware itself, so it says nothing on a machine with no backup board fitted and is
	/// ignored by default. Run it on one with `--ignored --nocapture`.
	#[test]
	#[ignore = "needs a device with a backup board fitted"]
	fn the_real_hardware_answers() {
		let mut watch = Watch::default();
		match watch.gauge() {
			Ok(gauge) => println!("gauge: {:.3} V, {:.2}%", gauge.volts, gauge.charge),
			Err(err) => println!("gauge FAILED: {err}"),
		}
		match gpio::read_by_name(POWER_LINE) {
			Ok(high) => println!("{POWER_LINE}: {}", if high { "high" } else { "low" }),
			Err(err) => println!("{POWER_LINE} FAILED: {err}"),
		}
		let readings = watch.readings();
		println!("readings: {}", readings.len());
		for reading in &readings {
			println!(
				"  {} = {:?} state={:?}",
				reading.name, reading.value, reading.state
			);
		}
	}

	#[test]
	fn the_gauge_maths_matches_what_the_hardware_reported() {
		// The words read from the test device, big-endian as the gauge sends them.
		let vcell: u16 = 0xd160;
		let soc: u16 = 0x6459;
		let volts = f64::from(vcell >> 4) * VCELL_STEP_MV / 1000.0;
		let charge = f64::from(soc >> 8) + f64::from(soc & 0xff) / 256.0;
		assert!((volts - 4.1875).abs() < 0.001, "{volts}");
		assert!((charge - 100.35).abs() < 0.01, "{charge}");
	}
}
