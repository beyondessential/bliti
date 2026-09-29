//! Time left, and time to full: the charge over the rate it has recently moved at (CHG, "Time
//! left").
//!
//! The rate is a least-squares line through the reported charge over the last [`WINDOW`] the cell
//! has spent going one way. The margin combines, in quadrature, the figure's own error, which is
//! off by the same share of a cell whatever the rate and so by `error / rate` in time, with how far
//! the rate itself may be off, the line's standard error, which moves the time by
//! `time * spread / rate`.

use std::time::Duration;

/// How far back the rate is taken over.
pub const WINDOW: Duration = Duration::from_secs(10 * 60);

/// How long the charge must have been watched going one way before it has a rate.
pub const SETTLED: Duration = Duration::from_secs(5 * 60);

/// How many standard errors the rate must stand clear of still before the charge is taken to be
/// moving at all.
const CLEAR: f64 = 2.0;

/// The fewest samples a rate is taken from.
const FEWEST: usize = 3;

/// How the charge has recently moved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trend {
	/// Reported charge per second: negative falling, positive rising.
	pub rate: f64,
	/// The rate's standard error, in the same unit.
	pub spread: f64,
}

/// A time, in seconds, and how far either way it may be off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
	pub seconds: f64,
	pub margin: f64,
}

/// The trend through `samples` of `(seconds, charge)`, where they span at least [`SETTLED`].
pub fn trend(samples: &[(f64, f64)]) -> Option<Trend> {
	if samples.len() < FEWEST {
		return None;
	}
	let (earliest, latest) = samples
		.iter()
		.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(t, _)| {
			(lo.min(t), hi.max(t))
		});
	if latest - earliest < SETTLED.as_secs_f64() {
		return None;
	}
	let n = samples.len() as f64;
	let mean_t = samples.iter().map(|&(t, _)| t).sum::<f64>() / n;
	let mean_q = samples.iter().map(|&(_, q)| q).sum::<f64>() / n;
	let (stt, stq) = samples.iter().fold((0.0, 0.0), |(stt, stq), &(t, q)| {
		let dt = t - mean_t;
		(stt + dt * dt, stq + dt * (q - mean_q))
	});
	let rate = stq / stt;
	let residual = samples
		.iter()
		.map(|&(t, q)| {
			let off = q - (mean_q + rate * (t - mean_t));
			off * off
		})
		.sum::<f64>();
	let spread = (residual / (n - 2.0) / stt).sqrt();
	Some(Trend { rate, spread })
}

/// How long until `charge` reaches 0, where it is falling clear of still. `error` is the figure's,
/// as a share of what is reported.
pub fn to_empty(charge: f64, trend: Trend, error: f64) -> Option<Estimate> {
	until(charge, -trend.rate, trend.spread, error)
}

/// How long until `charge` reaches 1, where it is rising clear of still.
pub fn to_full(charge: f64, trend: Trend, error: f64) -> Option<Estimate> {
	until(1.0 - charge, trend.rate, trend.spread, error)
}

