//! The one state of the backup supply: what the record thread last saw of it, the history behind
//! that, and the curves its charge is read from (CHG, LOW).
//!
//! The record thread looks every ten seconds whether or not the device is sampling, and is the only
//! writer of what it sees. The sampler reports from here rather than keeping a history of its own,
//! and the curve stream reads and replaces the curves through the same [`Supply`], so a curve changed
//! by one is the curve in force for all.

use std::{
	collections::VecDeque,
	sync::{Arc, Mutex, MutexGuard, PoisonError},
	time::{Duration, Instant},
};

use tokio::sync::watch;

use self::{
	full::Full,
	learn::Which,
	low::{Floor, Low},
	rate::Trend,
};
use super::{
	Gauge, Recent, WATCH,
	curve::{
		Curve, Document, Invalid,
		store::{DEFAULT_PATH, Store, StoreError, Stored},
		unmeasured_error,
	},
	record::{Event, Record},
};

mod full;
mod learn;
mod low;
pub mod rate;
#[cfg(test)]
mod tests;

/// How far back the cell's voltage and charge are held: minutes, for the rates time left is taken
/// from (CHG, "Time left").
const HISTORY: Duration = Duration::from_secs(30 * 60);

/// How many charges a charging curve must have been learnt from before it is read (CHG).
const CHARGING_LEARNT: u32 = 3;

/// The most samples a run or charge being recorded holds: a day at ten seconds apart, far longer than
/// the cell lasts. Longer than that, the cell is not carrying the device or taking charge at all, and
/// the recording is dropped as teaching nothing.
const RECORDED: usize = 24 * 60 * 6;

/// The backup supply, shared by the record thread, the sampler and the curve stream.
///
/// Where no gauge has answered, as on a machine with no backup board, it manages no backup supply:
/// it holds no curves, and refuses to load or reset them (CRV).
#[derive(Debug, Clone)]
pub struct Supply {
	state: Arc<Mutex<State>>,
}

/// Why the curves were left as they were.
#[derive(Debug, thiserror::Error)]
pub enum Unchanged {
	#[error("this device manages no backup supply")]
	NotManaged,
	#[error("the curves could not be saved: {0}")]
	Save(#[source] StoreError),
}

/// One look at the supply.
#[derive(Debug)]
pub struct Look {
	pub gauge: Result<Gauge, Unanswered>,
	/// Whether external power reaches the board, where the power line could be read. Read only once
	/// the gauge has answered, since an unconnected pin reads as external power present.
	pub external: Option<bool>,
}

/// Why the gauge gave nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Unanswered {
	/// No gauge is here: there is no backup board.
	NoGauge,
	/// The bus is there and the gauge did not answer, with why.
	Failed(String),
}

/// What the last look found, for the sampler to report (NFO).
#[derive(Debug, Clone, PartialEq)]
pub enum Seen {
	/// Nothing has been looked at yet.
	Nothing,
	NoGauge,
	Unanswered {
		at: u64,
		reason: String,
	},
	Gauge(Reading),
}

/// The supply as a look with an answering gauge found it.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
	/// When it was taken, in milliseconds since boot.
	pub at: u64,
	pub gauge: Gauge,
	pub external: Option<bool>,
	/// The charge as CHG estimates it, from 0 at the floor to 1 at full.
	pub charge: f64,
	/// Whether the charge has finished since the cell last carried the device (CHG).
	pub full: bool,
	/// The cell voltage over the last [`WATCH`].
	pub recent: Recent,
	/// How the charge has recently moved, where it has been watched going one way long enough.
	pub trend: Option<Trend>,
	/// How far the charge may be off, as a share of what is reported (CHG, "Accuracy").
	pub error: f64,
}

/// What a look calls for from the record thread.
#[derive(Debug, Default, PartialEq)]
pub struct Observed {
	/// A change to report on standard error, with the charge estimated at the time (DEV).
	pub report: Option<(Event, f64)>,
	/// The floor has been held long enough to power off (LOW).
	pub held: Option<Held>,
}

