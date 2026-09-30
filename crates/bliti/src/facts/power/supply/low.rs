//! Confirming a low battery before powering off for it (LOW, "Shutting down").

use std::time::{Duration, Instant};

use super::super::curve::FLOOR_VOLTS;

/// How long every reading must have been below the floor, with external power absent (LOW).
pub const CONFIRM: Duration = Duration::from_secs(60);

/// How long after the system starts no shutdown begins, read from the system's boot rather than
/// bliti's start (LOW).
pub const BOOT_GRACE: Duration = Duration::from_secs(120);

/// One look at the supply, as far as a low battery goes: the cell voltage and whether external power
/// reaches the board. Only a look where both the gauge and the power line answered gives one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Floor {
	pub volts: f64,
	pub external: bool,
}

/// The confirmation clock.
#[derive(Debug, Default)]
pub struct Low {
	/// Since when every reading has been below the floor with external power absent.
	since: Option<Instant>,
}

impl Low {
	/// Count one look, where `floor` is `None` for a look that could not be taken or a supply that
	/// is not ours to watch, and `uptime` is how long the system has been up, where known. True where
	/// the floor has now been held long enough to power off.
	pub fn observe(
		&mut self,
		now: Instant,
		uptime: Option<Duration>,
		floor: Option<Floor>,
	) -> bool {
		match floor {
			Some(Floor {
				volts,
				external: false,
			}) if volts < FLOOR_VOLTS => {
				let since = *self.since.get_or_insert(now);
				// A boot time that cannot be read does not hold the shutdown off: the cell matters more.
				now.duration_since(since) >= CONFIRM && uptime.is_none_or(|up| up >= BOOT_GRACE)
			}
			_ => {
				self.since = None;
				false
			}
		}
	}

	/// Powering off has been asked for, whatever came of it: the count starts again. The controller
	/// refuses a second shutdown while one is under way, so asking again after another sixty seconds
	/// begins one only where the last could not be carried out, and a device that cannot power off
	/// says so each sixty seconds rather than at every reading (LOW).
	pub fn settle(&mut self) {
		self.since = None;
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const EVERY: Duration = Duration::from_secs(10);
	const UP: Option<Duration> = Some(Duration::from_secs(3600));

	fn below() -> Option<Floor> {
		Some(Floor {
			volts: 2.79,
			external: false,
		})
	}

	/// Feed `floor` every ten seconds from `start` for `ticks` looks; the tick it first held on.
	fn held_after(
		low: &mut Low,
		start: Instant,
		ticks: u32,
		uptime: impl Fn(u32) -> Option<Duration>,
		floor: impl Fn(u32) -> Option<Floor>,
	) -> Option<u32> {
		(0..ticks).find(|&tick| low.observe(start + EVERY * tick, uptime(tick), floor(tick)))
	}

	#[test]
	fn sixty_seconds_below_the_floor_confirms() {
		let mut low = Low::default();
		let held = held_after(&mut low, Instant::now(), 20, |_| UP, |_| below());
		// The first reading starts the count; the seventh is sixty seconds on.
		assert_eq!(held, Some(6));
	}

	#[test]
	fn a_reading_at_the_floor_starts_the_count_again() {
		let mut low = Low::default();
		let held = held_after(
			&mut low,
			Instant::now(),
			20,
			|_| UP,
			|tick| {
				if tick == 4 {
					Some(Floor {
						volts: FLOOR_VOLTS,
						external: false,
					})
				} else {
					below()
				}
			},
		);
		assert_eq!(held, Some(11));
	}

	#[test]
	fn a_failed_reading_starts_the_count_again() {
		let mut low = Low::default();
		let held = held_after(
			&mut low,
			Instant::now(),
			20,
			|_| UP,
			|tick| if tick == 5 { None } else { below() },
		);
		assert_eq!(held, Some(12));
	}

	#[test]
	fn external_power_returning_starts_the_count_again() {
		let mut low = Low::default();
		let held = held_after(
			&mut low,
			Instant::now(),
			20,
			|_| UP,
			|tick| {
				if tick == 3 {
					Some(Floor {
						volts: 2.79,
						external: true,
					})
				} else {
					below()
				}
			},
		);
		assert_eq!(held, Some(10));
	}

	/// Nothing begins within two minutes of the system starting, however long the floor was held.
	#[test]
	fn nothing_begins_within_two_minutes_of_boot() {
		let mut low = Low::default();
		// Up for ten seconds at the first look: the floor is held from sixty on, the grace ends at 120.
		let held = held_after(
			&mut low,
			Instant::now(),
			30,
			|tick| Some(Duration::from_secs(10) + EVERY * tick),
			|_| below(),
		);
		assert_eq!(held, Some(11));
	}

	/// Once powering off has been asked for, the floor must be held another sixty seconds before it
	/// is asked for again, not at every reading (LOW).
	#[test]
	fn an_attempt_counts_sixty_seconds_again() {
		let mut low = Low::default();
		let start = Instant::now();
		let mut held = Vec::new();
		for tick in 0..20 {
			if low.observe(start + EVERY * tick, UP, below()) {
				held.push(tick);
				low.settle();
			}
		}
		assert_eq!(held, vec![6, 13]);
	}
}
