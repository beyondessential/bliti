//! Network addresses and throughput.
//!
//! Physical interfaces are reported, wired and wireless, along with the overlay the fleet is reached
//! over. Loopback and other virtual interfaces are left out: they say nothing about whether the
//! device an operator is standing at can be reached.

use std::{collections::BTreeMap, fs, time::Duration};

use bliti_core::channel::readings::{Direction, Reading, Value};

use super::{compute::bytes, is_reportable};

/// The cumulative byte counts one read of `/proc/net/dev` yields for an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
	received: u64,
	sent: u64,
}

/// The overlay the fleet is reached over. Reported alongside the physical interfaces because whether
/// a device is reachable remotely is exactly what an operator sent to it wants to know.
const OVERLAY: &str = "tailscale";

/// Whether an interface is worth reporting: physical, or the overlay.
fn is_reportable_interface(name: &str) -> bool {
	if name == "lo" {
		return false;
	}
	if name.starts_with(OVERLAY) {
		return true;
	}
	// A physical interface has a device behind it in sysfs; a bridge, veth or tunnel does not.
	fs::metadata(format!("/sys/class/net/{name}/device")).is_ok()
}

/// One reading per interface holding an address, grouped so they show together.
pub fn addresses() -> Vec<Reading> {
	let Ok(interfaces) = if_addrs::get_if_addrs() else {
		return Vec::new();
	};

	let mut held: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for interface in interfaces {
		let ip = interface.addr.ip();
		if !is_reportable(ip) || !is_reportable_interface(&interface.name) {
			continue;
		}
		held.entry(interface.name).or_default().push(ip.to_string());
	}

	held.into_iter()
		.map(|(name, mut addresses)| {
			// A stable order, so a client comparing two reports sees only real changes.
			addresses.sort();
			let mut reading = Reading::new(
				format!("address-{name}"),
				name,
				Value::text(addresses.join(", ")),
			)
			.in_group("network");
			if addresses.len() > 1 {
				for address in &addresses {
					reading = reading.with_detail("Address", Value::text(address));
				}
			}
			reading
		})
		.collect()
}

/// Throughput per interface and direction, over the interval since the last sample.
///
/// Yields nothing on the first sample, which has no interval behind it.
pub fn throughput(
	previous: &mut BTreeMap<String, Counters>,
	elapsed: Option<Duration>,
) -> Vec<Reading> {
	let current = counters();
	let readings = match elapsed {
		Some(elapsed) if elapsed.as_secs_f64() > 0.0 => {
			rates(previous, &current, elapsed.as_secs_f64())
		}
		_ => Vec::new(),
	};
	*previous = current;
	readings
}

fn rates(
	previous: &BTreeMap<String, Counters>,
	current: &BTreeMap<String, Counters>,
	seconds: f64,
) -> Vec<Reading> {
	let mut readings = Vec::new();
	for (name, now) in current {
		let Some(then) = previous.get(name) else {
			// An interface that appeared since the last sample has no interval behind it either.
			continue;
		};
		// A counter that went backwards has wrapped or been reset; it says nothing about the interval.
		let (Some(received), Some(sent)) = (
			now.received.checked_sub(then.received),
			now.sent.checked_sub(then.sent),
		) else {
			continue;
		};

		let per_second = |count: u64| (count as f64 / seconds) as u64;
		readings.push(
			Reading::new(
				format!("network-in-{name}"),
				format!("{name} in"),
				rate(per_second(received)),
			)
			.in_group("network-throughput")
			.flowing(Direction::In),
		);
		readings.push(
			Reading::new(
				format!("network-out-{name}"),
				format!("{name} out"),
				rate(per_second(sent)),
			)
			.in_group("network-throughput")
			.flowing(Direction::Out),
		);
	}
	readings
}

/// A rate, in the unit that suits the count, per second.
fn rate(per_second: u64) -> Value {
	let Value::Quantity { number, unit, .. } = bytes(per_second) else {
		return Value::quantity(0.0, "B/s");
	};
	Value::quantity(number, format!("{unit}/s"))
}

/// The cumulative counts, one entry per interface worth reporting.
fn counters() -> BTreeMap<String, Counters> {
	let Ok(raw) = fs::read_to_string("/proc/net/dev") else {
		return BTreeMap::new();
	};

	raw.lines()
		.skip(2)
		.filter_map(|line| {
			let (name, rest) = line.split_once(':')?;
			let name = name.trim();
			if !is_reportable_interface(name) {
				return None;
			}
			let fields: Vec<u64> = rest
				.split_whitespace()
				.map(|field| field.parse().unwrap_or(0))
				.collect();
			// Received bytes first, then eight more received fields before transmitted bytes.
			Some((
				name.to_owned(),
				Counters {
					received: *fields.first()?,
					sent: *fields.get(8)?,
				},
			))
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn loopback_is_never_reported() {
		assert!(!is_reportable_interface("lo"));
	}

	#[test]
	fn the_overlay_is_reported_though_it_is_virtual() {
		assert!(is_reportable_interface("tailscale0"));
	}

	#[test]
	fn the_first_sample_yields_no_rate() {
		let mut previous = BTreeMap::new();
		assert!(throughput(&mut previous, None).is_empty());
		// But the baseline is kept, so the next sample has an interval behind it.
		assert!(!previous.is_empty() || counters().is_empty());
	}

	/// A wrapped or reset counter would otherwise produce an absurd rate from a huge subtraction.
	#[test]
	fn a_counter_going_backwards_yields_no_rate() {
		let previous = BTreeMap::from([(
			"end0".to_owned(),
			Counters {
				received: 1_000_000,
				sent: 1_000_000,
			},
		)]);
		let current = BTreeMap::from([(
			"end0".to_owned(),
			Counters {
				received: 10,
				sent: 10,
			},
		)]);
		assert!(rates(&previous, &current, 1.0).is_empty());
	}

	#[test]
	fn both_directions_are_reported_and_opposed() {
		let previous = BTreeMap::from([(
			"end0".to_owned(),
			Counters {
				received: 0,
				sent: 0,
			},
		)]);
		let current = BTreeMap::from([(
			"end0".to_owned(),
			Counters {
				received: 2_000,
				sent: 1_000,
			},
		)]);
		let readings = rates(&previous, &current, 1.0);
		assert_eq!(readings.len(), 2);

		let inbound = readings[0].direction.clone().unwrap();
		let outbound = readings[1].direction.clone().unwrap();
		assert!(inbound.opposes(&outbound));
		// Grouped together, so a client shows them as one reading with two directions.
		assert_eq!(readings[0].group, readings[1].group);
	}

	/// An interface that appeared since the last sample has no interval behind it.
	#[test]
	fn a_new_interface_yields_no_rate_until_its_second_sample() {
		let current = BTreeMap::from([(
			"wld0".to_owned(),
			Counters {
				received: 5_000,
				sent: 5_000,
			},
		)]);
		assert!(rates(&BTreeMap::new(), &current, 1.0).is_empty());
	}
}