/// The floor held, as reported when powering off for it (LOW).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Held {
	pub volts: f64,
	/// How long external power has been absent, where the device saw it go.
	pub away: Option<Duration>,
}

#[derive(Debug)]
struct State {
	store: Store,
	/// The curves and the gauge's full reading, loaded the first time the gauge answers.
	stored: Option<Stored>,
	/// The curve document in force, to every curve stream.
	curves: watch::Sender<Option<Document>>,
	seen: Seen,
	history: VecDeque<Sample>,
	record: Record,
	run: Option<Run>,
	charge: Option<Charge>,
	full: Full,
	low: Low,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Sample {
	at: Instant,
	volts: f64,
	charge: f64,
	external: Option<bool>,
}

/// Timed cell voltages, from when the recording began.
#[derive(Debug, Clone, PartialEq)]
struct Recording {
	began: Instant,
	samples: Vec<(Duration, f64)>,
}

/// A run on battery, recorded from when external power went, or from start where it was already
/// absent, for the discharging curve to be refined from (CHG, "Learning").
#[derive(Debug, Clone, PartialEq)]
struct Run {
	/// Whether it began from a full cell, which refines the whole curve rather than below its start.
	from_full: bool,
	recording: Recording,
}

/// A charge from a known start, recorded from when external power returned to a device on battery,
/// for the charging curve to be learnt from (CHG, "Learning").
#[derive(Debug, Clone, PartialEq)]
struct Charge {
	/// The discharging curve's charge where it began, on the curves' own scale.
	from: f64,
	recording: Recording,
}

impl Default for Supply {
	fn default() -> Self {
		Self::new(Store::new(DEFAULT_PATH))
	}
}

impl Supply {
	/// A supply whose curves are kept in `store`, read the first time a gauge answers.
	pub fn new(store: Store) -> Self {
		Self {
			state: Arc::new(Mutex::new(State {
				store,
				stored: None,
				curves: watch::Sender::new(None),
				seen: Seen::Nothing,
				history: VecDeque::new(),
				record: Record::default(),
				run: None,
				charge: None,
				full: Full::default(),
				low: Low::default(),
			})),
		}
	}

	/// A supply managing a backup supply from the start, as one whose gauge has answered.
	#[cfg(test)]
	pub fn managed_for_test(store: Store) -> Self {
		let supply = Self::new(store);
		supply.state().manage();
		supply
	}

