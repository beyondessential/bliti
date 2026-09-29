use super::*;

const EVERY: f64 = 10.0;

/// The gauge's voltage step.
const STEP: f64 = 0.00125;

fn curve(points: &[(f64, f64)], learnt_from: u32, error: f64, duration: f64) -> Curve {
	Curve {
		points: points.to_vec(),
		learnt_from,
		error,
		duration,
	}
}

/// The curve held: its floor at 2.8 V sits at 0.05.
fn held(learnt_from: u32, error: f64) -> Curve {
	curve(
		&[
			(2.6, 0.0),
			(2.8, 0.05),
			(3.2, 0.2),
			(3.6, 0.5),
			(3.9, 0.8),
			(4.2, 1.0),
		],
		learnt_from,
		error,
		20000.0,
	)
}

/// Another cell, fuller than [`held`] gives through the middle, with the same floor.
fn other() -> Curve {
	curve(
		&[
			(2.6, 0.0),
			(2.8, 0.05),
			(3.2, 0.3),
			(3.6, 0.6),
			(3.9, 0.85),
			(4.2, 1.0),
		],
		0,
		0.2,
		1.0,
	)
}

/// A cell following `truth` from charge `from`, drawn down across the whole scale in `duration`
/// seconds and looked at every ten, until it has been below the floor for a minute, as the
/// low-battery shutdown takes it. The voltage is in the gauge's steps, and sags under a burst of load
/// every sixth look.
fn run(truth: &Curve, duration: f64, from: f64) -> Vec<(Duration, f64)> {
	let mut samples = Vec::new();
	let mut below = 0;
	for tick in 0u32.. {
		let t = f64::from(tick) * EVERY;
		let mut volts = truth.volts_at((from - t / duration).max(0.0));
		if tick % 6 == 5 {
			volts -= 0.015;
		}
		let volts = (volts / STEP).round() * STEP;
		samples.push((Duration::from_secs_f64(t), volts));
		below = if volts < FLOOR_VOLTS { below + 1 } else { 0 };
		if below > 6 {
			break;
		}
	}
	samples
}

/// A charge following `truth` from `from` to full in `(1 - from) * duration` seconds, then the
/// voltage relaxing and holding as a finished charge does. Its samples, and when it reached full.
fn charge(truth: &Curve, duration: f64, from: f64) -> (Vec<(Duration, f64)>, Duration) {
	let end = (1.0 - from) * duration;
	let mut samples = Vec::new();
	for tick in 0u32.. {
		let t = f64::from(tick) * EVERY;
		let volts = if t <= end {
			truth.volts_at(from + t / duration)
		} else {
			4.1875
		};
		samples.push((Duration::from_secs_f64(t), (volts / STEP).round() * STEP));
		if t > end + 300.0 {
			break;
		}
	}
	(samples, Duration::from_secs_f64(end))
}

/// The most the two curves differ by, between the floor and 4.2 V.
fn apart(a: &Curve, b: &Curve) -> f64 {
	apart_within(a, b, FLOOR_VOLTS, 4.2)
}

fn apart_within(a: &Curve, b: &Curve, low: f64, high: f64) -> f64 {
	let steps = ((high - low) / 0.01).round() as u32;
	(0..=steps)
		.map(|step| low + f64::from(step) * 0.01)
		.map(|volts| (a.charge_at(volts) - b.charge_at(volts)).abs())
		.fold(0.0, f64::max)
}

fn valid(curve: &Curve) {
	Document::new(curve.clone(), None).unwrap();
}

/// How far `held` is off `truth` along a run from `from`, as the root mean square the learning
/// measures, worked out from the truth rather than the samples.
fn expected_error(held: &Curve, truth: &Curve, duration: f64, from: f64) -> f64 {
	let floor = truth.charge_at(FLOOR_VOLTS);
	let end = (from - floor) * duration;
	let differences: Vec<f64> = (0..)
		.map(|tick| f64::from(tick) * EVERY)
		.take_while(|&t| t <= end)
		.map(|t| {
			let q = from - t / duration;
			held.charge_at(truth.volts_at(q)) - q
		})
		.collect();
	(differences.iter().map(|d| d * d).sum::<f64>() / differences.len() as f64).sqrt()
}

/// A curve learnt from none takes its first run whole, and a run from full reshapes all of it and
/// sets how long the cell lasts (CHG).
#[test]
fn a_run_from_full_rescales_the_whole_curve_and_sets_the_duration() {
	let truth = other();
	let learnt = from_run(&held(0, 0.2), true, &run(&truth, 30000.0, 1.0)).unwrap();
	valid(&learnt);
	assert_eq!(learnt.learnt_from, 1);
	assert!(apart(&learnt, &truth) < 0.01, "{learnt:?}");
	assert!(
		(learnt.duration - 30000.0).abs() < 100.0,
		"{}",
		learnt.duration
	);
	assert_eq!(
		learnt.points[0],
		(2.6, 0.0),
		"below the floor carries through"
	);
	assert_eq!(learnt.points.last(), Some(&(4.2, 1.0)));
}

