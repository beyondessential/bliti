use std::{
	fs,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};

use super::{super::curve::Curve, *};

const EVERY: Duration = Duration::from_secs(10);
const UP: Option<Duration> = Some(Duration::from_secs(3600));

/// A directory of its own for one test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Self {
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, Ordering::Relaxed);
		let dir = std::env::temp_dir().join(format!("bliti-supply-{}-{n}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		Self(dir)
	}

	fn store(&self) -> Store {
		Store::new(self.0.join("battery-curve.json"))
	}

	fn supply(&self) -> Supply {
		Supply::new(self.store())
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

fn curve(points: &[(f64, f64)], learnt_from: u32) -> Curve {
	Curve {
		points: points.to_vec(),
		learnt_from,
		error: 0.1,
		duration: 1000.0,
	}
}

/// The floor at 2.8 V sits at 0.2; 3.5 V is halfway from there to full.
fn discharging() -> Curve {
	curve(&[(2.6, 0.0), (2.8, 0.2), (4.2, 1.0)], 0)
}

/// 3.9 V is halfway from the floor's charge to full.
fn document(charging_learnt: Option<u32>) -> Document {
	let charging = charging_learnt.map(|learnt| curve(&[(3.6, 0.2), (4.2, 1.0)], learnt));
	Document::new(discharging(), charging).unwrap()
}

fn stored(charging_learnt: Option<u32>, gauge_full: Option<f64>) -> Stored {
	Stored {
		document: document(charging_learnt),
		gauge_full,
	}
}

fn gauge(volts: f64, charge: f64) -> Gauge {
	Gauge { volts, charge }
}

fn look(volts: f64, external: bool) -> Look {
	Look {
		gauge: Ok(gauge(volts, 70.0)),
		external: Some(external),
	}
}

fn close(a: f64, b: f64) -> bool {
	(a - b).abs() < 1e-9
}

/// Feed `looks` ten seconds apart from `start`, returning what each called for.
fn feed(
	supply: &Supply,
	start: Instant,
	looks: impl IntoIterator<Item = Look>,
) -> Vec<Observed> {
	looks
		.into_iter()
		.enumerate()
		.map(|(tick, look)| supply.observe(look, start + EVERY * tick as u32, 1, UP))
		.collect()
}

fn reading(supply: &Supply) -> Reading {
	match supply.seen() {
		Seen::Gauge(reading) => reading,
		seen => panic!("no gauge reading: {seen:?}"),
	}
}

#[test]
fn off_mains_the_charge_is_the_discharging_curves() {
	let stored = stored(Some(5), Some(0.9));
	assert!(close(estimate(&stored, gauge(3.5, 70.0), Some(false), false), 0.5));
	assert_eq!(estimate(&stored, gauge(2.7, 70.0), Some(false), false), 0.0);
	assert_eq!(estimate(&stored, gauge(4.25, 70.0), Some(false), false), 1.0);
}

/// On mains with no charging curve learnt, the gauge's figure scaled to its full reading, unscaled
/// until one is known (CHG).
#[test]
fn on_mains_the_charge_is_the_gauge_scaled_to_full() {
	let unscaled = stored(None, None);
	assert!(close(estimate(&unscaled, gauge(4.0, 70.0), Some(true), false), 0.7));
	assert_eq!(estimate(&unscaled, gauge(4.1, 100.4), Some(true), false), 1.0);
	let scaled = stored(None, Some(0.9));
	assert!(close(estimate(&scaled, gauge(4.0, 81.0), Some(true), false), 0.9));
	assert_eq!(estimate(&scaled, gauge(4.1, 95.0), Some(true), false), 1.0);
}

/// The charging curve is read only once learnt from three charges, and only within what it covers
/// (CHG).
#[test]
fn the_charging_curve_is_read_once_learnt_and_covering() {
	let learnt = stored(Some(3), None);
	assert!(close(estimate(&learnt, gauge(3.9, 70.0), Some(true), false), 0.5));
	assert!(
		close(estimate(&learnt, gauge(3.5, 70.0), Some(true), false), 0.7),
		"below what the charging curve covers"
	);
	let young = stored(Some(2), None);
	assert!(close(estimate(&young, gauge(3.9, 70.0), Some(true), false), 0.7));
}

#[test]
fn a_finished_charge_is_full_whatever_else_gives() {
	let stored = stored(Some(3), Some(0.9));
	assert_eq!(estimate(&stored, gauge(3.9, 60.0), Some(true), true), 1.0);
}

/// Where the power line cannot be read, the charge is the discharging curve's.
#[test]
fn with_no_power_line_the_charge_is_the_discharging_curves() {
	let stored = stored(None, None);
	assert!(close(estimate(&stored, gauge(3.5, 70.0), None, false), 0.5));
}

/// Only a look where both the gauge and the power line answered counts towards a low battery, so a
/// machine with no backup board never arms the shutdown (LOW).
#[test]
fn only_the_gauge_and_the_line_together_arm_the_shutdown() {
	let floor = |gauge: Result<Gauge, Unanswered>, external| Look { gauge, external }.floor();
	assert_eq!(floor(Err(Unanswered::NoGauge), None), None);
	assert_eq!(floor(Err(Unanswered::NoGauge), Some(false)), None);
	assert_eq!(
		floor(Err(Unanswered::Failed("timed out".to_owned())), Some(false)),
		None
	);
	assert_eq!(floor(Ok(gauge(2.7, 0.0)), None), None);
	assert_eq!(
		floor(Ok(gauge(2.7, 0.0)), Some(false)),
		Some(Floor {
			volts: 2.7,
			external: false
		})
	);
}

/// A gauge below the floor with no power line to read never holds the floor.
#[test]
fn a_gauge_with_no_power_line_never_powers_off() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let looks = (0..100).map(|_| Look {
		gauge: Ok(gauge(2.7, 0.0)),
		external: None,
	});
	assert!(feed(&supply, Instant::now(), looks).iter().all(|o| o.held.is_none()));
}

/// With no gauge there is no backup supply: nothing to hold, nothing loaded, and no curve to load
/// or reset (CRV, LOW).
#[test]
fn with_no_gauge_nothing_is_managed() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let looks = (0..30).map(|_| Look {
		gauge: Err(Unanswered::NoGauge),
		external: None,
	});
	assert!(
		feed(&supply, Instant::now(), looks)
			.iter()
			.all(|observed| *observed == Observed::default())
	);
	assert_eq!(supply.seen(), Seen::NoGauge);
	assert!(!supply.managed());
	assert_eq!(supply.document(), None);
	assert_eq!(*supply.curves().borrow(), None);
	assert!(matches!(
		supply.load(Document::shipped()),
		Err(Unchanged::NotManaged)
	));
	assert!(matches!(supply.reset(), Err(Unchanged::NotManaged)));
	assert!(!scratch.0.exists(), "no curve file is written");
}

