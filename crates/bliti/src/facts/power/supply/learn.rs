//! Refining the curves from a run on battery that reached the floor, and from a charge that began at
//! a known charge and finished (CHG, "Learning" and "Accuracy").
//!
//! The gauge measures voltage and nothing else, so time at the device's own draw stands in for
//! charge: a run or charge is taken to have moved the charge evenly in time between its two known
//! ends.
//!
//! A run's ends are where it began and the floor. A run from full began at 1; one from below full
//! began at the charge the curve gives for the voltage it began at, and teaches nothing above that.
//! It ends at the floor, not at 0: the curve's 0 is where the board gave out, below the floor, which
//! a run ended by a low-battery shutdown never reaches. So with the run beginning at charge `began`
//! at time 0 and the voltage first reaching the floor at time `end`, the charge the run showed at
//! time `t` is
//!
//! ```text
//! floor + (began - floor) * (end - t) / end
//! ```
//!
//! where `floor` is the curve's charge at the floor. What lies below the floor carries through from
//! the curve as it was, and the curve's `duration` is `end / (began - floor)`, the time the run took
//! over the share of the scale it covered.
//!
//! A charge begins at the discharging curve's figure where the run before it ended, `from`, and ends
//! at 1 once the charge has finished, so at time `t` of a charge taking `end` it showed
//! `from + (1 - from) * t / end`, and the charging curve's `duration` is `end / (1 - from)`.
//!
//! The observed curve is read off the voltage as smoothed: a rolling median over seventy seconds,
//! for the gauge's 1.25 mV steps and the sag under bursts of load, then the lowest voltage so far on
//! a run or the highest on a charge, so the voltage only ever moves one way. The charge at a voltage
//! is the charge shown when the smoothed voltage first reached it, as the shipped curve's tail was
//! derived (`shipped-curve.py`).
//!
//! Each run or charge is blended into the curve with a weight of `1 / (n + 1)` for a curve learnt
//! from `n`, the mean of all so far, until that falls to [`RECENT`]; from there older ones fade
//! exponentially, so a replaced cell's curve takes over within a few runs. The error measured against
//! the curve as it was, and the duration, are blended with the same weight, the duration's scaled by
//! the share of the scale between the floor and full the run or charge covered.

use std::time::Duration;

use super::super::curve::{Curve, Document, FLOOR_VOLTS, unmeasured_error};

#[cfg(test)]
mod tests;

/// The weight the newest run or charge keeps once a curve has been learnt from a few.
const RECENT: f64 = 0.4;

/// Spacing of the voltages a refined curve is resampled at.
const STEP_VOLTS: f64 = 0.05;

/// How many samples either side of each the rolling median takes in: seventy seconds at the record
/// thread's ten.
const MEDIAN_REACH: usize = 3;

/// The shortest run or charge that teaches anything.
const SHORTEST: Duration = Duration::from_secs(10 * 60);

/// The least voltage a run or charge must cross to teach anything.
const NARROWEST_VOLTS: f64 = 0.05;

/// The least share of the scale a run or charge must cover to teach anything.
const NARROWEST_CHARGE: f64 = 0.02;

/// Grid voltages closer than this to a curve's end, or to where a run began, are left out, so no two
/// points land at one voltage once rounded.
const CROWDED_VOLTS: f64 = 0.005;

/// Which curve was refined, for the report of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
	Discharging,
	Charging,
}

impl Which {
	pub fn name(self) -> &'static str {
		match self {
			Self::Discharging => "discharging",
			Self::Charging => "charging",
		}
	}

	pub fn curve(self, document: &Document) -> Option<&Curve> {
		match self {
			Self::Discharging => Some(document.discharging()),
			Self::Charging => document.charging(),
		}
	}
}

/// Why a run or charge taught nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Untaught {
	#[error("the run never reached the floor")]
	NoFloor,
	#[error("it was too short to learn from")]
	TooShort,
	#[error("it covered too little of the curve to learn from")]
	TooNarrow,
}

/// The weight a curve learnt from `learnt_from` gives the next run or charge.
pub fn weight(learnt_from: u32) -> f64 {
	(1.0 / (f64::from(learnt_from) + 1.0)).max(RECENT)
}