/// How long `left` takes at `speed`, where the speed stands clear of its spread.
fn until(left: f64, speed: f64, spread: f64, error: f64) -> Option<Estimate> {
	if !(speed > 0.0 && speed >= CLEAR * spread) {
		return None;
	}
	let seconds = left.max(0.0) / speed;
	let figure = error / speed;
	let varied = seconds * spread / speed;
	Some(Estimate {
		seconds,
		margin: figure.hypot(varied),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Samples ten seconds apart over `secs`, the charge moving at `rate` from `from`, wobbling by
	/// `wobble` either way on alternate samples.
	fn line(secs: u32, from: f64, rate: f64, wobble: f64) -> Vec<(f64, f64)> {
		(0..=secs / 10)
			.map(|tick| {
				let t = f64::from(tick * 10);
				let sign = if tick % 2 == 0 { 1.0 } else { -1.0 };
				(t, from + rate * t + sign * wobble)
			})
			.collect()
	}

	fn close(a: f64, b: f64, within: f64) -> bool {
		(a - b).abs() <= within
	}

	#[test]
	fn a_steady_fall_has_its_rate_and_no_spread() {
		let trend = trend(&line(600, 0.8, -1e-4, 0.0)).unwrap();
		assert!(close(trend.rate, -1e-4, 1e-12), "{trend:?}");
		assert!(trend.spread < 1e-12, "{trend:?}");
	}

	/// No rate until the charge has been watched for five minutes (CHG).
	#[test]
	fn no_rate_until_watched_long_enough() {
		assert_eq!(trend(&line(290, 0.8, -1e-4, 0.0)), None);
		assert!(trend(&line(300, 0.8, -1e-4, 0.0)).is_some());
		assert_eq!(trend(&[(0.0, 0.5), (600.0, 0.4)]), None);
	}

	/// With the figure exact and the rate steady, the time is the charge over the rate and nothing
	/// either way.
	#[test]
	fn time_to_empty_is_the_charge_over_the_rate() {
		let trend = Trend {
			rate: -1e-4,
			spread: 0.0,
		};
		let estimate = to_empty(0.5, trend, 0.0).unwrap();
		assert!(close(estimate.seconds, 5000.0, 1e-6));
		assert_eq!(estimate.margin, 0.0);
	}

	#[test]
	fn time_to_full_is_what_is_left_over_the_rate() {
		let trend = Trend {
			rate: 2e-4,
			spread: 0.0,
		};
		let estimate = to_full(0.6, trend, 0.0).unwrap();
		assert!(close(estimate.seconds, 2000.0, 1e-6));
	}

	/// The figure's error is a share of a cell, so it is off by that share's time at the rate
	/// (CHG).
	#[test]
	fn the_figures_error_is_its_time_at_the_rate() {
		let trend = Trend {
			rate: -1e-4,
			spread: 0.0,
		};
		let estimate = to_empty(0.5, trend, 0.05).unwrap();
		assert!(close(estimate.margin, 500.0, 1e-6));
	}

	/// How far the rate may be off moves the time in proportion, and combines with the figure's
	/// error in quadrature (CHG).
	#[test]
	fn the_rates_variation_combines_with_the_figures_error() {
		let trend = Trend {
			rate: -1e-4,
			spread: 1e-5,
		};
		let varied = to_empty(0.5, trend, 0.0).unwrap();
		assert!(close(varied.margin, 500.0, 1e-6), "a tenth of 5000 s");
		let both = to_empty(0.5, trend, 0.03).unwrap();
		assert!(close(both.margin, 300f64.hypot(500.0), 1e-6));
	}

	/// A noisier charge has a wider margin, from the line's own spread.
	#[test]
	fn a_wobbling_charge_widens_the_margin() {
		let steady = trend(&line(600, 0.8, -1e-4, 0.0)).unwrap();
		let wobbling = trend(&line(600, 0.8, -1e-4, 0.002)).unwrap();
		assert!(close(wobbling.rate, -1e-4, 1e-5));
		let margin = |trend| to_empty(0.5, trend, 0.0).unwrap().margin;
		assert!(margin(wobbling) > margin(steady) + 10.0);
	}

	/// A charge going the other way, or not clear of still, has no time either way.
	#[test]
	fn no_time_where_the_charge_goes_the_other_way_or_nowhere() {
		let rising = Trend {
			rate: 1e-4,
			spread: 0.0,
		};
		assert_eq!(to_empty(0.5, rising, 0.1), None);
		let falling = Trend {
			rate: -1e-4,
			spread: 0.0,
		};
		assert_eq!(to_full(0.5, falling, 0.1), None);
		let still = Trend {
			rate: -1e-6,
			spread: 1e-5,
		};
		assert_eq!(to_empty(0.5, still, 0.1), None);
		let flat = trend(&line(600, 0.5, 0.0, 0.001)).unwrap();
		assert_eq!(to_empty(0.5, flat, 0.1), None);
		assert_eq!(to_full(0.5, flat, 0.1), None);
	}
}
