//! The buffer of recent samples, and the task that fills it.
//!
//! Behaviour is specified in BLI-SYS, "Sampling and history". A client subscribing receives the
//! window before it receives anything live, so a graph is populated the moment it appears rather
//! than filling from empty while an operator waits.
//!
//! Sampling runs whether or not anyone is connected, so the window covers the minutes before a
//! client arrived. It stops after a spell with no session, so a device left unattended is not
//! sampling forever, and starts again when a session opens.

use std::{
	collections::VecDeque,
	sync::{Arc, Mutex},
	time::Duration,
};

use bliti_core::channel::readings::{Sample, Series, Value};
use tokio::sync::broadcast;

use crate::facts::Facts;

/// How far back the window reaches. About what a graph on a phone can legibly show.
const WINDOW: Duration = Duration::from_secs(5 * 60);

/// How long the fast readings go between samples. Often enough to read as live.
const FAST: Duration = Duration::from_secs(1);

/// How many fast samples pass between the slow readings being taken.
const SLOW_EVERY: u32 = 5;

/// How long sampling continues with no session open.
const IDLE_STOP: Duration = Duration::from_secs(30 * 60);

/// How many samples the window can hold, at the fastest cadence.
const CAPACITY: usize = (WINDOW.as_secs() / FAST.as_secs()) as usize;

/// The most points one series carries when the window is sent.
///
/// The window is kept at full resolution; this thins only what goes on the wire. A graph on a phone
/// cannot draw more than this, and the link it crosses is BLE: every point costs notifications, and
/// the whole window arrives in one message the moment a client subscribes.
const MAX_POINTS: usize = 150;

/// The recent window, and a feed of samples as they are taken.
#[derive(Debug, Clone)]
pub struct Sampler {
	window: Arc<Mutex<VecDeque<Sample>>>,
	sessions: Arc<Mutex<usize>>,
	live: broadcast::Sender<Sample>,
}

impl Sampler {
	/// Start sampling. Called when the device starts, so the window covers the time before anyone
	/// connects.
	pub fn start() -> Self {
		let sampler = Self {
			window: Arc::new(Mutex::new(VecDeque::with_capacity(CAPACITY))),
			sessions: Arc::new(Mutex::new(0)),
			live: broadcast::channel(CAPACITY).0,
		};
		tokio::spawn(sampler.clone().run());
		sampler
	}

	/// The window as it stands, as one series of numbers per reading that has any.
	///
	/// Numbers only. Sending the window as whole samples repeats every reading's description against
	/// every point, which measured at 865 kB for a five-minute window: over a BLE link that is
	/// thousands of notifications and it drowns the connection before anything else can be said.
	pub fn series(&self) -> Vec<Series> {
		let window = self
			.window
			.lock()
			.expect("the window is never held across a panic");

		// Insertion-ordered so the series come out in the order the readings were first seen, which is
		// the order a client will show them in.
		let mut order: Vec<String> = Vec::new();
		let mut points: std::collections::HashMap<String, Vec<(u64, f64)>> =
			std::collections::HashMap::new();

		for sample in window.iter() {
			for reading in &sample.readings {
				let Some(number) = numeric(reading.value.as_ref()) else {
					continue;
				};
				let held = points.entry(reading.name.clone()).or_insert_with(|| {
					order.push(reading.name.clone());
					Vec::new()
				});
				held.push((sample.at, number));
			}
		}

		order
			.into_iter()
			.filter_map(|name| {
				let points = thin(points.remove(&name)?);
				Some(Series { name, points })
			})
			.collect()
	}

	/// The newest value of every reading the window holds, as one sample.
	///
	/// Merged across samples rather than taken from the last one: the readings that move slowly are
	/// taken every fifth sample, so the newest sample on its own is missing most of them.
	pub fn latest(&self) -> Option<Sample> {
		let window = self
			.window
			.lock()
			.expect("the window is never held across a panic");

		let mut order: Vec<String> = Vec::new();
		let mut newest: std::collections::HashMap<String, bliti_core::channel::readings::Reading> =
			std::collections::HashMap::new();
		let mut at = 0;
		for sample in window.iter() {
			at = at.max(sample.at);
			for reading in &sample.readings {
				if newest
					.insert(reading.name.clone(), reading.clone())
					.is_none()
				{
					order.push(reading.name.clone());
				}
			}
		}
		if order.is_empty() {
			return None;
		}
		Some(Sample {
			at,
			readings: order
				.into_iter()
				.filter_map(|name| newest.remove(&name))
				.collect(),
		})
	}