/// The discharging curve refined by a run on battery that ended in a low-battery shutdown, from its
/// samples as `(time since the run began, volts)`.
pub fn from_run(
	old: &Curve,
	from_full: bool,
	samples: &[(Duration, f64)],
) -> Result<Curve, Untaught> {
	let smoothed = smoothed(samples, f64::min);
	let &(start, began_volts) = smoothed.first().ok_or(Untaught::TooShort)?;
	let end = crossed(&smoothed, FLOOR_VOLTS, |volts, at| volts <= at).ok_or(Untaught::NoFloor)?;
	let span = end - start;
	let floor = old.charge_at(FLOOR_VOLTS);
	let began = if from_full {
		1.0
	} else {
		old.charge_at(began_volts)
	};
	if span < SHORTEST.as_secs_f64() {
		return Err(Untaught::TooShort);
	}
	if began_volts - FLOOR_VOLTS < NARROWEST_VOLTS || began - floor < NARROWEST_CHARGE {
		return Err(Untaught::TooNarrow);
	}
	let shown = |t: f64| floor + (began - floor) * (end - t) / span;
	let observed =
		|volts: f64| crossed(&smoothed, volts, |volts, at| volts <= at).map_or(began, shown);

	let measured = rms(smoothed
		.iter()
		.filter(|&&(t, _)| t <= end)
		.map(|&(t, volts)| old.charge_at(volts) - shown(t)));
	let w = weight(old.learnt_from);

	let top = old.points.last().map_or(began_volts, |&(volts, _)| volts);
	let (refined_to, top) = if from_full {
		let top = top.max(began_volts);
		(top, top)
	} else {
		(began_volts, top)
	};
	let mut points: Vec<_> = old
		.points
		.iter()
		.copied()
		.filter(|&(volts, _)| volts < FLOOR_VOLTS)
		.collect();
	points.extend(
		grid(FLOOR_VOLTS, refined_to)
			.map(|volts| (volts, blend(old.charge_at(volts), observed(volts), w))),
	);
	points.extend(
		old.points
			.iter()
			.copied()
			.filter(|&(volts, _)| volts > refined_to + CROWDED_VOLTS && volts <= top),
	);
	monotone(&mut points);

	let covered = ((began - floor) / (1.0 - floor)).min(1.0);
	Ok(Curve {
		points,
		learnt_from: old.learnt_from.saturating_add(1),
		error: blend(old.error, measured.unwrap_or(old.error), w).clamp(0.0, 1.0),
		duration: blend(old.duration, span / (began - floor), w * covered),
	})
}

/// The charging curve refined by a charge from `from` on the discharging curve's scale until it
/// finished at `end` into it, created where the device holds none. `floor` is the discharging
/// curve's charge at the floor.
pub fn from_charge(
	old: Option<&Curve>,
	from: f64,
	floor: f64,
	samples: &[(Duration, f64)],
	end: Duration,
) -> Result<Curve, Untaught> {
	let before_end: Vec<_> = samples.iter().copied().filter(|&(t, _)| t <= end).collect();
	let smoothed = smoothed(&before_end, f64::max);
	let (Some(&(start, low)), Some(&(_, high))) = (smoothed.first(), smoothed.last()) else {
		return Err(Untaught::TooShort);
	};
	let span = end.as_secs_f64() - start;
	if span < SHORTEST.as_secs_f64() {
		return Err(Untaught::TooShort);
	}
	if high - low < NARROWEST_VOLTS || 1.0 - from < NARROWEST_CHARGE {
		return Err(Untaught::TooNarrow);
	}
	let shown = |t: f64| from + (1.0 - from) * (t - start) / span;
	let observed = |volts: f64| crossed(&smoothed, volts, |volts, at| volts >= at).map(shown);
	let duration = span / (1.0 - from);

	let Some(old) = old else {
		let mut points: Vec<_> = grid(low, high)
			.filter_map(|volts| Some((volts, observed(volts)?)))
			.collect();
		monotone(&mut points);
		return Ok(Curve {
			points,
			learnt_from: 1,
			error: unmeasured_error(1),
			duration,
		});
	};

	let measured = rms(smoothed
		.iter()
		.filter(|&&(_, volts)| old.covers(volts))
		.map(|&(t, volts)| old.charge_at(volts) - shown(t)));
	let w = weight(old.learnt_from);
	let old_low = old.points.first().map_or(low, |&(volts, _)| volts);
	let old_high = old.points.last().map_or(high, |&(volts, _)| volts);
	let mut points: Vec<_> = grid(low.min(old_low), high.max(old_high))
		.filter_map(|volts| {
			let new = (low..=high)
				.contains(&volts)
				.then(|| observed(volts))
				.flatten();
			let was = old.covers(volts).then(|| old.charge_at(volts));
			match (was, new) {
				(Some(was), Some(new)) => Some((volts, blend(was, new, w))),
				(was, new) => Some((volts, was.or(new)?)),
			}
		})
		.collect();
	monotone(&mut points);

	let covered = ((1.0 - from) / (1.0 - floor)).min(1.0);
	Ok(Curve {
		points,
		learnt_from: old.learnt_from.saturating_add(1),
		error: blend(old.error, measured.unwrap_or(old.error), w).clamp(0.0, 1.0),
		duration: blend(old.duration, duration, w * covered),
	})
}