/// A run from below full is anchored at the curve's charge where it began, and leaves the curve
/// above that as it was (CHG).
#[test]
fn a_partial_run_refines_only_below_where_it_began() {
	let held = held(0, 0.2);
	let truth = other();
	let samples = run(&truth, 30000.0, 0.6);
	let learnt = from_run(&held, false, &samples).unwrap();
	valid(&learnt);

	let began = samples[2].1;
	assert!((began - 3.6).abs() < 0.01, "{began}");
	for &point in held.points.iter().filter(|&&(volts, _)| volts > began) {
		assert!(learnt.points.contains(&point), "{point:?} kept");
	}
	// Below, the other cell's shape, scaled from the floor up to where the held curve put the start.
	let anchor = held.charge_at(began);
	for volts in [2.9, 3.0, 3.2, 3.4, 3.5] {
		let scaled = 0.05 + (anchor - 0.05) * (truth.charge_at(volts) - 0.05) / (0.6 - 0.05);
		assert!(
			(learnt.charge_at(volts) - scaled).abs() < 0.01,
			"{volts} V: {} against {scaled}",
			learnt.charge_at(volts)
		);
	}
	// 16500 s over the 0.45 of the scale the held curve gives it, weighed by the 0.45 / 0.95 of the
	// scale it covered.
	let covered = (anchor - 0.05) / 0.95;
	let expected = 20000.0 * (1.0 - covered) + 16500.0 / (anchor - 0.05) * covered;
	assert!(
		(learnt.duration - expected).abs() < 100.0,
		"{} against {expected}",
		learnt.duration
	);
}

/// A replaced cell's curve takes over from the old one's within a few runs, however long the old one
/// was learnt for (CHG).
#[test]
fn a_replaced_cell_takes_over_within_a_few_runs() {
	let truth = other();
	let mut learnt = held(20, 0.05);
	let before = apart(&learnt, &truth);
	for runs in 1..=5 {
		learnt = from_run(&learnt, true, &run(&truth, 30000.0, 1.0)).unwrap();
		valid(&learnt);
		let now = apart(&learnt, &truth);
		if runs == 3 {
			assert!(now < before * 0.25, "{now} after three runs, from {before}");
		}
	}
	assert!(apart(&learnt, &truth) < before * 0.1);
	assert!((learnt.duration - 30000.0).abs() < 30000.0 * 0.1);
	assert_eq!(learnt.learnt_from, 25);
	assert!(learnt.points.len() < 40, "resampled, not grown");
}

/// The error is measured against the curve as it was before the run refined it, and weighed in as
/// the run is (CHG, "Accuracy").
#[test]
fn the_error_is_measured_before_refining_and_weighed_in() {
	let truth = other();
	let measured = expected_error(&held(0, 0.2), &truth, 30000.0, 1.0);
	assert!(measured > 0.03, "{measured}");

	let first = from_run(&held(0, 0.2), true, &run(&truth, 30000.0, 1.0)).unwrap();
	assert!(
		(first.error - measured).abs() < 0.005,
		"the first run is taken whole: {} against {measured}",
		first.error
	);

	let later = from_run(&held(20, 0.1), true, &run(&truth, 30000.0, 1.0)).unwrap();
	let expected = 0.6 * 0.1 + 0.4 * measured;
	assert!(
		(later.error - expected).abs() < 0.005,
		"{} against {expected}",
		later.error
	);

	// A run the curve already follows measures next to nothing, so the error falls.
	let held = held(20, 0.1);
	let exact = from_run(&held, true, &run(&held, 20000.0, 1.0)).unwrap();
	assert!((exact.error - 0.06).abs() < 0.005, "{}", exact.error);
}

#[test]
fn a_run_that_never_reached_the_floor_teaches_nothing() {
	let samples: Vec<_> = run(&other(), 30000.0, 1.0)
		.into_iter()
		.filter(|&(_, volts)| volts > 3.0)
		.collect();
	assert_eq!(
		from_run(&held(0, 0.2), true, &samples),
		Err(Untaught::NoFloor)
	);
}

