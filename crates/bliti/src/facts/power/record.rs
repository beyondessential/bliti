//! The backup supply's history in the daemon's log: when external power went and came back, and how
//! the cell fell in between, so a run on battery can be read back after the device has died (DEV).
//!
//! On its own thread rather than in the sampler, because sampling stops when nobody has been
//! connected for a while, and a power cut seldom has an audience.

use std::{
	io, thread,
	time::{Duration, Instant},
};

use super::{
	Gauge, POWER_LINE, gpio, i2c, read_gauge,
	supply::{Held, Look, Supply, Unanswered},
};
use crate::power::{Controller, NotBegun};

/// How often the supply is looked at: at least every ten seconds, which LOW asks for while external
/// power is absent.
const EVERY: Duration = Duration::from_secs(10);

/// How far the cell falls between the voltages reported while external power is absent, in
/// millivolts. Eight gauge steps: about a hundred reports across a full run, closest together in the
/// steep fall just before the device dies.
const FALL_MV: f64 = 10.0;

/// Watch the backup supply for as long as the daemon runs: updating `supply`, reporting what DEV
/// asks for, and powering off through `controller` once the cell has held below the floor (LOW).
/// Silent where no gauge answers, as on a machine with no backup board, and from the moment one does.
pub fn record_supply(supply: Supply, controller: Controller) -> io::Result<()> {
	thread::Builder::new()
		.name("power-record".to_owned())
		.spawn(move || {
			let mut next = Instant::now();
			loop {
				let look = look();
				let now = Instant::now();
				let observed = supply.observe(
					look,
					now,
					crate::facts::Facts::since_boot(),
					crate::facts::uptime(),
				);
				if let Some((event, charge)) = observed.report {
					event.report(charge);
				}
				if let Some(held) = observed.held {
					power_off(&supply, &controller, held);
				}
				// Timed from the last look's start, so the reads themselves do not stretch the interval.
				next += EVERY;
				thread::sleep(next.saturating_duration_since(Instant::now()));
			}
		})
		.map(drop)
}

/// Read the gauge, and the power line once it has answered: the line has a pull-up, so an
/// unconnected pin reads as external power present.
fn look() -> Look {
	let gauge = read_gauge().map_err(|err| match err {
		i2c::Error::NoDevice => Unanswered::NoGauge,
		err => Unanswered::Failed(err.to_string()),
	});
	let external = match gauge {
		Ok(_) => gpio::read_by_name(POWER_LINE).ok(),
		Err(_) => None,
	};
	Look { gauge, external }
}

/// What can power the device off for a low battery: the [`Controller`], or a test's stand-in.
pub trait PowerOff {
	/// As [`Controller::low_battery`].
	fn low_battery(&self, before: impl FnOnce()) -> Result<(), NotBegun>;
}

impl PowerOff for Controller {
	fn low_battery(&self, before: impl FnOnce()) -> Result<(), NotBegun> {
		Controller::low_battery(self, before)
	}
}

/// The floor has been held: record what the run taught, then power off, saying so (LOW).
fn power_off(supply: &Supply, controller: &impl PowerOff, held: Held) {
	let Held { volts, away } = held;
	let volts = format!("{volts:.4}");
	let away_secs = away.map(|away| away.as_secs());
	let result = controller.low_battery(|| {
		supply.learn_from_run();
		tracing::warn!(%volts, ?away_secs, "powering off for low battery");
	});
	if let Err(NotBegun::CannotPowerOff) = result {
		tracing::warn!(
			%volts,
			?away_secs,
			"the battery is below the floor and this device cannot power off"
		);
	}
	supply.settle(&result);
}

/// What has been seen of the supply so far.
#[derive(Debug, Default)]
pub struct Record {
	/// Whether external power reached the board at the last look, and nothing before the first.
	external: Option<bool>,
	/// When external power was seen to go, where it went while the device watched.
	lost: Option<Instant>,
	/// The cell voltage last reported while external power is absent.
	reported: f64,
}

/// Something to report.
#[derive(Debug, PartialEq)]
pub enum Event {
	/// The first look since the daemon started.
	Found {
		external: bool,
		gauge: Gauge,
	},
	Lost {
		gauge: Gauge,
	},
	Restored {
		gauge: Gauge,
		away: Option<Duration>,
	},
	/// The cell has fallen another step with external power absent.
	Falling {
		gauge: Gauge,
		away: Option<Duration>,
	},
}

