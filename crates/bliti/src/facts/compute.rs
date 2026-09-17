//! Processor and memory use.

use std::fs;

use bliti_core::channel::readings::{Reading, Value};

/// The cumulative processor times one read of `/proc/stat` yields.
///
/// Held between samples because the kernel counts since boot: the figure worth reporting is how much
/// of the interval between two samples was spent working, which a single read cannot give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuCounters {
	busy: u64,
	total: u64,
}

/// Processor use across all cores, as a fraction of the interval since the last sample.
///
/// Yields nothing on the first sample, which has no interval behind it.
pub fn cpu(previous: &mut Option<CpuCounters>) -> Option<Reading> {
	let current = counters()?;
	let last = previous.replace(current)?;

	// A counter that has not advanced, or has gone backwards across a suspend, says nothing.
	let total = current.total.checked_sub(last.total)?;
	let busy = current.busy.checked_sub(last.busy)?;
	if total == 0 {
		return None;
	}

	let used = (busy as f64 / total as f64).clamp(0.0, 1.0);
	Some(Reading::new("cpu", "CPU", Value::Fraction(used)))
}

/// The aggregate line of `/proc/stat`, which sums every core.
fn counters() -> Option<CpuCounters> {
	let raw = fs::read_to_string("/proc/stat").ok()?;
	let line = raw.lines().next()?.strip_prefix("cpu ")?;
	let fields: Vec<u64> = line
		.split_whitespace()
		.filter_map(|field| field.parse().ok())
		.collect();
	// user, nice, system, idle, iowait, irq, softirq, steal, ...
	if fields.len() < 4 {
		return None;
	}
	let total: u64 = fields.iter().sum();
	// Idle and iowait are both time not spent working.
	let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
	Some(CpuCounters {
		busy: total.saturating_sub(idle),
		total,
	})
}

/// Memory in use, as a fraction, with the figures behind it.
///
/// Uses available rather than free: the kernel's free figure excludes cache it would hand back on
/// demand, and reporting it would show a healthy machine as nearly full.
pub fn memory() -> Option<Reading> {
	let raw = fs::read_to_string("/proc/meminfo").ok()?;
	let kib = |name: &str| -> Option<u64> {
		raw.lines()
			.find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
			.and_then(|rest| rest.split_whitespace().next())
			.and_then(|value| value.parse().ok())
	};

	let total = kib("MemTotal")?;
	let available = kib("MemAvailable").unwrap_or_else(|| kib("MemFree").unwrap_or(0));
	if total == 0 {
		return None;
	}
	let used = total.saturating_sub(available);

	Some(
		Reading::new(
			"memory",
			"Memory",
			Value::Fraction((used as f64 / total as f64).clamp(0.0, 1.0)),
		)
		.with_detail("Used", bytes(used * 1024))
		.with_detail("Total", bytes(total * 1024)),
	)
}

/// A byte count as a quantity in the largest unit that leaves it above one.
pub fn bytes(count: u64) -> Value {
	const UNITS: [(&str, f64); 4] = [
		("GB", 1_000_000_000.0),
		("MB", 1_000_000.0),
		("kB", 1_000.0),
		("B", 1.0),
	];
	let count = count as f64;
	for (unit, scale) in UNITS {
		if count >= scale {
			return Value::quantity((count / scale * 10.0).round() / 10.0, unit);
		}
	}
	Value::quantity(0.0, "B")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_first_read_of_the_processor_yields_nothing() {
		let mut previous = None;
		assert!(cpu(&mut previous).is_none(), "no interval behind it");
		assert!(previous.is_some(), "but a baseline is kept");
	}

	#[test]
	fn memory_is_a_fraction_with_its_figures_behind_it() {
		let reading = memory().expect("every Linux machine has meminfo");
		let Some(Value::Fraction(used)) = reading.value else {
			panic!("memory use is a fraction")
		};
		assert!((0.0..=1.0).contains(&used), "{used}");
		assert_eq!(reading.detail.len(), 2);
	}

	#[test]
	fn a_byte_count_takes_the_unit_that_suits_it() {
		assert_eq!(bytes(0), Value::quantity(0.0, "B"));
		assert_eq!(bytes(512), Value::quantity(512.0, "B"));
		assert_eq!(bytes(1_500), Value::quantity(1.5, "kB"));
		assert_eq!(bytes(2_000_000), Value::quantity(2.0, "MB"));
		assert_eq!(bytes(441_000_000_000), Value::quantity(441.0, "GB"));
	}

	/// A counter that went backwards, which a suspend can cause, must not produce a nonsense figure.
	#[test]
	fn a_counter_going_backwards_yields_nothing() {
		let mut previous = Some(CpuCounters {
			busy: u64::MAX,
			total: u64::MAX,
		});
		assert!(cpu(&mut previous).is_none());
	}
}
