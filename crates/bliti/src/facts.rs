//! What the device reports about itself, as the self-describing readings of NFO.
//!
//! Every reading carries its own meaning, so a client renders one it has never heard of. Nothing
//! here assumes a client that knows the names below.
//!
//! Two rules govern what appears. Hardware that is not fitted produces no reading at all, because an
//! operator standing at the device can see what is attached. Hardware that is fitted and does not
//! answer produces a reading carrying its error, because a source that has broken is a fault nobody
//! can see from outside the case.
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

use bliti_core::channel::readings::{Reading, Value};

mod board;
mod compute;
mod network;
mod power;
mod storage;
mod thermal;

/// Everything the device reports, and the counters that only mean something as a difference.
///
/// Processor use and network throughput are both cumulative in the kernel, so a single read of
/// either says nothing: the first sample establishes a baseline and reports neither.
#[derive(Debug, Default)]
pub struct Facts {
	cpu: Option<compute::CpuCounters>,
	network: BTreeMap<String, network::Counters>,
	// The cell is watched across samples: what tells a battery carrying the device from one sitting
	// idle is whether its voltage moves, which no single reading can say.
	power: power::Watch,
	taken: Option<Instant>,
}

impl Facts {
	/// A fresh source, holding no baseline yet.
	pub fn new() -> Self {
		Self::default()
	}

	/// Milliseconds since the device booted.
	///
	/// Measured from boot rather than from an epoch because a device in the field may have no set
	/// clock and no way to reach one. Meaningful only against other times from the same device.
	pub fn since_boot() -> u64 {
		uptime().map_or(0, |up| (up.as_secs_f64() * 1000.0) as u64)
	}

	/// What the device is: readings that do not change while it runs, or change rarely.
	pub fn statics(&self) -> Vec<Reading> {
		let mut readings = vec![
			Reading::new("hostname", "Hostname", Value::text(board::hostname())),
			board::identity(),
		];
		readings.extend(board::os());
		readings.extend(network::addresses());
		readings
	}

	/// How the device is doing, as of now.
	///
	/// Readings are split by how fast what they measure moves. Processor use and throughput are taken
	/// every time; the rest are taken only when `slow` is set, because a disk that filled up in the
	/// last second is not a thing that happens and reporting it at that rate is traffic over a link
	/// somebody's phone is holding open.
	pub fn sample(&mut self, slow: bool) -> Vec<Reading> {
		let now = Instant::now();
		let elapsed = self.taken.map(|then| now.duration_since(then));
		self.taken = Some(now);

		let mut readings = Vec::new();
		readings.extend(compute::cpu(&mut self.cpu));
		// Memory is a single file read and belongs beside processor use, both in what it costs and in
		// where an operator looks for it.
		readings.extend(compute::memory());
		readings.extend(network::throughput(&mut self.network, elapsed));

		if slow {
			readings.extend(storage::disks());
			readings.extend(thermal::readings());
			readings.extend(self.power.readings());
			readings.extend(
				// Uptime only ever climbs, so its graph is a ramp that says nothing.
				uptime().map(|up| {
					Reading::new("uptime", "Uptime", Value::Duration(up.as_secs_f64())).ungraphed()
				}),
			);
		}
		readings
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
	fn the_statics_name_the_device() {
		let readings = Facts::new().statics();
		let hostname = readings.iter().find(|r| r.name == "hostname").unwrap();
		assert!(matches!(&hostname.value, Some(Value::Text(name)) if !name.is_empty()));
		assert!(readings.iter().all(Reading::is_coherent));
	}

	/// Every reading is either a measurement or an account of why there is none, at every sample.
	#[test]
	fn every_reading_holds_together() {
		let mut facts = Facts::new();
		for _ in 0..2 {
			for reading in facts.sample(true) {
				assert!(reading.is_coherent(), "{reading:?}");
				assert!(!reading.name.is_empty());
				assert!(!reading.label.is_empty());
			}
		}
	}

	/// Cumulative counters say nothing on their own, so the first sample establishes a baseline and
	/// reports no rate. Reporting one would divide the whole of uptime into the whole of the counter.
	#[test]
	fn the_first_sample_reports_no_rate() {
		let mut facts = Facts::new();
		let first = facts.sample(true);
		assert!(
			!first.iter().any(|r| r.name == "cpu"),
			"a first sample cannot know processor use"
		);
		assert!(
			!first.iter().any(|r| r.name.starts_with("network-")),
			"a first sample cannot know throughput"
		);
	}

	#[test]
	fn a_second_sample_reports_what_the_first_could_not() {
		let mut facts = Facts::new();
		facts.sample(true);
		std::thread::sleep(Duration::from_millis(60));
		let second = facts.sample(true);
		let cpu = second.iter().find(|r| r.name == "cpu").expect("cpu");
		let Some(Value::Fraction(used)) = cpu.value else {
			panic!("processor use is a fraction")
		};
		assert!((0.0..=1.0).contains(&used), "{used}");
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

#[cfg(test)]
mod size {
	use super::*;
	use bliti_core::channel::{
		messages::DeviceMessage,
		readings::{Sample, Series},
	};

	/// What a subscription puts on the wire. The link is BLE, so this is not a curiosity: sending the
	/// window as whole samples measured at 865 kB, which is thousands of notifications and drowns the
	/// connection before anything else can be said.
	#[test]
	#[ignore = "reports sizes rather than asserting"]
	fn report_the_size_of_a_window() {
		let mut facts = Facts::new();
		facts.sample(true);
		std::thread::sleep(Duration::from_millis(50));
		let readings = facts.sample(true);

		let one = DeviceMessage::SystemSample {
			at: 1,
			readings: readings.clone(),
		}
		.to_json();
		println!(
			"one slow sample: {} bytes, {} readings",
			one.len(),
			readings.len()
		);

		let as_samples = DeviceMessage::SystemHistory { series: Vec::new() };
		let _ = as_samples;

		let window: Vec<Sample> = (0..300)
			.map(|index| Sample {
				at: index * 1000,
				readings: readings.clone(),
			})
			.collect();
		let whole = serde_json::to_vec(&window).unwrap();
		println!("300 samples, whole: {} bytes", whole.len());

		let series: Vec<Series> = readings
			.iter()
			.filter_map(|reading| {
				let number = match reading.value.as_ref()? {
					Value::Fraction(number) | Value::Quantity { number, .. } => *number,
					Value::Duration(seconds) => *seconds,
					_ => return None,
				};
				Some(Series {
					name: reading.name.clone(),
					points: (0..300).map(|index| (index * 1000, number)).collect(),
				})
			})
			.collect();
		let sent = DeviceMessage::SystemHistory { series }.to_json();
		println!("300 samples, as series: {} bytes", sent.len());
		println!("  at a 247-byte MTU: {} notifications", sent.len() / 244);
	}
}