/// The curve file is read the first time the gauge answers, and its document goes to every curve
/// stream.
#[test]
fn the_curves_are_loaded_when_the_gauge_first_answers() {
	let scratch = Scratch::new();
	scratch.store().save(&stored(Some(3), Some(0.95))).unwrap();
	let supply = scratch.supply();
	let curves = supply.curves();
	assert!(!supply.managed());
	feed(&supply, Instant::now(), [look(3.9, true)]);
	assert!(supply.managed());
	assert_eq!(supply.document(), Some(document(Some(3))));
	assert_eq!(*curves.borrow(), Some(document(Some(3))));
	assert!(close(reading(&supply).charge, 0.5), "the loaded charging curve is read");
}

/// A load is saved, put in force and sent; a reset returns to the shipped curve, keeping the gauge's
/// full reading, which is the device's own (CRV).
#[test]
fn a_load_and_a_reset_are_saved_and_sent() {
	let scratch = Scratch::new();
	scratch.store().save(&stored(None, Some(0.95))).unwrap();
	let supply = scratch.supply();
	feed(&supply, Instant::now(), [look(3.9, true)]);
	let mut curves = supply.curves();
	curves.borrow_and_update();

	supply.load(document(Some(4))).unwrap();
	assert!(curves.has_changed().unwrap());
	assert_eq!(*curves.borrow_and_update(), Some(document(Some(4))));
	assert_eq!(supply.document(), Some(document(Some(4))));
	assert_eq!(scratch.store().read().unwrap(), Some(stored(Some(4), Some(0.95))));

	supply.reset().unwrap();
	assert_eq!(*curves.borrow_and_update(), Some(Document::shipped()));
	assert_eq!(
		scratch.store().read().unwrap(),
		Some(Stored {
			document: Document::shipped(),
			gauge_full: Some(0.95)
		})
	);
}

/// A load that cannot be saved is not put in force (CRV: accepted only where done).
#[test]
fn a_load_that_cannot_be_saved_changes_nothing() {
	let scratch = Scratch::new();
	fs::create_dir_all(&scratch.0).unwrap();
	// A file where the curve file's directory would be.
	let blocked = scratch.0.join("blocked");
	fs::write(&blocked, "").unwrap();
	let supply = Supply::new(Store::new(blocked.join("battery-curve.json")));
	feed(&supply, Instant::now(), [look(3.9, true)]);
	let curves = supply.curves();
	assert!(matches!(
		supply.load(document(Some(4))),
		Err(Unchanged::Save(_))
	));
	assert_eq!(supply.document(), Some(Document::shipped()));
	assert!(!curves.has_changed().unwrap());
}

/// A finished charge reports full and idle until the cell next carries the device, and records the
/// gauge's reading as its full one (CHG).
#[test]
fn a_finished_charge_is_full_until_the_cell_carries_the_device() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let start = Instant::now();
	let resting = (0..31).map(|_| Look {
		gauge: Ok(gauge(4.1875, 96.0)),
		external: Some(true),
	});
	feed(&supply, start, resting);
	let full = reading(&supply);
	assert!(full.full);
	assert_eq!(full.charge, 1.0);
	assert_eq!(
		scratch.store().read().unwrap().unwrap().gauge_full,
		Some(0.96)
	);

	supply.observe(look(4.15, false), start + EVERY * 31, 1, UP);
	let on_battery = reading(&supply);
	assert!(!on_battery.full);
	assert!(on_battery.charge < 1.0);

	// Back on mains, the gauge is scaled to the full reading recorded: 70 % of a gauge reading 96 %.
	supply.observe(look(4.0, true), start + EVERY * 32, 1, UP);
	assert!(close(reading(&supply).charge, 0.7 / 0.96));
}