	fn state(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// What the last look found.
	pub fn seen(&self) -> Seen {
		self.state().seen.clone()
	}

	/// Whether this device manages a backup supply: whether a gauge has answered.
	pub fn managed(&self) -> bool {
		self.state().stored.is_some()
	}

	/// The curve document in force, where a backup supply is managed.
	#[cfg(test)]
	pub fn document(&self) -> Option<Document> {
		self.state()
			.stored
			.as_ref()
			.map(|stored| stored.document.clone())
	}

	/// The curve document in force, as it changes: by a load, a reset or a refinement, and from
	/// nothing once a gauge first answers (CRV).
	pub fn curves(&self) -> watch::Receiver<Option<Document>> {
		self.state().curves.subscribe()
	}

	/// Replace both curves with `document`'s, saved before they take effect (CRV, "Loading and
	/// resetting"). Blocking, since it writes the curve file.
	pub fn load(&self, document: Document) -> Result<(), Unchanged> {
		self.state().replace(document)
	}

	/// Return to the curve the build carries, holding no charging curve (CRV). Blocking, as
	/// [`Supply::load`].
	pub fn reset(&self) -> Result<(), Unchanged> {
		self.state().replace(Document::shipped())
	}

	/// Take in one look at the supply. `at` is milliseconds since boot, and `uptime` how long the
	/// system has been up, where it can be read.
	pub fn observe(&self, look: Look, now: Instant, at: u64, uptime: Option<Duration>) -> Observed {
		self.state().observe(look, now, at, uptime)
	}

	/// Record what the run on battery taught, before a low-battery shutdown goes away (CHG).
	pub fn learn_from_run(&self) {
		self.state().learn_from_run();
	}

	/// Powering off for a low battery has been asked for, whatever came of it (LOW).
	pub fn settle(&self) {
		self.state().low.settle();
	}
}

impl Look {
	/// What the low-battery watch counts: only a look where the gauge and the power line both
	/// answered. A machine with no backup board, or a board with no power line, never gives one, so
	/// its battery stays with the operating system's power management (LOW).
	pub fn floor(&self) -> Option<Floor> {
		match (&self.gauge, self.external) {
			(Ok(gauge), Some(external)) => Some(Floor {
				volts: gauge.volts,
				external,
			}),
			_ => None,
		}
	}
}

impl State {
	fn observe(&mut self, look: Look, now: Instant, at: u64, uptime: Option<Duration>) -> Observed {
		let floor = look.floor();
		let gauge = match look.gauge {
			Ok(gauge) => gauge,
			Err(unanswered) => {
				self.seen = match unanswered {
					Unanswered::NoGauge => {
						self.history.clear();
						Seen::NoGauge
					}
					Unanswered::Failed(reason) => Seen::Unanswered { at, reason },
				};
				self.low.observe(now, uptime, None);
				return Observed::default();
			}
		};
		let external = look.external;
		self.manage();

		let was_full = self.full.finished();
		match external {
			Some(true) => {
				if self.full.observe(now, gauge.volts) {
					self.charge_finished(now, gauge);
				}
			}
			// The cell carries the device again, so it is no longer known full (CHG).
			Some(false) => self.full = Full::default(),
			None => {}
		}
		let full = self.full.finished();
		let (charge, error) = self.stored.as_ref().map_or((0.0, 1.0), |stored| {
			(
				estimate(stored, gauge, external, full),
				figure_error(stored, gauge, external),
			)
		});

		self.history.push_back(Sample {
			at: now,
			volts: gauge.volts,
			charge,
			external,
		});
		while self
			.history
			.front()
			.is_some_and(|sample| now.duration_since(sample.at) > HISTORY)
		{
			self.history.pop_front();
		}

		let event = external.and_then(|external| self.record.observe(now, external, gauge));
		self.follow(now, event.as_ref(), external, gauge.volts, was_full);

		self.seen = Seen::Gauge(Reading {
			at,
			gauge,
			external,
			charge,
			full,
			recent: self.recent(),
			trend: self.trend(),
			error,
		});
		let held = self.low.observe(now, uptime, floor).then(|| Held {
			volts: gauge.volts,
			away: self.record.away(now),
		});
		Observed {
			report: event.map(|event| (event, charge)),
			held,
		}
	}

	/// Load the curves from the store the first time a gauge answers.
	fn manage(&mut self) {
		if self.stored.is_none() {
			let stored = self.store.load();
			self.curves.send_replace(Some(stored.document.clone()));
			self.stored = Some(stored);
		}
	}

	/// Start, extend and drop the run and charge being recorded, as external power comes and goes.
	fn follow(
		&mut self,
		now: Instant,
		event: Option<&Event>,
		external: Option<bool>,
		volts: f64,
		was_full: bool,
	) {
		match event {
			Some(Event::Found {
				external: false, ..
			}) => {
				self.run = Some(Run {
					from_full: false,
					recording: Recording::new(now),
				});
			}
			Some(Event::Lost { .. }) => {
				// A charge cut short never reached full, and teaches nothing.
				self.charge = None;
				self.run = Some(Run {
					from_full: was_full,
					recording: Recording::new(now),
				});
			}
			Some(Event::Restored { .. }) => {
				// A run that did not end in a shutdown teaches nothing (CHG). Where it ended is the
				// known start the charge is anchored at, read before the charge current lifts it.
				let from = self
					.run
					.take()
					.and_then(|run| run.recording.last())
					.unwrap_or(volts);
				let from = self
					.stored
					.as_ref()
					.map_or(0.0, |stored| stored.document.discharging().charge_at(from));
				self.charge = Some(Charge {
					from,
					recording: Recording::new(now),
				});
			}
			_ => {}
		}
		match external {
			Some(false) => {
				if let Some(run) = &mut self.run
					&& !run.recording.push(now, volts)
				{
					self.run = None;
				}
			}
			Some(true) => {
				if let Some(charge) = &mut self.charge
					&& !charge.recording.push(now, volts)
				{
					self.charge = None;
				}
			}
			None => {}
		}
	}

