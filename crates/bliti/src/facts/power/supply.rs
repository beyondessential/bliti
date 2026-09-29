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
	low::{Floor, Low},
};
use super::{
	Gauge, Recent, WATCH,
	curve::{
		Document,
		store::{DEFAULT_PATH, Store, StoreError, Stored},
	},
	record::{Event, Record},
};

mod full;
mod low;
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

	fn state(&self) -> MutexGuard<'_, State> {
		self.state.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// What the last look found.
	pub fn seen(&self) -> Seen {
		self.state().seen.clone()
	}

	/// Whether this device manages a backup supply: whether a gauge has answered.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "read by the curve stream, still to come (T2)")
	)]
	pub fn managed(&self) -> bool {
		self.state().stored.is_some()
	}

	/// The curve document in force, where a backup supply is managed.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "read by the curve stream, still to come (T2)")
	)]
	pub fn document(&self) -> Option<Document> {
		self.state()
			.stored
			.as_ref()
			.map(|stored| stored.document.clone())
	}

	/// The curve document in force, as it changes: by a load, a reset or a refinement, and from
	/// nothing once a gauge first answers (CRV).
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "read by the curve stream, still to come (T2)")
	)]
	pub fn curves(&self) -> watch::Receiver<Option<Document>> {
		self.state().curves.subscribe()
	}

	/// Replace both curves with `document`'s, saved before they take effect (CRV, "Loading and
	/// resetting"). Blocking, since it writes the curve file.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "called by the curve stream, still to come (T2)")
	)]
	pub fn load(&self, document: Document) -> Result<(), Unchanged> {
		self.state().replace(document)
	}

	/// Return to the curve the build carries, holding no charging curve (CRV). Blocking, as
	/// [`Supply::load`].
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "called by the curve stream, still to come (T2)")
	)]
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

	/// What came of powering off for a low battery (LOW).
	pub fn settle(&self, result: &Result<(), crate::power::NotBegun>) {
		self.state().low.settle(result);
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
					self.charge_finished(gauge);
				}
			}
			// The cell carries the device again, so it is no longer known full (CHG).
			Some(false) => self.full = Full::default(),
			None => {}
		}
		let full = self.full.finished();
		let charge = self
			.stored
			.as_ref()
			.map_or(0.0, |stored| estimate(stored, gauge, external, full));

		self.history.push_back(Sample {
			at: now,
			volts: gauge.volts,
			charge,
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
	fn charge_finished(&mut self, gauge: Gauge) {
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
			self.learn_from_charge(charge);
		}
	}

	/// Where a charge from a known start has reached full. Learning the charging curve from it comes
	/// here, saved and sent to every curve stream through [`State::replace`] (CHG, "Learning").
	fn learn_from_charge(&mut self, _charge: Charge) {}

	/// Where a run on battery has ended in a low-battery shutdown, before it goes away. Refining the
	/// discharging curve from the run comes here, and must be saved before this returns (CHG,
	/// "Learning").
	fn learn_from_run(&mut self) {
		self.run = None;
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

	/// The cell voltage over the last [`WATCH`], up to the newest sample.
	fn recent(&self) -> Recent {
		let Some(newest) = self.history.back() else {
			return Recent::default();
		};
		Recent::new(
			self.history
				.iter()
				.filter(|sample| newest.at.duration_since(sample.at) <= WATCH)
				.map(|sample| (sample.at, sample.volts))
				.collect(),
		)
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
		if let Some(charging) = document
			.charging()
			.filter(|curve| curve.learnt_from >= CHARGING_LEARNT && curve.covers(gauge.volts))
		{
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
