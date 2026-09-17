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

use bliti_core::channel::readings::Sample;
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

	/// The window as it stands, oldest first.
	pub fn window(&self) -> Vec<Sample> {
		self.window
			.lock()
			.expect("the window is never held across a panic")
			.iter()
			.cloned()
			.collect()
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
		let window = sampler.window();
		assert!(!window.is_empty(), "sampling fills the window");

		// Times are boot-relative and ascending, which is what lets a client space a graph by them.
		for pair in window.windows(2) {
			assert!(pair[0].at <= pair[1].at, "{:?} then {:?}", pair[0], pair[1]);
		}
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