#[test]
fn a_short_or_narrow_run_teaches_nothing() {
	// Five minutes from 3 V down past the floor.
	let short = run(&other(), 3000.0, 0.15);
	assert!(short.len() < 40, "{}", short.len());
	assert_eq!(
		from_run(&held(0, 0.2), false, &short),
		Err(Untaught::TooShort)
	);

	// Twenty minutes resting just above the floor, then below it.
	let narrow: Vec<_> = (0..130u32)
		.map(|tick| {
			let volts = if tick < 120 { 2.84 } else { 2.79 };
			(Duration::from_secs(u64::from(tick) * 10), volts)
		})
		.collect();
	assert_eq!(
		from_run(&held(0, 0.2), false, &narrow),
		Err(Untaught::TooNarrow)
	);

	assert_eq!(from_run(&held(0, 0.2), false, &[]), Err(Untaught::TooShort));
}

/// Whatever the run, what is learnt is a curve CRV accepts.
#[test]
fn the_refined_curve_is_always_valid() {
	let truths = [held(0, 0.2), other()];
	for truth in &truths {
		for from in [1.0, 0.9, 0.6, 0.3, 0.12] {
			for duration in [8000.0, 30000.0, 90000.0] {
				let mut learnt = held(3, 0.1);
				for _ in 0..3 {
					let samples = run(truth, duration, from);
					if let Ok(next) = from_run(&learnt, from == 1.0, &samples) {
						valid(&next);
						learnt = next;
					}
				}
			}
		}
	}
}

/// How the cell charges: the voltage reaches termination at 0.9 and sits there to full.
fn charging() -> Curve {
	curve(
		&[(3.3, 0.05), (3.6, 0.3), (3.9, 0.6), (4.1, 0.8), (4.2, 0.9)],
		0,
		0.2,
		1.0,
	)
}

/// The first charge from a known start creates the charging curve, over the voltages it covered
/// (CHG).
#[test]
fn the_first_charge_creates_the_charging_curve() {
	let truth = charging();
	let (samples, end) = charge(&truth, 20000.0, 0.1);
	let learnt = from_charge(None, 0.1, 0.05, &samples, end).unwrap();
	Document::new(held(0, 0.2), Some(learnt.clone())).unwrap();
	assert_eq!(learnt.learnt_from, 1);
	assert_eq!(learnt.error, unmeasured_error(1));
	assert!(
		(learnt.duration - 20000.0).abs() < 100.0,
		"{}",
		learnt.duration
	);
	let (low, _) = learnt.points[0];
	assert!((low - truth.volts_at(0.1)).abs() < 0.01, "{low}");
	for volts in [3.5, 3.7, 3.9, 4.0, 4.2] {
		assert!(
			(learnt.charge_at(volts) - truth.charge_at(volts)).abs() < 0.01,
			"{volts} V: {}",
			learnt.charge_at(volts)
		);
	}
}

/// A later charge refines the charging curve where it overlaps, keeps it where only the curve
/// reaches, and measures its error against the curve first (CHG).
#[test]
fn a_later_charge_refines_the_charging_curve() {
	let truth = charging();
	let (samples, end) = charge(&truth, 20000.0, 0.1);
	let first = from_charge(None, 0.1, 0.05, &samples, end).unwrap();

	let (samples, end) = charge(&truth, 20000.0, 0.4);
	let second = from_charge(Some(&first), 0.4, 0.05, &samples, end).unwrap();
	Document::new(held(0, 0.2), Some(second.clone())).unwrap();
	assert_eq!(second.learnt_from, 2);
	assert!(
		second.error < first.error * 0.6,
		"a charge the curve follows measures next to nothing: {}",
		second.error
	);
	let (low, _) = first.points[0];
	assert!(apart_within(&second, &truth, low, 4.2) < 0.01);
	assert_eq!(second.points[0], first.points[0], "kept below the start");
	assert!((second.duration - 20000.0).abs() < 200.0);
}

#[test]
fn a_short_or_narrow_charge_teaches_nothing() {
	let (samples, end) = charge(&charging(), 20000.0, 0.99);
	assert_eq!(
		from_charge(None, 0.99, 0.05, &samples, end),
		Err(Untaught::TooShort)
	);
	let still: Vec<_> = (0..200u32)
		.map(|tick| (Duration::from_secs(u64::from(tick) * 10), 4.0))
		.collect();
	assert_eq!(
		from_charge(None, 0.5, 0.05, &still, Duration::from_secs(1900)),
		Err(Untaught::TooNarrow)
	);
}

#[test]
fn the_weight_is_the_mean_until_it_reaches_the_recent_share() {
	assert_eq!(weight(0), 1.0);
	assert_eq!(weight(1), 0.5);
	assert_eq!(weight(2), RECENT);
	assert_eq!(weight(100), RECENT);
}
