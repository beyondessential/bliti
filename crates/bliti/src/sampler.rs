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

/// A source of readings the sampler ticks. The gathering here reads the kernel's files directly and
/// can take as long as a syscall on a stalled filesystem does, so the sampler runs it off the async
/// runtime: a source that has blocked holds a blocking-pool thread, never the worker carrying the
/// BLE session and the connection to the system bus.
pub(crate) trait Source: Send + 'static {
	/// Gather one tick's readings. `slow` asks for the readings taken every fifth tick as well as the
	/// fast ones. Runs off the runtime, so it may block for as long as the kernel takes to answer.
	fn gather(&mut self, slow: bool) -> Vec<Entry>;

	/// Drop the derivation state after a gap in sampling, so every counter is a baseline again (NFO).
	fn reset(&mut self);
}

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
	///
	/// `wireless` is what the network backend joined and runs, where it configures the network.
	pub fn start(wireless: Option<crate::network::stack::Report>) -> Self {
		Self::start_with(Box::new(Facts::new(wireless)))
	}

	/// Start sampling from a given source. The device samples [`Facts`]; a test substitutes a source
	/// of its own to exercise the sampler without reading the real machine.
	fn start_with(source: Box<dyn Source>) -> Self {
		let sampler = Self {
			current: Arc::new(Mutex::new(HashMap::new())),
			sessions: Arc::new(Mutex::new(0)),
			live: broadcast::channel(LIVE_LAG).0,
		};
		tokio::spawn(sampler.clone().run(source));
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

	async fn run(self, mut source: Box<dyn Source>) {
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
					source.reset();
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
			let slow = ticks % SLOW_EVERY == 0;
			// The gathering reads the kernel's files directly and can block for as long as a stalled
			// filesystem takes to answer, so it runs off the runtime: while it blocks it holds a
			// blocking-pool thread, not the worker the session and the bus connection run on. The
			// source moves in and back out so its per-tick derivation state survives.
			let (returned, readings) = tokio::task::spawn_blocking(move || {
				let readings = source.gather(slow);
				(source, readings)
			})
			.await
			.expect("the sampling task neither panics nor is cancelled");
			source = returned;
			let mut readings = readings;

			{
				let mut current = self
					.current
					.lock()
					.expect("the snapshot is never held across a panic");
				// A slow tick takes every reading, so what it does not take is no longer there, as a
				// hotspot that has stopped is not, and is sent once more as ended (NFO).
				let before = if slow {
					std::mem::take(&mut *current)
				} else {
					HashMap::new()
				};
				for reading in &readings {
					current.insert(identity_key(reading), reading.clone());
				}
				let at = Facts::since_boot();
				for (key, gone) in before {
					if !current.contains_key(&key) {
						readings.push(gone.ended(at, ENDED));
					}
				}
			}
			if readings.is_empty() {
				continue;
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

/// The traits NFO makes wholly descriptive, and the members of others that describe.
const DESCRIPTIVE: [&str; 4] = [STATUS, LIMITS, "security", "channel"];
const DESCRIPTIVE_MEMBERS: [(&str, &[&str]); 2] = [
	("interface", &["route", "overlay"]),
	("battery", &["serial", "model", "vendor"]),
];

/// The reason an entry that has ended gives. What stopped it is not known this far from it.
pub(crate) const ENDED: &str = "no longer applies";

/// A stable identity for one reading instance: its name and its distinguishing traits, leaving out
/// the descriptive ones that change while the thing measured stays the same, as a hotspot's channel
/// does when it follows its station. This is only the device's own snapshot key; how a reader groups
/// readings is its own business (NFO).
pub(crate) fn identity_key(entry: &Entry) -> String {
	let mut traits = entry.traits.clone();
	for name in DESCRIPTIVE {
		traits.remove(name);
	}
	for (name, members) in DESCRIPTIVE_MEMBERS {
		if let Some(serde_json::Value::Object(object)) = traits.get_mut(name) {
			for member in members {
				object.remove(*member);
			}
		}
	}
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
	use std::sync::mpsc;

	use super::*;

	/// A source whose first gather blocks until the test releases it, standing in for a reading whose
	/// kernel source has stalled (a filesystem that has gone away). It signals when it has entered the
	/// block so the test need not guess at timing.
	struct BlockingSource {
		entered: mpsc::Sender<()>,
		release: mpsc::Receiver<()>,
		blocked: bool,
	}

	impl Source for BlockingSource {
		fn gather(&mut self, _slow: bool) -> Vec<Entry> {
			if !self.blocked {
				self.blocked = true;
				let _ = self.entered.send(());
				let _ = self.release.recv();
			}
			vec![Entry::quantity(1, "test-reading", "unit", 1.0)]
		}

		fn reset(&mut self) {}
	}

	/// The failure this guards against: a source taking its time must not occupy the runtime worker the
	/// session runs on, or a device whose storage is in trouble loses the Bluetooth link to the very
	/// operator diagnosing it. With the gather off the runtime, an async timer still fires and
	/// the session count still moves while a source sits blocked mid-sample.
	#[tokio::test]
	async fn a_blocking_source_does_not_stall_sampling_or_the_session() {
		let (entered_tx, entered_rx) = mpsc::channel();
		let (release_tx, release_rx) = mpsc::channel();
		let sampler = Sampler::start_with(Box::new(BlockingSource {
			entered: entered_tx,
			release: release_rx,
			blocked: false,
		}));
		let session = sampler.session();
		let mut live = sampler.live();

		// Wait, off the runtime, for the source to reach its block. If gathering ran on the runtime
		// worker instead, the worker would be frozen here and nothing below would make progress.
		let entered_rx = tokio::task::spawn_blocking(move || {
			entered_rx
				.recv_timeout(Duration::from_secs(5))
				.expect("sampling reached the source");
			entered_rx
		})
		.await
		.unwrap();

		// The source is blocked mid-sample. The runtime must still be serving: an async timer fires,
		// and a session opens and closes.
		tokio::time::timeout(
			Duration::from_secs(1),
			tokio::time::sleep(Duration::from_millis(50)),
		)
		.await
		.expect("the runtime kept running while a source blocked");
		let another = sampler.session();
		assert_eq!(sampler.open_sessions(), 2);
		drop(another);
		assert_eq!(sampler.open_sessions(), 1);

		// Release the source; sampling carries on and the reading reaches the feed.
		release_tx.send(()).unwrap();
		let readings = tokio::time::timeout(Duration::from_secs(2), live.recv())
			.await
			.expect("a reading arrived once the block cleared")
			.expect("the live feed stayed open");
		assert!(!readings.is_empty());

		drop(session);
		drop(entered_rx);
	}

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

		// Nor do descriptive traits and members: a network on a new channel, an interface taking the
		// default route.
		let joined = |channel: u32, route: bool| {
			let mut interface = json!({ "name": "wld0" });
			if route {
				interface["route"] = json!("default");
			}
			Entry::text(1, "wireless-network", "clinic")
				.with_trait("interface", interface)
				.with_trait("security", json!("psk"))
				.with_trait("channel", json!({ "number": channel, "band": "2ghz" }))
		};
		assert_eq!(
			identity_key(&joined(1, false)),
			identity_key(&joined(11, true))
		);
	}

	#[tokio::test(start_paused = true)]
	async fn a_subscriber_receives_readings_as_they_are_taken() {
		let sampler = Sampler::start(None);
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
		let sampler = Sampler::start(None);
		let _session = sampler.session();
		tokio::time::sleep(FAST * 7).await;

		let names: Vec<String> = sampler.current().iter().map(|e| e.name.clone()).collect();
		// Taken on the fast tier. Memory rather than processor use, because time is paused: the ticks
		// pass in virtual time, so the kernel's jiffy counters need not have advanced, and processor
		// use correctly reports nothing when they have not.
		assert!(names.iter().any(|n| n == "memory-usage"), "{names:?}");
	}

	/// A source reporting a hotspot for its first slow tick and not after.
	struct Stopping {
		slow_ticks: u32,
	}

	impl Source for Stopping {
		fn gather(&mut self, slow: bool) -> Vec<Entry> {
			let mut readings = vec![Entry::quantity(1, "memory-usage", "fraction", 0.5)];
			if slow {
				self.slow_ticks += 1;
				if self.slow_ticks == 1 {
					readings.push(Entry::text(1, "hotspot", "bliti"));
				}
			}
			readings
		}

		fn reset(&mut self) {}
	}

	/// What stops being there leaves the snapshot at the next slow tick (NFO).
	#[tokio::test(start_paused = true)]
	async fn the_snapshot_forgets_what_a_slow_tick_no_longer_takes() {
		let sampler = Sampler::start_with(Box::new(Stopping { slow_ticks: 0 }));
		let _session = sampler.session();
		let names = |sampler: &Sampler| -> Vec<String> {
			sampler.current().iter().map(|e| e.name.clone()).collect()
		};
		tokio::time::sleep(FAST * (SLOW_EVERY + 1)).await;
		assert!(names(&sampler).contains(&"hotspot".to_owned()));
		tokio::time::sleep(FAST * SLOW_EVERY).await;
		assert_eq!(names(&sampler), ["memory-usage"]);
	}

	/// A reader holding what stopped is told, since leaving it out says nothing (NFO).
	#[tokio::test(start_paused = true)]
	async fn what_a_slow_tick_no_longer_takes_is_sent_as_ended() {
		let sampler = Sampler::start_with(Box::new(Stopping { slow_ticks: 0 }));
		let _session = sampler.session();
		let mut live = sampler.live();
		tokio::time::sleep(FAST * (SLOW_EVERY * 2 + 1)).await;

		let mut sent = Vec::new();
		while let Ok(readings) = live.try_recv() {
			sent.extend(readings);
		}
		let hotspot: Vec<_> = sent.iter().filter(|e| e.name == "hotspot").collect();
		assert_eq!(hotspot.len(), 2, "{hotspot:?}");
		assert_eq!(hotspot[0].status(), Some("passed"));
		assert_eq!(hotspot[1].status(), Some("ended"));
		assert_eq!(hotspot[1].value, None);
		assert!(
			sent.iter()
				.filter(|e| e.name == "memory-usage")
				.all(|e| e.status() != Some("ended"))
		);
	}
}
