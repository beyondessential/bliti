//! Network addresses and throughput.
//!
//! Physical interfaces are reported, wired and wireless, along with the overlay the fleet is reached
//! over. Loopback and other virtual interfaces are left out: they say nothing about whether the
//! device an operator is standing at can be reached (NFO).

use std::{collections::BTreeMap, fs, net::IpAddr, time::Duration};

use bliti_core::channel::readings::{Entry, kind};
use serde_json::Value as Json;

use super::is_reportable;

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

/// The `interface` trait: its name, and the route and overlay that describe it. `route` is `default`
/// on the interface carrying the default route; `overlay` names the overlay where it is one. Both are
/// descriptive: they move without changing which interface is being measured (NFO).
pub(super) fn interface_trait(name: &str, route: Option<&str>) -> Json {
	let mut object = serde_json::Map::new();
	object.insert("name".to_owned(), Json::String(name.to_owned()));
	if Some(name) == route {
		object.insert("route".to_owned(), Json::String("default".to_owned()));
	}
	if name.starts_with(OVERLAY) {
		object.insert("overlay".to_owned(), Json::String(OVERLAY.to_owned()));
	}
	Json::Object(object)
}

/// One `network-address` fact per address held, its kind naming the family.
pub fn addresses(at: u64) -> Vec<Entry> {
	let route = default_route();
	let held = by_interface();
	let mut entries = Vec::new();
	for (name, addresses) in &held {
		for ip in addresses {
			let kind = if ip.is_ipv4() { kind::IPV4 } else { kind::IPV6 };
			entries.push(
				Entry::address(at, "network-address", kind, ip.to_string())
					.with_trait("interface", interface_trait(name, route.as_deref())),
			);
		}
	}
	entries
}

/// Every address worth reporting, by the interface holding it, in a stable order.
fn by_interface() -> BTreeMap<String, Vec<IpAddr>> {
	let Ok(interfaces) = if_addrs::get_if_addrs() else {
		return BTreeMap::new();
	};

	let mut held: BTreeMap<String, Vec<IpAddr>> = BTreeMap::new();
	for interface in interfaces {
		let ip = interface.addr.ip();
		if !is_reportable(ip) || !is_reportable_interface(&interface.name) {
			continue;
		}
		held.entry(interface.name).or_default().push(ip);
	}
	for addresses in held.values_mut() {
		addresses.sort();
	}
	held
}

/// The interface carrying the default route, which is the one most likely to reach this device.
pub(super) fn default_route() -> Option<String> {
	default_route_in(&fs::read_to_string("/proc/net/route").ok()?)
}

/// The interface of the default route with the lowest metric in a `/proc/net/route` table. A device
/// holding candidates up on several interfaces has a default route on each, and the lowest metric is
/// the one traffic takes (LINK).
fn default_route_in(table: &str) -> Option<String> {
	table
		.lines()
		.skip(1)
		.filter_map(|line| {
			let fields: Vec<&str> = line.split_whitespace().collect();
			// A destination of all zeroes is a default route; the metric is the seventh field.
			(fields.get(1) == Some(&"00000000"))
				.then(|| Some((fields.get(6)?.parse::<u32>().ok()?, fields[0])))
				.flatten()
		})
		.min_by_key(|(metric, _)| *metric)
		.map(|(_, name)| name.to_owned())
}

/// Throughput as one reading per interface and direction, never aggregated (NFO).
///
/// Yields nothing on the first sample, which has no interval behind it. An aggregate is a sum a reader
/// can take, and one taken on the device is a figure it cannot break down.
pub fn throughput(
	at: u64,
	previous: &mut BTreeMap<String, Counters>,
	elapsed: Option<Duration>,
) -> Vec<Entry> {
	let current = counters();
	let route = default_route();
	let readings = match elapsed {
		Some(elapsed) if elapsed.as_secs_f64() > 0.0 => rates(
			at,
			previous,
			&current,
			elapsed.as_secs_f64(),
			route.as_deref(),
		),
		_ => Vec::new(),
	};
	*previous = current;
	readings
}

fn rates(
	at: u64,
	previous: &BTreeMap<String, Counters>,
	current: &BTreeMap<String, Counters>,
	seconds: f64,
	route: Option<&str>,
) -> Vec<Entry> {
	let mut entries = Vec::new();
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
		let per_second = |count: u64| (count as f64 / seconds).round();
		let interface = interface_trait(name, route);
		entries.push(
			Entry::quantity(
				at,
				"network-throughput",
				"bytes/second",
				per_second(received),
			)
			.with_trait("interface", interface.clone())
			.with_trait("direction", Json::String("in".to_owned())),
		);
		entries.push(
			Entry::quantity(at, "network-throughput", "bytes/second", per_second(sent))
				.with_trait("interface", interface)
				.with_trait("direction", Json::String("out".to_owned())),
		);
	}
	entries
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

	fn direction(entry: &Entry) -> &str {
		entry
			.traits
			.get("direction")
			.and_then(Json::as_str)
			.unwrap()
	}

	fn interface_name(entry: &Entry) -> &str {
		entry
			.traits
			.get("interface")
			.and_then(|i| i.get("name"))
			.and_then(Json::as_str)
			.unwrap()
	}

	#[test]
	fn loopback_is_never_reported() {
		assert!(!is_reportable_interface("lo"));
	}

	#[test]
	fn the_default_route_is_the_one_with_the_lowest_metric() {
		let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
			wlan0\t00000000\t0164000A\t0003\t0\t0\t101\t00000000\t0\t0\t0\n\
			end0\t00000000\t0164000A\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
			end0\t0064000A\t00000000\t0001\t0\t0\t100\t00FEFFFF\t0\t0\t0\n";
		assert_eq!(default_route_in(table).as_deref(), Some("end0"));
		assert_eq!(default_route_in("Iface\tDestination\n"), None);
	}

	#[test]
	fn the_overlay_is_reported_though_it_is_virtual() {
		assert!(is_reportable_interface("tailscale0"));
	}

	#[test]
	fn the_first_sample_yields_no_rate() {
		let mut previous = BTreeMap::new();
		assert!(throughput(1, &mut previous, None).is_empty());
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
		assert!(rates(1, &previous, &current, 1.0, None).is_empty());
	}

	/// Throughput is one reading per interface and direction, never summed on the device (NFO).
	#[test]
	fn throughput_is_reported_per_interface_and_direction() {
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
		let readings = rates(1, &previous, &current, 1.0, Some("end0"));
		assert_eq!(readings.len(), 4, "two interfaces, two directions each");

		// Each names its interface and direction, and none is an aggregate.
		let end0_in = readings
			.iter()
			.find(|e| interface_name(e) == "end0" && direction(e) == "in")
			.unwrap();
		assert_eq!(end0_in.value.as_ref().and_then(Json::as_f64), Some(2_000.0));
		assert_eq!(end0_in.unit.as_deref(), Some("bytes/second"));
		// The default route is marked descriptively on the interface it is on.
		assert_eq!(
			end0_in
				.traits
				.get("interface")
				.and_then(|i| i.get("route"))
				.and_then(Json::as_str),
			Some("default")
		);
	}

	#[test]
	fn a_new_interface_yields_no_rate_until_its_second_sample() {
		let current = BTreeMap::from([(
			"wld0".to_owned(),
			Counters {
				received: 5_000,
				sent: 5_000,
			},
		)]);
		assert!(rates(1, &BTreeMap::new(), &current, 1.0, None).is_empty());
	}
}
