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

/// The device's addresses: the ones worth reaching it on up front, and all of them behind the tap.
///
/// The headline is the address an operator is most likely to need, which is the one on the interface
/// carrying the default route, together with the overlay address the fleet is reached over. IPv4 is
/// preferred for both, being the one a person can read out and type; an interface with no IPv4 falls
/// back to what it has.
pub fn addresses() -> Option<Reading> {
	let held = by_interface();
	if held.is_empty() {
		return None;
	}

	let route = default_route();
	let primary = held
		.iter()
		.filter(|(name, _)| !name.starts_with(OVERLAY))
		.min_by_key(|(name, _)| (Some(name.as_str()) != route.as_deref(), (*name).clone()))
		.and_then(|(_, addresses)| preferred(addresses));
	let overlay = held
		.iter()
		.find(|(name, _)| name.starts_with(OVERLAY))
		.and_then(|(_, addresses)| preferred(addresses));

	let headline: Vec<String> = [primary, overlay].into_iter().flatten().collect();
	if headline.is_empty() {
		return None;
	}

	let mut reading = Reading::new("address", "Address", Value::text(headline.join("  ·  ")));
	// Every address with the interface it belongs to, which is what someone diagnosing needs and what
	// the headline deliberately leaves out.
	for (name, addresses) in &held {
		for address in addresses {
			reading = reading.with_detail(name, Value::text(address));
		}
	}
	Some(reading)
}

/// IPv4 where the interface has one, because it is the address a person can read out and type.
fn preferred(addresses: &[String]) -> Option<String> {
	addresses
		.iter()
		.find(|address| !address.contains(':'))
		.or_else(|| addresses.first())
		.cloned()
}

/// Every address worth reporting, by the interface holding it.
fn by_interface() -> BTreeMap<String, Vec<String>> {
	let Ok(interfaces) = if_addrs::get_if_addrs() else {
		return BTreeMap::new();
	};

	let mut held: BTreeMap<String, Vec<String>> = BTreeMap::new();
	for interface in interfaces {
		let ip = interface.addr.ip();
		if !is_reportable(ip) || !is_reportable_interface(&interface.name) {
			continue;
		}
		held.entry(interface.name).or_default().push(ip.to_string());
	}
	// A stable order, so a client comparing two reports sees only real changes.
	for addresses in held.values_mut() {
		addresses.sort();
	}
	held
}

/// The interface carrying the default route, which is the one most likely to reach this device.
fn default_route() -> Option<String> {
	let raw = fs::read_to_string("/proc/net/route").ok()?;
	raw.lines().skip(1).find_map(|line| {
		let mut fields = line.split_whitespace();
		let name = fields.next()?;
		// A destination of all zeroes is the default route.
		(fields.next()? == "00000000").then(|| name.to_owned())
	})
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
	// Summed across interfaces for the headline, with each interface behind it. A device with several
	// links is answering one question at a glance, which is whether anything is moving at all.
	let mut total_in = 0u64;
	let mut total_out = 0u64;
	let mut per_interface: Vec<(String, u64, u64)> = Vec::new();

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
		let (inbound, outbound) = (per_second(received), per_second(sent));
		total_in += inbound;
		total_out += outbound;
		per_interface.push((name.clone(), inbound, outbound));
	}

	if per_interface.is_empty() {
		return Vec::new();
	}

	let mut inbound = Reading::new("network-in", "In", rate(total_in))
		.in_group("network")
		.flowing(Direction::In);
	let mut outbound = Reading::new("network-out", "Out", rate(total_out))
		.in_group("network")
		.flowing(Direction::Out);
	// Only worth breaking down where there is more than one link to break it into.
	if per_interface.len() > 1 {
		for (name, each_in, each_out) in &per_interface {
			inbound = inbound.with_detail(name, rate(*each_in));
			outbound = outbound.with_detail(name, rate(*each_out));
		}
	}
	vec![inbound, outbound]
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
	fn the_headline_prefers_ipv4_and_falls_back_to_what_there_is() {
		assert_eq!(
			preferred(&["2001:db8::1".to_owned(), "192.0.2.10".to_owned()]).as_deref(),
			Some("192.0.2.10")
		);
		assert_eq!(
			preferred(&["2001:db8::1".to_owned()]).as_deref(),
			Some("2001:db8::1")
		);
		assert_eq!(preferred(&[]), None);
	}

	/// The headline is deliberately shorter than the detail: an operator needs one address to reach
	/// the device, and everything else once they are diagnosing.
	#[test]
	fn the_detail_carries_every_address_with_its_interface() {
		let Some(reading) = addresses() else {
			return; // A machine with nothing up is a legitimate answer.
		};
		assert!(reading.is_coherent());
		assert!(!reading.detail.is_empty(), "the detail names every address");
		for entry in &reading.detail {
			assert!(!entry.label.is_empty(), "each address names its interface");
		}
	}

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

	/// One tile answers the question at a glance, and a device with several links sums into it rather
	/// than making an operator add up four numbers.
	#[test]
	fn throughput_is_summed_across_interfaces() {
		let previous = BTreeMap::from([
			(
				"end0".to_owned(),
				Counters {
					received: 0,
					sent: 0,
				},
			),
			(
				"wld0".to_owned(),
				Counters {
					received: 0,
					sent: 0,
				},
			),
		]);
		let current = BTreeMap::from([
			(
				"end0".to_owned(),
				Counters {
					received: 2_000,
					sent: 1_000,
				},
			),
			(
				"wld0".to_owned(),
				Counters {
					received: 3_000,
					sent: 500,
				},
			),
		]);
		let readings = rates(&previous, &current, 1.0);
		assert_eq!(
			readings.len(),
			2,
			"one reading a direction, however many links"
		);
		assert_eq!(readings[0].value, Some(rate(5_000)));
		assert_eq!(readings[1].value, Some(rate(1_500)));
		// Each link is still there, behind the tap.
		assert_eq!(readings[0].detail.len(), 2);
	}

	/// With one link there is nothing to break down, so the detail stays empty.
	#[test]
	fn a_single_interface_gets_no_breakdown() {
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
		assert!(readings[0].detail.is_empty());
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
		// Grouped together, so a client shows them as one reading with two directions, and exactly two
		// so the pair can be drawn mirrored about one axis.
		assert_eq!(readings[0].group.as_deref(), Some("network"));
		assert_eq!(readings[1].group.as_deref(), Some("network"));
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