impl Record {
	/// Take in a look where the gauge and the power line both answered.
	pub fn observe(&mut self, now: Instant, external: bool, gauge: Gauge) -> Option<Event> {
		let away = |lost: Option<Instant>| lost.map(|at| now.duration_since(at));
		match self.external.replace(external) {
			None => {
				self.reported = gauge.volts;
				Some(Event::Found { external, gauge })
			}
			Some(true) if !external => {
				self.lost = Some(now);
				self.reported = gauge.volts;
				Some(Event::Lost { gauge })
			}
			Some(false) if external => Some(Event::Restored {
				gauge,
				away: away(self.lost.take()),
			}),
			Some(true) => None,
			// Half a millivolt of slack, so a fall of exactly one step is not lost to floating point.
			Some(false) => ((self.reported - gauge.volts) * 1000.0 >= FALL_MV - 0.5).then(|| {
				self.reported = gauge.volts;
				Event::Falling {
					gauge,
					away: away(self.lost),
				}
			}),
		}
	}
}

impl Record {
	/// How long external power has been absent, where the device saw it go.
	pub fn away(&self, now: Instant) -> Option<Duration> {
		match self.external {
			Some(false) => self.lost.map(|at| now.duration_since(at)),
			_ => None,
		}
	}
}

impl Event {
	/// Report on standard error, with `estimate` the charge CHG gives at the time (DEV).
	pub fn report(&self, estimate: f64) {
		// How long external power has been absent, where the device saw it go; unknown where it was
		// already absent when the daemon started.
		let secs = |away: &Option<Duration>| away.map(|away| away.as_secs());
		match self {
			Self::Found { external, gauge } => tracing::info!(
				external_power = external,
				volts = %volts(gauge),
				charge = %percent(estimate),
				gauge_charge = %charge(gauge),
				"backup supply found"
			),
			Self::Lost { gauge } => tracing::warn!(
				volts = %volts(gauge),
				charge = %percent(estimate),
				gauge_charge = %charge(gauge),
				"external power no longer reaches the backup supply"
			),
			Self::Restored { gauge, away } => tracing::info!(
				volts = %volts(gauge),
				charge = %percent(estimate),
				gauge_charge = %charge(gauge),
				away_secs = ?secs(away),
				"external power reaches the backup supply again"
			),
			Self::Falling { gauge, away } => tracing::info!(
				volts = %volts(gauge),
				charge = %percent(estimate),
				gauge_charge = %charge(gauge),
				away_secs = ?secs(away),
				"cell falling without external power"
			),
		}
	}
}

fn volts(gauge: &Gauge) -> String {
	format!("{:.4}", gauge.volts)
}

/// The gauge's own state of charge, as a percentage.
fn charge(gauge: &Gauge) -> String {
	format!("{:.1}", gauge.charge)
}

/// A share as a percentage, as the gauge's is given.
fn percent(share: f64) -> String {
	format!("{:.1}", share * 100.0)
}

#[cfg(test)]
mod tests {
	use std::cell::Cell;

	use super::*;
	use crate::{
		facts::curve::store::Store,
		power::{Act, Cause, Going},
	};

	fn gauge(volts: f64) -> Gauge {
		Gauge {
			volts,
			charge: 50.0,
		}
	}

	/// What the supply was doing when the daemon started is reported either way (DEV).
	#[test]
	fn the_first_look_is_reported() {
		let now = Instant::now();
		let mut record = Record::default();
		assert_eq!(
			record.observe(now, true, gauge(4.1)),
			Some(Event::Found {
				external: true,
				gauge: gauge(4.1)
			})
		);
		assert_eq!(record.observe(now + EVERY, true, gauge(4.1)), None);
	}

	/// Losing and regaining external power are both reported, the return with how long it was gone
	/// (DEV).
	#[test]
	fn a_power_cut_is_reported_at_both_ends() {
		let now = Instant::now();
		let mut record = Record::default();
		record.observe(now, true, gauge(4.1));
		assert_eq!(
			record.observe(now + EVERY, false, gauge(4.05)),
			Some(Event::Lost { gauge: gauge(4.05) })
		);
		let back = now + EVERY + Duration::from_secs(600);
		assert_eq!(
			record.observe(back, true, gauge(4.0)),
			Some(Event::Restored {
				gauge: gauge(4.0),
				away: Some(Duration::from_secs(600))
			})
		);
	}

