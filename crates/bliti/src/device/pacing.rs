//! The device's send rate across every session (CHN, "Send rate").

/// What a device may put on the air in any one second, across every session (CHN, "Send rate").
///
/// Payload bytes rather than a count of notifications: a peer may coalesce several of them into one
/// ATT protocol data unit, so the same count occupies the link for very different lengths of time
/// depending on how large each one is and on whether the peer coalesces at all, neither of which the
/// device is told. A device with a backlog takes longer to clear it rather than taking the connection
/// down, which is the outcome worth having: a slow reading beats a dropped session.
const NOTIFY_BYTES_A_SECOND: usize = 40 * 1024;

/// How far ahead of the pacing schedule a send may run before it waits.
///
/// One small notification is worth a few hundred microseconds of the allowance, and sleeping for that
/// rounds up to the timer's granularity and throttles far below the ceiling. Letting sends bunch this
/// far ahead and then waiting off the whole debt at once keeps the long-run rate exact. It costs a
/// burst of `NOTIFY_BYTES_A_SECOND` times this, a couple of hundred bytes.
const NOTIFY_PACING_SLACK: std::time::Duration = std::time::Duration::from_millis(5);

/// How much of the second's allowance a payload of this size spends.
fn notify_pacing(len: usize) -> std::time::Duration {
	std::time::Duration::from_nanos((len as u64 * 1_000_000_000) / NOTIFY_BYTES_A_SECOND as u64)
}

/// Holds the device to the send rate of CHN, by payload rather than by counting notifications.
///
/// Paced rather than windowed: clearing a counter each second lets one second's allowance land at its
/// end and the next at its start, putting twice the ceiling on the air across the boundary, which is
/// the region the ceiling exists to stay out of.
pub struct Pacer {
	next_send: std::time::Instant,
}

impl Pacer {
	pub fn new(now: std::time::Instant) -> Self {
		Self { next_send: now }
	}

	/// How long to hold `len` payload bytes back before putting them on the air, if at all.
	pub fn wait_for(&mut self, now: std::time::Instant, len: usize) -> Option<std::time::Duration> {
		let wait = match self.next_send.checked_duration_since(now) {
			Some(wait) if wait >= NOTIFY_PACING_SLACK => Some(wait),
			Some(_) => None,
			None => {
				// A quiet stretch is not credit towards a later burst.
				self.next_send = now;
				None
			}
		};
		self.next_send += notify_pacing(len);
		wait
	}
}

#[cfg(test)]
mod tests {
	use std::time::{Duration, Instant};

	use super::*;

	/// The chunk the transport actually hands the pump (`NOTIFY_CHUNK` in `gatt.rs`).
	const CHUNK: usize = 20;

	/// What the slack buys a sender on top of the second's allowance.
	fn burst_allowance() -> usize {
		(NOTIFY_BYTES_A_SECOND as f64 * NOTIFY_PACING_SLACK.as_secs_f64()) as usize + CHUNK
	}

	/// Run `chunks` payloads through the pacer, advancing a simulated clock by whatever it asks
	/// for, and return the time each one went out.
	fn schedule(chunks: usize, len: usize) -> (Instant, Vec<Instant>) {
		let start = Instant::now();
		let mut pacer = Pacer::new(start);
		let mut now = start;
		let mut sent = Vec::with_capacity(chunks);
		for _ in 0..chunks {
			if let Some(wait) = pacer.wait_for(now, len) {
				now += wait;
			}
			sent.push(now);
		}
		(start, sent)
	}

	#[test]
	fn a_seconds_allowance_takes_a_second() {
		let (start, sent) = schedule(NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let elapsed = sent.last().expect("sent something").duration_since(start);
		assert!(
			elapsed >= Duration::from_millis(980),
			"a second of payload drained in {elapsed:?}"
		);
	}

	#[test]
	fn no_one_second_window_exceeds_the_ceiling() {
		// Three seconds of continuous offering, so a window straddles every boundary.
		let (_, sent) = schedule(3 * NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let ceiling = NOTIFY_BYTES_A_SECOND + burst_allowance();
		for (i, from) in sent.iter().enumerate() {
			let in_window = sent[i..]
				.iter()
				.take_while(|at| at.duration_since(*from) < Duration::from_secs(1))
				.count() * CHUNK;
			assert!(
				in_window <= ceiling,
				"{in_window} bytes went out in one second, over the {ceiling} allowed"
			);
		}
	}

	#[test]
	fn a_quiet_stretch_is_not_saved_up() {
		let start = Instant::now();
		let mut pacer = Pacer::new(start);
		let idle = start + Duration::from_secs(5);

		// The first payload after the lull goes straight out, however long the lull was.
		assert_eq!(pacer.wait_for(idle, NOTIFY_BYTES_A_SECOND), None);

		// Having just spent the whole allowance, the next one waits about a second, rather than
		// drawing on five seconds of silence.
		let wait = pacer.wait_for(idle, CHUNK).expect("must be held back");
		assert!(
			wait >= Duration::from_millis(980),
			"held back only {wait:?}"
		);
	}

	#[test]
	fn the_ceiling_is_the_same_in_bytes_whatever_the_payload_size() {
		// A count-based ceiling would let the larger payload put far more on the air.
		let small = schedule(NOTIFY_BYTES_A_SECOND / CHUNK, CHUNK);
		let large = schedule(NOTIFY_BYTES_A_SECOND / 500, 500);
		let small_span = small.1.last().unwrap().duration_since(small.0);
		let large_span = large.1.last().unwrap().duration_since(large.0);
		let difference = small_span.abs_diff(large_span);
		assert!(
			difference < Duration::from_millis(50),
			"same bytes took {small_span:?} at {CHUNK}B but {large_span:?} at 500B"
		);
	}
}