	/// Subscribe to samples as they are taken.
	pub fn live(&self) -> broadcast::Receiver<Sample> {
		self.live.subscribe()
	}

	/// Mark a session open for as long as the guard lives, so sampling does not stop under it.
	pub fn session(&self) -> SessionGuard {
		*self
			.sessions
			.lock()
			.expect("the count is never held across a panic") += 1;
		SessionGuard {
			sessions: self.sessions.clone(),
		}
	}

	fn open_sessions(&self) -> usize {
		*self
			.sessions
			.lock()
			.expect("the count is never held across a panic")
	}

	async fn run(self) {
		let mut facts = Facts::new();
		let mut ticks: u32 = 0;
		let mut idle = Duration::ZERO;
		let mut ticker = tokio::time::interval(FAST);
		ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

		loop {
			ticker.tick().await;

			// A spell with nobody connected stops the sampling; a session opening starts it again.
			if self.open_sessions() == 0 {
				idle += FAST;
				if idle >= IDLE_STOP {
					tracing::debug!("no session for a while; sampling stops until one opens");
					self.await_session(&mut ticker).await;
					// The gap makes every counter a baseline again.
					facts = Facts::new();
					idle = Duration::ZERO;
					continue;
				}
			} else {
				idle = Duration::ZERO;
			}

			ticks = ticks.wrapping_add(1);
			let readings = facts.sample(ticks % SLOW_EVERY == 0);
			if readings.is_empty() {
				continue;
			}

			let sample = Sample {
				at: Facts::since_boot(),
				readings,
			};

			{
				let mut window = self
					.window
					.lock()
					.expect("the window is never held across a panic");
				if window.len() == CAPACITY {
					window.pop_front();
				}
				window.push_back(sample.clone());
			}

			// Nobody subscribed is the ordinary case, not a failure.
			let _ = self.live.send(sample);
		}
	}

	/// Wait, cheaply, until a session opens.
	async fn await_session(&self, ticker: &mut tokio::time::Interval) {
		while self.open_sessions() == 0 {
			ticker.tick().await;
		}
	}
}

/// Every point where there are few enough, and an even spread of them where there are not.
///
/// The newest point is always kept, so the end of the graph is where the reading actually is rather
/// than wherever the spread happened to land.
fn thin(points: Vec<(u64, f64)>) -> Vec<(u64, f64)> {
	if points.len() <= MAX_POINTS {
		return points;
	}
	let last = points.len() - 1;
	let step = points.len() as f64 / MAX_POINTS as f64;
	let mut thinned: Vec<(u64, f64)> = (0..MAX_POINTS)
		.map(|index| points[((index as f64 * step) as usize).min(last)])
		.collect();
	if thinned.last() != points.last() {
		thinned.push(points[last]);
	}
	thinned
}

/// The number behind a value, where it has one, rounded to what a graph can show. A text value has
/// none, and nor has a kind this build does not know.
fn numeric(value: Option<&Value>) -> Option<f64> {
	let raw = match value? {
		Value::Fraction(number) => *number,
		Value::Quantity { number, .. } => *number,
		Value::Duration(seconds) => *seconds,
		Value::Text(_) | Value::Unknown(_) => return None,
	};
	// Four significant places is finer than any graph can draw and keeps the numbers short, which is
	// the whole point of sending a series rather than the samples.
	Some((raw * 10_000.0).round() / 10_000.0)
}

/// Holds sampling open for as long as a session lasts.
#[derive(Debug)]
pub struct SessionGuard {
	sessions: Arc<Mutex<usize>>,
}

