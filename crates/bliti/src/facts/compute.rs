//! Processor and memory.

use std::fs;

use bliti_core::channel::readings::Entry;

use super::{read_number, read_trimmed};

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
pub fn cpu_usage(at: u64, previous: &mut Option<CpuCounters>) -> Option<Entry> {
	let current = counters()?;
	let last = previous.replace(current)?;

	// A counter that has not advanced, or has gone backwards across a suspend, says nothing.
	let total = current.total.checked_sub(last.total)?;
	let busy = current.busy.checked_sub(last.busy)?;
	if total == 0 {
		return None;
	}

	let used = (busy as f64 / total as f64).clamp(0.0, 1.0);
	Some(Entry::fraction(at, "cpu-usage", used))
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

/// Where the processor's current and maximum speed sit, in kilohertz.
const CPUFREQ: &str = "/sys/devices/system/cpu/cpu0/cpufreq";

/// The processor's current speed, reported as `warning` where the platform reports it is limiting
/// the processor. Never inferred from the frequency sitting below its maximum: idle scaling lowers it
/// on a device that is simply not busy (NFO).
pub fn cpu_frequency(at: u64) -> Option<Entry> {
	let khz = read_number(&format!("{CPUFREQ}/scaling_cur_freq"))?;
	let entry = Entry::quantity(at, "cpu-frequency", "hertz", khz as f64 * 1000.0);
	Some(match throttled_reason() {
		Some(reason) => entry.warning(reason),
		None => entry,
	})
}

/// The speed the processor is capable of, in hertz.
pub fn cpu_frequency_max(at: u64) -> Option<Entry> {
	let khz = read_number(&format!("{CPUFREQ}/cpuinfo_max_freq"))?;
	Some(Entry::quantity(
		at,
		"cpu-frequency-max",
		"hertz",
		khz as f64 * 1000.0,
	))
}

/// The Pi firmware's throttling word, where the platform exposes one and reports it is limiting the
/// processor now. Bit 1 is the arm frequency capped, bit 2 is currently throttled; the higher bits
/// are the has-occurred history and are not what "limiting now" means.
fn throttled_reason() -> Option<String> {
	let raw = read_trimmed("/sys/devices/platform/soc/soc:firmware/get_throttled")?;
	let word = u64::from_str_radix(raw.trim_start_matches("0x"), 16).ok()?;
	(word & 0b110 != 0).then(|| "the platform is limiting the processor".to_owned())
}

/// Memory in use, as a fraction.
///
/// Uses available rather than free: the kernel's free figure excludes cache it would hand back on
/// demand, and reporting it would show a healthy machine as nearly full.
pub fn memory_usage(at: u64) -> Option<Entry> {
	let (used, total) = memory()?;
	Some(Entry::fraction(
		at,
		"memory-usage",
		(used as f64 / total as f64).clamp(0.0, 1.0),
	))
}

/// Memory fitted, in bytes.
pub fn memory_total(at: u64) -> Option<Entry> {
	let (_, total) = memory()?;
	Some(Entry::quantity(at, "memory-total", "bytes", total as f64))
}

/// Bytes used and total, from `/proc/meminfo`.
fn memory() -> Option<(u64, u64)> {
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
	Some((total.saturating_sub(available) * 1024, total * 1024))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_first_read_of_the_processor_yields_nothing() {
		let mut previous = None;
		assert!(
			cpu_usage(1, &mut previous).is_none(),
			"no interval behind it"
		);
		assert!(previous.is_some(), "but a baseline is kept");
	}

	#[test]
	fn memory_use_is_a_fraction() {
		let entry = memory_usage(1).expect("every Linux machine has meminfo");
		let used = entry.value.as_ref().and_then(|v| v.as_f64()).unwrap();
		assert!((0.0..=1.0).contains(&used), "{used}");
	}

	#[test]
	fn memory_total_is_a_quantity_in_bytes() {
		let entry = memory_total(1).expect("meminfo");
		assert_eq!(entry.unit.as_deref(), Some("bytes"));
	}

	/// A counter that went backwards, which a suspend can cause, must not produce a nonsense figure.
	#[test]
	fn a_counter_going_backwards_yields_nothing() {
		let mut previous = Some(CpuCounters {
			busy: u64::MAX,
			total: u64::MAX,
		});
		assert!(cpu_usage(1, &mut previous).is_none());
	}
}