/// Sixty seconds below the floor with external power absent is held, carrying the voltage and how
/// long external power has been away (LOW).
#[test]
fn the_floor_held_carries_the_voltage_and_the_time_away() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let looks = std::iter::once(look(3.0, true)).chain((0..10).map(|_| look(2.79, false)));
	let observed = feed(&supply, Instant::now(), looks);
	let held: Vec<_> = observed
		.iter()
		.enumerate()
		.filter_map(|(tick, observed)| observed.held.map(|held| (tick, held)))
		.collect();
	assert_eq!(
		held.first(),
		Some(&(
			7,
			Held {
				volts: 2.79,
				away: Some(EVERY * 6)
			}
		))
	);
}

/// A run on a device that starts on battery has no known time away.
#[test]
fn the_floor_held_from_a_start_on_battery_has_no_time_away() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let observed = feed(&supply, Instant::now(), (0..7).map(|_| look(2.79, false)));
	assert_eq!(
		observed[6].held,
		Some(Held {
			volts: 2.79,
			away: None
		})
	);
}

/// A run is recorded from external power going, or from start on battery; a charge from external
/// power returning, anchored at the discharging curve's charge where the run ended (CHG).
#[test]
fn runs_and_charges_are_recorded_as_power_comes_and_goes() {
	let scratch = Scratch::new();
	scratch.store().save(&stored(None, None)).unwrap();
	let supply = scratch.supply();
	let start = Instant::now();
	feed(
		&supply,
		start,
		[look(3.7, false), look(3.69, false), look(3.68, false)],
	);
	{
		let state = supply.state();
		let run = state.run.as_ref().unwrap();
		assert!(!run.from_full);
		assert_eq!(run.recording.began, start);
		let volts: Vec<_> = run.recording.samples.iter().map(|&(_, v)| v).collect();
		assert_eq!(volts, [3.7, 3.69, 3.68]);
		assert_eq!(run.recording.samples[2].0, EVERY * 2);
	}

	supply.observe(look(3.8, true), start + EVERY * 3, 1, UP);
	{
		let state = supply.state();
		assert!(state.run.is_none(), "a run not ending in a shutdown is dropped");
		let charge = state.charge.as_ref().unwrap();
		assert!(close(charge.from, discharging().charge_at(3.68)));
		assert_eq!(charge.recording.samples, [(Duration::ZERO, 3.8)]);
	}

	supply.observe(look(3.75, false), start + EVERY * 4, 1, UP);
	let state = supply.state();
	assert!(state.charge.is_none(), "a charge cut short is dropped");
	assert!(state.run.is_some());
}

/// A run begun from a finished charge is marked as from full, which refines the whole curve (CHG).
#[test]
fn a_run_from_a_finished_charge_is_from_full() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let start = Instant::now();
	feed(&supply, start, (0..31).map(|_| look(4.1875, true)));
	supply.observe(look(4.15, false), start + EVERY * 31, 1, UP);
	assert!(supply.state().run.as_ref().unwrap().from_full);
}

/// The history is held for minutes, not forever, and the sampler sees only the last [`WATCH`] of it.
#[test]
fn the_history_is_bounded_and_the_recent_window_short() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	feed(&supply, Instant::now(), (0..400).map(|_| look(3.9, true)));
	let state = supply.state();
	assert_eq!(state.history.len(), 181);
	assert!(state.history.iter().all(|sample| close(sample.charge, 0.7)));
	assert_eq!(reading_of(&state).recent.watched(), EVERY * 4);
}

fn reading_of(state: &State) -> Reading {
	match &state.seen {
		Seen::Gauge(reading) => reading.clone(),
		seen => panic!("no gauge reading: {seen:?}"),
	}
}

/// A gauge that stops answering is broken rather than gone, and holds no floor.
#[test]
fn a_gauge_that_fails_is_reported_and_restarts_the_count() {
	let scratch = Scratch::new();
	let supply = scratch.supply();
	let start = Instant::now();
	let looks = (0..12).map(|tick| {
		if tick == 5 {
			Look {
				gauge: Err(Unanswered::Failed("timed out".to_owned())),
				external: None,
			}
		} else {
			look(2.79, false)
		}
	});
	let observed = feed(&supply, start, looks);
	let first = observed.iter().position(|observed| observed.held.is_some());
	assert_eq!(first, None, "counted again from the failed read");
	supply.observe(
		Look {
			gauge: Err(Unanswered::Failed("timed out".to_owned())),
			external: None,
		},
		start + EVERY * 12,
		42,
		UP,
	);
	assert_eq!(
		supply.seen(),
		Seen::Unanswered {
			at: 42,
			reason: "timed out".to_owned()
		}
	);
}
