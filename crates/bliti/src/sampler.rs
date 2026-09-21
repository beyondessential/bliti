//! The sampling task: the readings the device is taking now, and a feed of them as they are taken.
//!
//! Behaviour is specified in NFO, "Sampling". Sampling runs whether or not anyone is connected, so a
//! feed that opens is served what is current at once rather than waiting a tick for it, and so the
//! cell voltage has the history its direction of travel is derived from. It stops after a spell with
//! no session, and starts again when one opens.
//!
//! Nothing is held for replay. A reader that comes back is sent what is current, not what accumulated
//! while it was away; history is the reader's to accumulate forward from when it connects (NFO). The
//! only state kept is what a derivation needs, which lives on [`Facts`].

use std::{
	collections::HashMap,
	sync::{Arc, Mutex},
	time::Duration,
};

use bliti_core::channel::readings::{Entry, LIMITS, STATUS};
use tokio::sync::broadcast;

use crate::facts::Facts;

/// How long the fast readings go between samples. Often enough to read as live.
const FAST: Duration = Duration::from_secs(1);

/// How many fast samples pass between the slow readings being taken.
const SLOW_EVERY: u32 = 5;

/// How long sampling continues with no session open (NFO).
const IDLE_STOP: Duration = Duration::from_secs(30 * 60);

/// How many ticks the live feed can fall behind before a subscriber misses some. A subscriber too
/// slow to keep up misses samples rather than stalling the sampler.
const LIVE_LAG: usize = 64;

/// The current readings, and a feed of them as they are taken.
#[derive(Debug, Clone)]
pub struct Sampler {
	current: Arc<Mutex<HashMap<String, Entry>>>,
	sessions: Arc<Mutex<usize>>,
	live: broadcast::Sender<Vec<Entry>>,
}

impl Sampler {
	/// Start sampling. Called when the device starts, so a feed that opens finds current readings
	/// rather than an empty view (NFO).
	pub fn start() -> Self {
		let sampler = Self {
			current: Arc::new(Mutex::new(HashMap::new())),
			sessions: Arc::new(Mutex::new(0)),
			live: broadcast::channel(LIVE_LAG).0,
		};
		tokio::spawn(sampler.clone().run());
		sampler
	}

	/// The newest reading of each distinct measurement, as one set. Merged across fast and slow ticks,
	/// so a feed opening sends a complete view at once rather than filling in over five seconds.
	pub fn current(&self) -> Vec<Entry> {
		self.current
			.lock()
			.expect("the snapshot is never held across a panic")
			.values()
			.cloned()
			.collect()
	}

	/// Subscribe to readings as they are taken.
	pub fn live(&self) -> broadcast::Receiver<Vec<Entry>> {
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
					// The gap makes every counter a baseline again, and the snapshot stale.
					facts = Facts::new();
					self.current
						.lock()
						.expect("the snapshot is never held across a panic")
						.clear();
					idle = Duration::ZERO;
					continue;
				}
			} else {
				idle = Duration::ZERO;
			}

			ticks = ticks.wrapping_add(1);
			let readings = facts.sample(Facts::since_boot(), ticks % SLOW_EVERY == 0);
			if readings.is_empty() {
				continue;
			}

			{
				let mut current = self
					.current
					.lock()
					.expect("the snapshot is never held across a panic");
				for reading in &readings {
					current.insert(identity_key(reading), reading.clone());
				}
			}

			// Nobody subscribed is the ordinary case, not a failure.
			let _ = self.live.send(readings);
		}
	}

	/// Wait, cheaply, until a session opens.
	async fn await_session(&self, ticker: &mut tokio::time::Interval) {
		while self.open_sessions() == 0 {
			ticker.tick().await;
		}
	}
}

/// A stable identity for one reading instance: its name and its distinguishing traits, leaving out
/// the descriptive `status` and `limits` that change while the thing measured stays the same. This is
/// only the device's own snapshot key; how a reader groups readings is its own business (NFO).
fn identity_key(entry: &Entry) -> String {
	let mut traits = entry.traits.clone();
	traits.remove(STATUS);
	traits.remove(LIMITS);
	// serde_json sorts object keys, so member order does not change the key.
	format!("{}\u{1f}{}", entry.name, serde_json::Value::Object(traits))
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

	#[tokio::test]
	async fn a_session_guard_holds_sampling_open_and_releases_it() {
		let sampler = Sampler {
			current: Arc::new(Mutex::new(HashMap::new())),
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

	/// Two readings alike but for a distinguishing trait are two entries in the snapshot; the same one
	/// re-sampled is one. Member order does not change the key.
	#[test]
	fn the_snapshot_keys_by_name_and_distinguishing_traits() {
		use serde_json::json;
		let a = Entry::quantity(1, "network-throughput", "bytes/second", 10.0)
			.with_trait("interface", json!({ "name": "eth0" }))
			.with_trait("direction", serde_json::Value::String("in".to_owned()));
		let b = Entry::quantity(1, "network-throughput", "bytes/second", 20.0)
			.with_trait("direction", serde_json::Value::String("out".to_owned()))
			.with_trait("interface", json!({ "name": "eth0" }));
		assert_ne!(
			identity_key(&a),
			identity_key(&b),
			"different direction, different key"
		);

		// The same reading re-sampled, into a warning, keeps its key: status is left out.
		let later = Entry::quantity(2, "network-throughput", "bytes/second", 30.0)
			.with_trait("interface", json!({ "name": "eth0" }))
			.with_trait("direction", serde_json::Value::String("in".to_owned()))
			.warning("busy");
		assert_eq!(identity_key(&a), identity_key(&later));
	}

	#[tokio::test(start_paused = true)]
	async fn a_subscriber_receives_readings_as_they_are_taken() {
		let sampler = Sampler::start();
		let _session = sampler.session();
		let mut live = sampler.live();

		tokio::time::sleep(FAST * 3).await;
		let readings = live.try_recv().expect("readings reached the subscriber");
		assert!(!readings.is_empty());
	}

	/// Sampling fills the current snapshot, merging fast and slow tiers so a feed opening sees a full
	/// set at once.
	#[tokio::test(start_paused = true)]
	async fn the_snapshot_merges_across_fast_and_slow_ticks() {
		let sampler = Sampler::start();
		let _session = sampler.session();
		tokio::time::sleep(FAST * 7).await;

		let names: Vec<String> = sampler.current().iter().map(|e| e.name.clone()).collect();
		// Taken on the fast tier. Memory rather than processor use, because time is paused: the ticks
		// pass in virtual time, so the kernel's jiffy counters need not have advanced, and processor
		// use correctly reports nothing when they have not.
		assert!(names.iter().any(|n| n == "memory-usage"), "{names:?}");
	}
}