fn blend(old: f64, new: f64, w: f64) -> f64 {
	old * (1.0 - w) + new * w
}

/// The samples as `(seconds, volts)`, median-smoothed, then held to one direction by `hold`:
/// [`f64::min`] for a run's voltage, which only falls, and [`f64::max`] for a charge's.
fn smoothed(samples: &[(Duration, f64)], hold: fn(f64, f64) -> f64) -> Vec<(f64, f64)> {
	let mut held: Option<f64> = None;
	(0..samples.len())
		.map(|i| {
			let window =
				&samples[i.saturating_sub(MEDIAN_REACH)..(i + MEDIAN_REACH + 1).min(samples.len())];
			let mut volts: Vec<f64> = window.iter().map(|&(_, volts)| volts).collect();
			volts.sort_by(f64::total_cmp);
			let median = volts[volts.len() / 2];
			let volts = held.map_or(median, |held| hold(held, median));
			held = Some(volts);
			(samples[i].0.as_secs_f64(), volts)
		})
		.collect()
}

/// When the smoothed voltage first reached `volts`, interpolated between samples, where `reached`
/// says whether a voltage has.
fn crossed(smoothed: &[(f64, f64)], volts: f64, reached: impl Fn(f64, f64) -> bool) -> Option<f64> {
	let index = smoothed.iter().position(|&(_, v)| reached(v, volts))?;
	let (t1, v1) = smoothed[index];
	let Some(&(t0, v0)) = index.checked_sub(1).map(|before| &smoothed[before]) else {
		return Some(t1);
	};
	Some(t0 + (t1 - t0) * (v0 - volts) / (v0 - v1))
}

/// The root mean square of `differences`, where there are any.
fn rms(differences: impl Iterator<Item = f64>) -> Option<f64> {
	let (count, sum) = differences.fold((0u32, 0.0), |(count, sum), difference| {
		(count + 1, sum + difference * difference)
	});
	(count > 0).then(|| (sum / f64::from(count)).sqrt())
}

/// `low`, `high`, and the multiples of [`STEP_VOLTS`] between them.
fn grid(low: f64, high: f64) -> impl Iterator<Item = f64> {
	let first = (low / STEP_VOLTS).floor() as i64 + 1;
	let last = (high / STEP_VOLTS).ceil() as i64 - 1;
	let between = (first..=last)
		.map(|step| step as f64 * STEP_VOLTS)
		.filter(move |&volts| volts > low + CROWDED_VOLTS && volts < high - CROWDED_VOLTS);
	std::iter::once(low)
		.chain(between)
		.chain((high > low + CROWDED_VOLTS).then_some(high))
}

/// Keep the charge from falling as the voltage rises, and within a cell.
fn monotone(points: &mut [(f64, f64)]) {
	let mut highest = 0.0f64;
	for (_, charge) in points {
		highest = highest.max(charge.clamp(0.0, 1.0));
		*charge = highest;
	}
}
