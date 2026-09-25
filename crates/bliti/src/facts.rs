//! What the device reports about itself, as the facts and readings of NFO.
//!
//! Every entry carries its own meaning, so a reader renders one it has never heard of. Nothing here
//! assumes a reader that knows the names below.
//!
//! Facts and readings are two catalogues of one shape. A [`facts`] entry is something true about the
//! device, read fresh each time and holding no state; a reading is a measurement whose derivation may
//! need recent history, so the readings live on [`Facts`], which the sampler owns and ticks.
//!
//! Two rules govern what appears (NFO). Hardware that is not fitted produces no entry at all, because
//! an operator standing at the device can see what is attached. Hardware that is fitted and did not
//! answer produces a `broken` entry carrying its reason, because a source that has broken is a fault
//! nobody can see from outside the case.
//!
//! Sources are read from `/proc` and `/sys` directly rather than through a system-information crate:
//! several of the readings here are specific to the board we ship, and the ones that are not are a
//! line of parsing each.

use std::{
	collections::BTreeMap,
	fs,
	net::IpAddr,
	time::{Duration, Instant},
};

use bliti_core::channel::readings::Entry;

use crate::network::stack::Report;

pub use power::record_supply;

mod board;
mod compute;
mod network;
mod power;
mod storage;
mod thermal;

/// The facts a device reports: what board it is, what it runs, and the shapes a reading is measured
/// against. Read fresh, so the caller polls this and sends what changed (NFO).
pub fn facts(at: u64) -> Vec<Entry> {
	let mut entries = vec![Entry::text(at, "hostname", board::hostname())];
	entries.extend(board::identity(at));
	entries.extend(board::os(at));
	entries.extend(board::last_boot(at));
	entries.extend(network::addresses(at));
	entries.extend(compute::memory_total(at));
	entries.extend(compute::cpu_frequency_max(at));
	entries.extend(storage::totals(at));
	entries
}

/// The readings-derivation state: the counters and voltage history that only mean something across
/// samples. The sampler holds one of these and ticks it (NFO).
#[derive(Debug, Default)]
pub struct Facts {
	cpu: Option<compute::CpuCounters>,
	network: BTreeMap<String, network::Counters>,
	// The cell is watched across samples: what tells a battery carrying the device from one sitting
	// idle is whether its voltage moves, which no single reading can say.
	power: power::Watch,
	taken: Option<Instant>,
	/// What the network backend joined and runs, where it configures the network.
	wireless: Option<Report>,
}

impl Facts {
	/// A fresh source, holding no baseline yet, reporting too what the network backend joined and
	/// runs where it has one (NFO).
	pub fn new(wireless: Option<Report>) -> Self {
		Self {
			wireless,
			..Self::default()
		}
	}

	/// Milliseconds since the device booted. Boot-relative because a device in the field may have no
	/// set clock; meaningful only against other times from the same device (NFO).
	pub fn since_boot() -> u64 {
		uptime().map_or(0, |up| (up.as_secs_f64() * 1000.0) as u64)
	}

	/// How the device is doing, as of now.
	///
	/// Readings split by how fast what they measure moves. Processor use and throughput are taken
	/// every time; the rest only when `slow` is set, because a disk that filled up in the last second
	/// is not a thing that happens and reporting it at that rate is traffic over a link somebody's
	/// phone is holding open.
	pub fn sample(&mut self, at: u64, slow: bool) -> Vec<Entry> {
		let now = Instant::now();
		let elapsed = self.taken.map(|then| now.duration_since(then));
		self.taken = Some(now);

		let mut readings = Vec::new();
		readings.extend(compute::cpu_usage(at, &mut self.cpu));
		readings.extend(compute::memory_usage(at));
		readings.extend(network::throughput(at, &mut self.network, elapsed));

		if slow {
			readings.extend(compute::cpu_frequency(at));
			readings.extend(storage::usage(at));
			readings.extend(thermal::temperature(at));
			readings.extend(thermal::fan(at));
			readings.extend(self.power.readings(at));
		}
		// The wireless network and hotspot are facts and change slowly; the client count is a
		// reading and is taken every time.
		if let Some(wireless) = &self.wireless {
			let route = network::default_route();
			readings.extend(wireless.entries(at, slow, |name| {
				network::interface_trait(name, route.as_deref())
			}));
		}
		readings
	}
}