	/// A draining cell is reported each step it falls, carrying how long external power has been
	/// gone, and not in between (DEV).
	#[test]
	fn a_falling_cell_is_reported_each_step() {
		let now = Instant::now();
		let mut record = Record::default();
		record.observe(now, true, gauge(3.9));
		record.observe(now, false, gauge(3.9));
		// Seven gauge steps is under one reporting step.
		assert_eq!(record.observe(now + EVERY, false, gauge(3.89125)), None);
		// Eight is one, however the steps sum.
		assert_eq!(
			record.observe(now + EVERY * 2, false, gauge(3.9 - 8.0 * 0.00125)),
			Some(Event::Falling {
				gauge: gauge(3.9 - 8.0 * 0.00125),
				away: Some(EVERY * 2)
			})
		);
		// The next step is counted from the voltage last reported.
		assert_eq!(record.observe(now + EVERY * 3, false, gauge(3.885)), None);
	}

	/// A device fed around its backup supply has a still cell, which adds nothing (DEV).
	#[test]
	fn a_still_cell_without_external_power_is_quiet() {
		let now = Instant::now();
		let mut record = Record::default();
		record.observe(now, false, gauge(4.15));
		for tick in 1..100 {
			let wobble = if tick % 2 == 0 { 0.00125 } else { 0.0 };
			assert_eq!(
				record.observe(now + EVERY * tick, false, gauge(4.15 - wobble)),
				None
			);
		}
	}

	/// Power already absent at start has no known time of going, so the fall is reported without one.
	#[test]
	fn a_fall_from_a_start_on_battery_has_no_time_away() {
		let now = Instant::now();
		let mut record = Record::default();
		record.observe(now, false, gauge(3.7));
		assert_eq!(
			record.observe(now + EVERY, false, gauge(3.69)),
			Some(Event::Falling {
				gauge: gauge(3.69),
				away: None
			})
		);
	}

	/// Our charge and the gauge's are both reported as percentages, to one place (DEV).
	#[test]
	fn our_charge_is_reported_as_the_gauges_is() {
		assert_eq!(percent(0.8734), "87.3");
		assert_eq!(charge(&gauge(3.9)), "50.0");
	}

	/// A stand-in for the controller, answering every shutdown asked for with `result`.
	struct Fake {
		result: Result<(), NotBegun>,
		asked: Cell<u32>,
		recorded: Cell<u32>,
	}

	impl Fake {
		fn new(result: Result<(), NotBegun>) -> Self {
			Self {
				result,
				asked: Cell::new(0),
				recorded: Cell::new(0),
			}
		}
	}

	impl PowerOff for Fake {
		fn low_battery(&self, before: impl FnOnce()) -> Result<(), NotBegun> {
			self.asked.set(self.asked.get() + 1);
			if self.result.is_ok() {
				before();
				self.recorded.set(self.recorded.get() + 1);
			}
			self.result
		}
	}

	/// Run the record thread's loop body for `ticks` looks below the floor with external power absent.
	fn below_the_floor(controller: &Fake, ticks: u32) {
		let supply = Supply::new(Store::new(
			std::env::temp_dir().join("bliti-record-unwritten/battery-curve.json"),
		));
		let start = Instant::now();
		for tick in 0..ticks {
			let look = Look {
				gauge: Ok(gauge(2.79)),
				external: Some(false),
			};
			let observed = supply.observe(
				look,
				start + EVERY * tick,
				1,
				Some(Duration::from_secs(3600)),
			);
			if let Some(held) = observed.held {
				power_off(&supply, controller, held);
			}
		}
	}

	/// Once begun, the run is recorded first and nothing more is asked (LOW).
	#[test]
	fn a_begun_shutdown_is_asked_for_once() {
		let controller = Fake::new(Ok(()));
		below_the_floor(&controller, 60);
		assert_eq!(controller.asked.get(), 1);
		assert_eq!(controller.recorded.get(), 1);
	}

	/// An act accepted first is carried out, and no shutdown is asked for again (LOW).
	#[test]
	fn no_second_shutdown_after_an_accepted_act() {
		let controller = Fake::new(Err(NotBegun::AlreadyGoing(Going {
			act: Act::Reboot,
			cause: Cause::ManualControl,
		})));
		below_the_floor(&controller, 60);
		assert_eq!(controller.asked.get(), 1);
		assert_eq!(controller.recorded.get(), 0);
	}

	/// A device that cannot power off is asked again each time the floor is held another sixty
	/// seconds, which is when it says it cannot (LOW).
	#[test]
	fn cannot_power_off_is_reported_each_sixty_seconds() {
		let controller = Fake::new(Err(NotBegun::CannotPowerOff));
		below_the_floor(&controller, 20);
		assert_eq!(controller.asked.get(), 2);
		assert_eq!(controller.recorded.get(), 0);
	}
}
