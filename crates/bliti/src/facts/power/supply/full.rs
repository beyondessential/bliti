//! Telling a finished charge from the cell voltage (CHG, "What is reported").
//!
//! During constant-voltage charging the voltage already sits at termination with charge still going
//! in, so a voltage at termination and still is not yet full. What follows termination is: the
//! charger stops, and the voltage relaxes back and holds. Every constant here is a guess until a
//! charge on each board is logged (plan, "Hardware data still needed").

use std::time::{Duration, Instant};

/// The X1208 terminates at about 4.2 V. Set above the 4.1875 V a full cell has been seen resting at
/// on mains, so a rested cell is not taken for one still in constant-voltage charging.
pub const TERMINATION_VOLTS: f64 = 4.19;

/// How far below the highest voltage seen at termination the cell must fall to have relaxed: four
/// gauge steps, clear of the one-step wobble.
pub const RELAX_VOLTS: f64 = 0.005;

/// How far the cell may move and still be holding: two gauge steps.
pub const STILL_VOLTS: f64 = 0.0025;

/// How long the cell must hold, having relaxed.
pub const HOLD: Duration = Duration::from_secs(5 * 60);

/// A cell holding this high, below termination, is resting rather than taking charge: at constant
/// current it rises fastest near the top, so it cannot hold here for as long as [`HOLD`]. This is
/// what tells a full cell on a device started on mains, which never sees termination.
pub const RESTING_VOLTS: f64 = 4.15;

/// Where a charge has got to, fed only while external power reaches the board.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Full {
	/// Below termination, and not yet holding high enough to be resting.
	#[default]
	Charging,
	/// At termination: charging at constant voltage, or about to stop.
	AtTermination { peak: f64 },
	/// Below termination, and holding within `low..=high` since `since`. `relaxed` where it fell back
	/// from termination rather than being found there.
	Holding {
		since: Instant,
		low: f64,
		high: f64,
		relaxed: bool,
	},
	/// The charge has finished.
	Finished,
}

impl Full {
	/// Count a reading taken with external power present. True at the reading the charge finishes on.
	pub fn observe(&mut self, now: Instant, volts: f64) -> bool {
		let next = match *self {
			Self::Finished => return false,
			_ if volts >= TERMINATION_VOLTS => match *self {
				Self::AtTermination { peak } => Self::AtTermination {
					peak: peak.max(volts),
				},
				_ => Self::AtTermination { peak: volts },
			},
			Self::AtTermination { peak } if volts <= peak - RELAX_VOLTS => {
				Self::holding(now, volts, true)
			}
			Self::AtTermination { .. } => *self,
			Self::Holding {
				since,
				low,
				high,
				relaxed,
			} => {
				let (low, high) = (low.min(volts), high.max(volts));
				if high - low > STILL_VOLTS {
					// Still falling from termination, or rising again: hold from here.
					Self::holding(now, volts, relaxed)
				} else if now.duration_since(since) >= HOLD {
					Self::Finished
				} else {
					Self::Holding {
						since,
						low,
						high,
						relaxed,
					}
				}
			}
			Self::Charging if volts >= RESTING_VOLTS => Self::holding(now, volts, false),
			Self::Charging => Self::Charging,
		};
		// A cell found high that then drops out of the resting band was taking charge after all.
		let next = match next {
			Self::Holding { relaxed: false, .. } if volts < RESTING_VOLTS => Self::Charging,
			next => next,
		};
		*self = next;
		next == Self::Finished
	}

	pub fn finished(&self) -> bool {
		*self == Self::Finished
	}

	fn holding(now: Instant, volts: f64, relaxed: bool) -> Self {
		Self::Holding {
			since: now,
			low: volts,
			high: volts,
			relaxed,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const EVERY: Duration = Duration::from_secs(10);

	/// The tick a series of readings, ten seconds apart, finishes the charge on.
	fn finishes(volts: impl IntoIterator<Item = f64>) -> Option<usize> {
		let start = Instant::now();
		let mut full = Full::default();
		volts
			.into_iter()
			.enumerate()
			.find(|&(tick, volts)| full.observe(start + EVERY * tick as u32, volts))
			.map(|(tick, _)| tick)
	}

	fn repeat(volts: f64, ticks: usize) -> impl Iterator<Item = f64> {
		std::iter::repeat_n(volts, ticks)
	}

	/// Constant-voltage charging sits at termination and still, which is not yet full.
	#[test]
	fn still_at_termination_is_not_full() {
		assert_eq!(finishes(repeat(4.2, 1000)), None);
	}

	#[test]
	fn a_rising_cell_is_not_full() {
		assert_eq!(
			finishes((0..500).map(|tick| 3.7 + 0.001 * tick as f64)),
			None
		);
	}

	/// Termination, then the voltage relaxing back and holding, is a finished charge.
	#[test]
	fn relaxing_from_termination_and_holding_is_full() {
		let volts = repeat(4.2, 30)
			.chain([4.196, 4.193, 4.19, 4.189])
			.chain(repeat(4.1875, 100));
		// Holding from the first reading below termination (tick 33), five minutes on is tick 63.
		assert_eq!(finishes(volts), Some(63));
	}

	/// A cell still falling after termination has not settled.
	#[test]
	fn a_cell_still_falling_after_termination_is_not_full() {
		let volts = repeat(4.2, 30).chain((0..200).map(|tick| 4.19 - 0.0005 * tick as f64));
		assert_eq!(finishes(volts), None);
	}

	/// Falling back and rising to termination again is charging again, and the hold starts over.
	#[test]
	fn a_return_to_termination_starts_over() {
		let volts = repeat(4.2, 10)
			.chain(repeat(4.185, 20))
			.chain(repeat(4.2, 10))
			.chain(repeat(4.185, 100));
		assert_eq!(finishes(volts), Some(70));
	}

	/// A device started on mains with its cell already full never sees termination; the cell resting
	/// high and still is full.
	#[test]
	fn a_cell_resting_high_is_full() {
		assert_eq!(finishes(repeat(4.1875, 100)), Some(30));
	}

	/// Once finished, it stays so: nothing here clears it but external power going.
	#[test]
	fn finished_stays_finished() {
		let start = Instant::now();
		let mut full = Full::default();
		for tick in 0..31 {
			full.observe(start + EVERY * tick, 4.1875);
		}
		assert!(full.finished());
		assert!(!full.observe(start + EVERY * 40, 3.9));
		assert!(full.finished());
	}
}