impl Drop for SessionGuard {
	fn drop(&mut self) {
		let mut count = self
			.sessions
			.lock()
			.expect("the count is never held across a panic");
		*count = count.saturating_sub(1);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The window is bounded by time at the fastest cadence, so it cannot grow without limit on a
	/// device nobody has connected to for weeks.
	#[test]
	fn the_window_is_bounded() {
		assert_eq!(CAPACITY, 300);
		assert!(WINDOW.as_secs() >= 60, "long enough to show a shape");
	}

	#[tokio::test]
	async fn a_session_guard_holds_sampling_open_and_releases_it() {
		let sampler = Sampler {
			window: Arc::new(Mutex::new(VecDeque::new())),
			sessions: Arc::new(Mutex::new(0)),
			live: broadcast::channel(4).0,
		};
		assert_eq!(sampler.open_sessions(), 0);

		let one = sampler.session();
		let two = sampler.session();
		assert_eq!(sampler.open_sessions(), 2);

		drop(one);
		assert_eq!(sampler.open_sessions(), 1);
		drop(two);
		assert_eq!(sampler.open_sessions(), 0);
	}

	#[tokio::test(start_paused = true)]
	async fn the_window_fills_and_then_evicts_oldest_first() {
		let sampler = Sampler::start();
		let _session = sampler.session();

		// Long enough for several samples, well short of the window.
		tokio::time::sleep(FAST * 4 + Duration::from_millis(100)).await;
		let series = sampler.series();
		assert!(!series.is_empty(), "sampling fills the window");

		// Times are boot-relative and ascending, which is what lets a client space a graph by them.
		for each in &series {
			for pair in each.points.windows(2) {
				assert!(pair[0].0 <= pair[1].0, "{} went backwards", each.name);
			}
		}
	}

	/// Text has no number to graph, so it is left out of the series rather than sent as something a
	/// graph cannot draw.
	#[test]
	fn only_values_with_a_number_become_a_series() {
		assert_eq!(numeric(Some(&Value::Fraction(0.125))), Some(0.125));
		assert_eq!(numeric(Some(&Value::quantity(4.19, "V"))), Some(4.19));
		assert_eq!(numeric(Some(&Value::Duration(20.0))), Some(20.0));
		assert_eq!(numeric(Some(&Value::text("Mains"))), None);
		assert_eq!(numeric(None), None);
	}

	/// A short series is sent whole; a long one is spread evenly and keeps its newest point, so the
	/// end of the graph is where the reading actually is.
	#[test]
	fn a_long_series_is_thinned_and_keeps_its_newest_point() {
		let short: Vec<(u64, f64)> = (0..10).map(|index| (index, index as f64)).collect();
		assert_eq!(thin(short.clone()), short);

		let long: Vec<(u64, f64)> = (0..1000).map(|index| (index, index as f64)).collect();
		let thinned = thin(long.clone());
		assert!(thinned.len() <= MAX_POINTS + 1, "{}", thinned.len());
		assert_eq!(thinned.first(), long.first());
		assert_eq!(thinned.last(), long.last());
		for pair in thinned.windows(2) {
			assert!(pair[0].0 < pair[1].0, "still in order");
		}
	}

	/// The slow readings are taken every fifth sample, so the newest sample alone is missing most of
	/// them. An operator would see a view that filled in over five seconds for no reason.
	#[tokio::test(start_paused = true)]
	async fn the_latest_merges_across_samples_rather_than_taking_the_last() {
		let sampler = Sampler::start();
		let _session = sampler.session();
		tokio::time::sleep(FAST * 7).await;

		let latest = sampler.latest().expect("something was sampled");
		let names: Vec<&str> = latest.readings.iter().map(|r| r.name.as_str()).collect();
		// Taken on the fast tier. Memory rather than processor use, because time is paused here: the
		// ticks pass in virtual time, so the kernel's jiffy counters need not have advanced between
		// two samples, and processor use correctly reports nothing when they have not.
		assert!(names.contains(&"memory"), "{names:?}");
		// Taken on the slow tier, so a newest-sample-only answer would usually miss it.
		assert!(
			names.iter().any(|name| name.starts_with("uptime")),
			"{names:?}"
		);
	}

	/// Long decimals are what make a series large, and no graph can draw them.
	#[test]
	fn numbers_are_rounded_to_what_a_graph_can_show() {
		assert_eq!(numeric(Some(&Value::Fraction(0.123_456_789))), Some(0.1235));
	}

	#[tokio::test(start_paused = true)]
	async fn a_subscriber_receives_samples_as_they_are_taken() {
		let sampler = Sampler::start();
		let _session = sampler.session();
		let mut live = sampler.live();

		tokio::time::sleep(FAST * 3).await;
		let sample = live.try_recv().expect("a sample reached the subscriber");
		assert!(!sample.readings.is_empty());
	}
}