	/// A finished charge: the gauge's reading now is its reading on a full cell (CHG).
	fn charge_finished(&mut self, now: Instant, gauge: Gauge) {
		let share = gauge.charge / 100.0;
		tracing::info!(
			volts = %format!("{:.4}", gauge.volts),
			gauge_charge = %format!("{:.1}", gauge.charge),
			"the backup supply has finished charging the cell"
		);
		if let Some(stored) = &mut self.stored
			&& share.is_finite()
			&& share > 0.0
		{
			stored.gauge_full = Some(share);
			if let Err(err) = self.store.save(stored) {
				tracing::warn!(%err, "the gauge's full reading could not be saved");
			}
		}
		if let Some(charge) = self.charge.take() {
			self.learn_from_charge(now, charge);
		}
	}

	/// A charge from a known start has reached full: refine the charging curve from it, creating it
	/// where none is held (CHG, "Learning").
	///
	/// The charge is taken to have finished where the cell began the hold the finished charge was
	/// told by, [`full::HOLD`] before now.
	fn learn_from_charge(&mut self, now: Instant, charge: Charge) {
		let Some(stored) = &self.stored else {
			return;
		};
		let document = &stored.document;
		let end = now
			.duration_since(charge.recording.began)
			.saturating_sub(full::HOLD);
		let learnt = learn::from_charge(
			document.charging(),
			charge.from,
			document.floor_charge(),
			&charge.recording.samples,
			end,
		);
		match learnt {
			Ok(curve) => {
				let refined = Document::new(document.discharging().clone(), Some(curve));
				self.refine(Which::Charging, refined);
			}
			Err(why) => tracing::info!(%why, "the charge taught the charging curve nothing"),
		}
	}

	/// A run on battery has ended in a low-battery shutdown: refine the discharging curve from it,
	/// saved before this returns, since the device goes away next (CHG, "Learning").
	fn learn_from_run(&mut self) {
		let (Some(run), Some(stored)) = (self.run.take(), &self.stored) else {
			return;
		};
		let document = &stored.document;
		match learn::from_run(
			document.discharging(),
			run.from_full,
			&run.recording.samples,
		) {
			Ok(curve) => {
				let refined = Document::new(curve, document.charging().cloned());
				self.refine(Which::Discharging, refined);
			}
			Err(why) => tracing::info!(%why, "the run taught the discharging curve nothing"),
		}
	}

	/// Put a refined document in force, reporting the refinement (CHG).
	fn refine(&mut self, which: Which, refined: Result<Document, Invalid>) {
		let document = match refined {
			Ok(document) => document,
			Err(err) => {
				tracing::warn!(curve = which.name(), %err, "a refined curve was not valid, and was not kept");
				return;
			}
		};
		let Some(&Curve {
			learnt_from,
			error,
			duration,
			..
		}) = which.curve(&document)
		else {
			return;
		};
		match self.replace(document) {
			Ok(()) => tracing::info!(
				curve = which.name(),
				learnt_from,
				error = %format!("{error:.4}"),
				duration_secs = %format!("{duration:.0}"),
				"refined the battery curve"
			),
			Err(err) => {
				tracing::warn!(curve = which.name(), %err, "the refined curve could not be put in force")
			}
		}
	}

	/// Put `document` in force: saved first, so what is in force is always what is on disk, then sent
	/// to every curve stream.
	fn replace(&mut self, document: Document) -> Result<(), Unchanged> {
		let Some(stored) = &self.stored else {
			return Err(Unchanged::NotManaged);
		};
		let next = Stored {
			document,
			gauge_full: stored.gauge_full,
		};
		self.store.save(&next).map_err(Unchanged::Save)?;
		self.curves.send_replace(Some(next.document.clone()));
		self.stored = Some(next);
		Ok(())
	}