impl crate::sampler::Source for Facts {
	fn gather(&mut self, slow: bool) -> Vec<Entry> {
		self.sample(Self::since_boot(), slow)
	}

	fn reset(&mut self) {
		*self = Self::new(self.wireless.take());
	}
}

/// How long the device has been up. The first field of `/proc/uptime`, in seconds.
fn uptime() -> Option<Duration> {
	let raw = fs::read_to_string("/proc/uptime").ok()?;
	let seconds: f64 = raw.split_whitespace().next()?.parse().ok()?;
	Some(Duration::from_secs_f64(seconds))
}

/// Read a file, trimmed, discarding the trailing NUL a device-tree property carries.
fn read_trimmed(path: &str) -> Option<String> {
	let raw = fs::read_to_string(path).ok()?;
	let text = raw.trim_end_matches('\0').trim();
	(!text.is_empty()).then(|| text.to_owned())
}

/// Read a file holding one integer.
fn read_number(path: &str) -> Option<i64> {
	read_trimmed(path)?.parse().ok()
}

/// Whether an address is worth reporting: global, and neither loopback nor link-local.
fn is_reportable(ip: IpAddr) -> bool {
	match ip {
		IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local() && !v4.is_unspecified(),
		IpAddr::V6(v6) => {
			// `is_unicast_link_local` is not stable, so match fe80::/10 directly.
			let link_local = (v6.segments()[0] & 0xffc0) == 0xfe80;
			!v6.is_loopback() && !link_local && !v6.is_unspecified()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_facts_name_the_device() {
		let entries = facts(20_308_140);
		let hostname = entries.iter().find(|e| e.name == "hostname").unwrap();
		assert_eq!(hostname.status(), Some("passed"));
		assert!(hostname.value.is_some());
	}

	/// Every fact and reading carries a status, at every sample (NFO).
	#[test]
	fn every_entry_carries_a_status() {
		for entry in facts(1) {
			assert!(entry.status().is_some(), "{entry:?}");
			assert!(!entry.name.is_empty());
		}
		let mut source = Facts::new(None);
		for _ in 0..2 {
			for entry in source.sample(1, true) {
				assert!(entry.status().is_some(), "{entry:?}");
				assert!(!entry.name.is_empty());
			}
		}
	}

	/// Cumulative counters say nothing on their own, so the first sample establishes a baseline and
	/// reports no rate (NFO).
	#[test]
	fn the_first_sample_reports_no_rate() {
		let mut source = Facts::new(None);
		let first = source.sample(1, true);
		assert!(
			!first.iter().any(|e| e.name == "cpu-usage"),
			"a first sample cannot know processor use"
		);
		assert!(
			!first.iter().any(|e| e.name == "network-throughput"),
			"a first sample cannot know throughput"
		);
	}

	#[test]
	fn boot_relative_time_advances() {
		let first = Facts::since_boot();
		assert!(first > 0);
		std::thread::sleep(Duration::from_millis(20));
		assert!(Facts::since_boot() >= first);
	}

	#[test]
	fn loopback_and_link_local_are_not_reportable() {
		use std::net::{Ipv4Addr, Ipv6Addr};
		assert!(!is_reportable(Ipv4Addr::LOCALHOST.into()));
		assert!(!is_reportable(Ipv6Addr::LOCALHOST.into()));
		assert!(!is_reportable(Ipv4Addr::new(169, 254, 3, 4).into()));
		assert!(!is_reportable(
			"fe80::1".parse::<Ipv6Addr>().unwrap().into()
		));
		assert!(is_reportable(Ipv4Addr::new(192, 168, 1, 10).into()));
		assert!(is_reportable(
			"2001:db8::1".parse::<Ipv6Addr>().unwrap().into()
		));
	}
}
