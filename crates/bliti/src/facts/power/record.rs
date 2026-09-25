//! The backup supply's history in the daemon's log: when external power went and came back, and how
//! the cell fell in between, so a run on battery can be read back after the device has died (DEV).
//!
//! On its own thread rather than in the sampler, because sampling stops when nobody has been
//! connected for a while, and a power cut seldom has an audience.

use std::{
	io, thread,
	time::{Duration, Instant},
};

use super::{Gauge, POWER_LINE, gpio, read_gauge};

/// How often the supply is looked at.
const EVERY: Duration = Duration::from_secs(10);

/// How far the cell falls between the voltages reported while external power is absent, in
/// millivolts. Eight gauge steps: about a hundred reports across a full run, closest together in the
/// steep fall just before the device dies.
const FALL_MV: f64 = 10.0;

/// Watch the backup supply for as long as the daemon runs, reporting what DEV asks for. Silent where
/// no gauge answers, as on a machine with no backup board, and from the moment one does.
pub fn record_supply() -> io::Result<()> {
	thread::Builder::new()
		.name("power-record".to_owned())
		.spawn(|| {
			let mut record = Record::default();
			loop {
				// The line only means anything once the gauge has answered: it has a pull-up, so an
				// unconnected pin reads as external power present.
				if let Ok(gauge) = read_gauge()
					&& let Ok(external) = gpio::read_by_name(POWER_LINE)
					&& let Some(event) = record.observe(Instant::now(), external, gauge)
				{
					event.report();
				}
				thread::sleep(EVERY);
			}
		})
		.map(drop)
}

/// What has been seen of the supply so far.
#[derive(Debug, Default)]
struct Record {
	/// Whether external power reached the board at the last look, and nothing before the first.
	external: Option<bool>,
	/// When external power was seen to go, where it went while the device watched.
	lost: Option<Instant>,
	/// The cell voltage last reported while external power is absent.
	reported: f64,
}

/// Something to report.
#[derive(Debug, PartialEq)]
enum Event {
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
	fn observe(&mut self, now: Instant, external: bool, gauge: Gauge) -> Option<Event> {
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

impl Event {
	fn report(&self) {
		// How long external power has been absent, where the device saw it go; unknown where it was
		// already absent when the daemon started.
		let secs = |away: &Option<Duration>| away.map(|away| away.as_secs());
		match self {
			Self::Found { external, gauge } => tracing::info!(
				external_power = external,
				volts = %volts(gauge),
				charge = %charge(gauge),
				"backup supply found"
			),
			Self::Lost { gauge } => tracing::warn!(
				volts = %volts(gauge),
				charge = %charge(gauge),
				"external power no longer reaches the backup supply"
			),
			Self::Restored { gauge, away } => tracing::info!(
				volts = %volts(gauge),
				charge = %charge(gauge),
				away_secs = ?secs(away),
				"external power reaches the backup supply again"
			),
			Self::Falling { gauge, away } => tracing::info!(
				volts = %volts(gauge),
				charge = %charge(gauge),
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

#[cfg(test)]
mod tests {
	use super::*;

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
}