	/// How the charge has moved over the last [`rate::WINDOW`], since external power last came or
	/// went.
	fn trend(&self) -> Option<Trend> {
		let newest = self.history.back()?;
		let samples: Vec<_> = self
			.history
			.iter()
			.rev()
			.take_while(|sample| {
				sample.external == newest.external
					&& newest.at.duration_since(sample.at) <= rate::WINDOW
			})
			.map(|sample| {
				(
					-newest.at.duration_since(sample.at).as_secs_f64(),
					sample.charge,
				)
			})
			.collect();
		rate::trend(&samples)
	}

	/// The cell voltage over the last [`WATCH`], up to the newest sample and back no further than the
	/// power line's last change: a cell falling on battery just before mains returned is not a cell
	/// draining on mains.
	fn recent(&self) -> Recent {
		let Some(newest) = self.history.back() else {
			return Recent::default();
		};
		let mut seen: Vec<_> = self
			.history
			.iter()
			.rev()
			.take_while(|sample| {
				newest.at.duration_since(sample.at) <= WATCH && sample.external == newest.external
			})
			.map(|sample| (sample.at, sample.volts))
			.collect();
		seen.reverse();
		Recent::new(seen)
	}
}

impl Recording {
	fn new(began: Instant) -> Self {
		Self {
			began,
			samples: Vec::new(),
		}
	}

	/// Add a sample; false where the recording has run too long to mean anything.
	fn push(&mut self, now: Instant, volts: f64) -> bool {
		if self.samples.len() >= RECORDED {
			return false;
		}
		self.samples.push((now.duration_since(self.began), volts));
		true
	}

	fn last(&self) -> Option<f64> {
		self.samples.last().map(|&(_, volts)| volts)
	}
}

/// The charge to report, from 0 at the floor to 1 at full (CHG, "What is reported").
///
/// Off mains, the discharging curve at the cell voltage. On mains, the charging curve where it has
/// been learnt from enough charges and covers the voltage, and otherwise the gauge's own figure
/// scaled to its reading on a full cell, unscaled until one is known. Full once the charge has
/// finished, whatever either gives. Where the power line cannot be read, the discharging curve.
fn estimate(stored: &Stored, gauge: Gauge, external: Option<bool>, full: bool) -> f64 {
	if full {
		return 1.0;
	}
	let document = &stored.document;
	if external == Some(true) {
		if let Some(charging) = charging_curve(document, gauge.volts) {
			return document.report(charging.charge_at(gauge.volts));
		}
		let share = gauge.charge / 100.0;
		let scaled = match stored.gauge_full {
			Some(full) => share / full,
			None => share,
		};
		return scaled.clamp(0.0, 1.0);
	}
	document.report(document.discharging().charge_at(gauge.volts))
}

/// How far the charge [`estimate`] gives may be off, as a share of what is reported: the error of the
/// curve it was read from, over the share of the scale between the floor and full; or, for the
/// gauge's figure on mains, that of a charging curve learnt from none (CHG, "Accuracy").
fn figure_error(stored: &Stored, gauge: Gauge, external: Option<bool>) -> f64 {
	let document = &stored.document;
	let curve = if external == Some(true) {
		match charging_curve(document, gauge.volts) {
			Some(charging) => charging,
			None => return unmeasured_error(0),
		}
	} else {
		document.discharging()
	};
	let scale = 1.0 - document.floor_charge();
	if scale > 0.0 {
		(curve.error / scale).min(1.0)
	} else {
		1.0
	}
}

/// The charging curve, where it is read at `volts`: learnt from enough charges, and covering them
/// (CHG).
fn charging_curve(document: &Document, volts: f64) -> Option<&Curve> {
	document
		.charging()
		.filter(|curve| curve.learnt_from >= CHARGING_LEARNT && curve.covers(volts))
}
