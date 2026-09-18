//! Length-prefix framing over a byte transport.
//!
//! Two layers delimit messages by length, at different widths, and this carries both. The transport
//! prefixes each Noise message with two bytes (CHN, "Transport"); the application layer prefixes each
//! message with three (MSG). The widths differ so that code reading one layer's framing cannot read
//! the other's.
//!
//! A prefix of a given width can express exactly the messages that fit in it, so the width is itself
//! the bound: a two-byte prefix cannot claim more than a Noise message may be, and a three-byte prefix
//! cannot claim more than a message may be. There is nothing to refuse and no maximum to carry
//! alongside the width.
//!
//! GATT carries reliable, ordered bytes, but a client writes and a device notifies in chunks no larger
//! than the negotiated attribute size, and a message may span several. [`Reassembler`] buffers those
//! chunks and yields whole messages, so a message is not limited by the attribute size.

/// The transport framing prefix: two bytes, one Noise message (CHN, "Transport").
pub const TRANSPORT_PREFIX: usize = 2;

/// The application message framing prefix: three bytes, one application message (MSG).
pub const MESSAGE_PREFIX: usize = 3;

/// Frame a message: its length as `prefix` big-endian bytes, then the message bytes.
///
/// The message must fit the width, which the structural bounds of the two layers guarantee: a Noise
/// message is at most `u16::MAX` bytes and a two-byte prefix expresses exactly that, and an
/// application message at most `2^24 - 1` bytes against a three-byte prefix.
pub fn frame(prefix: usize, message: &[u8]) -> Vec<u8> {
	debug_assert!(
		message.len() < (1usize << (8 * prefix)),
		"a {}-byte message does not fit a {prefix}-byte length prefix",
		message.len()
	);
	let len = (message.len() as u64).to_be_bytes();
	let mut framed = Vec::with_capacity(prefix + message.len());
	framed.extend_from_slice(&len[len.len() - prefix..]);
	framed.extend_from_slice(message);
	framed
}

/// Reassembles framed messages from the chunks a transport delivers.
///
/// Chunks are pushed in as they arrive, in any sizes, and complete messages are taken out as they
/// become available. The prefix width is fixed at construction and is the same width [`frame`] was
/// called with on the sending side.
#[derive(Debug)]
pub struct Reassembler {
	buf: Vec<u8>,
	prefix: usize,
}

impl Reassembler {
	/// A reassembler reading a `prefix`-byte big-endian length before each message.
	pub fn new(prefix: usize) -> Self {
		Self {
			buf: Vec::new(),
			prefix,
		}
	}

	/// Add a chunk as delivered by the transport.
	pub fn push(&mut self, chunk: &[u8]) {
		self.buf.extend_from_slice(chunk);
	}

	/// Take the next complete message, if one is available, or `None` when more bytes are needed.
	pub fn take(&mut self) -> Option<Vec<u8>> {
		if self.buf.len() < self.prefix {
			return None;
		}
		let mut len = [0u8; 8];
		len[8 - self.prefix..].copy_from_slice(&self.buf[..self.prefix]);
		let claimed = u64::from_be_bytes(len) as usize;
		if self.buf.len() < self.prefix + claimed {
			return None;
		}
		let message = self.buf[self.prefix..self.prefix + claimed].to_vec();
		self.buf.drain(..self.prefix + claimed);
		Some(message)
	}

	/// Push a chunk and take every message it completes.
	pub fn push_and_drain(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
		self.push(chunk);
		let mut messages = Vec::new();
		while let Some(message) = self.take() {
			messages.push(message);
		}
		messages
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn frame_prefixes_a_two_byte_length() {
		assert_eq!(frame(TRANSPORT_PREFIX, b"hi"), vec![0, 2, b'h', b'i']);
		assert_eq!(frame(TRANSPORT_PREFIX, b""), vec![0, 0]);
	}

	#[test]
	fn frame_prefixes_a_three_byte_length() {
		assert_eq!(frame(MESSAGE_PREFIX, b"hi"), vec![0, 0, 2, b'h', b'i']);
		assert_eq!(frame(MESSAGE_PREFIX, b""), vec![0, 0, 0]);
	}

	#[test]
	fn reassembles_a_message_delivered_in_one_chunk() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let messages = r.push_and_drain(&frame(TRANSPORT_PREFIX, b"hello"));
		assert_eq!(messages, vec![b"hello".to_vec()]);
	}

	#[test]
	fn reassembles_a_message_split_across_chunks() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let framed = frame(TRANSPORT_PREFIX, b"a longer message than one chunk");
		// Deliver a byte at a time; only the final byte completes the message.
		for (i, byte) in framed.iter().enumerate() {
			let out = r.push_and_drain(&[*byte]);
			if i + 1 < framed.len() {
				assert!(out.is_empty());
			} else {
				assert_eq!(out, vec![b"a longer message than one chunk".to_vec()]);
			}
		}
	}

	#[test]
	fn separates_several_messages_in_one_chunk() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let mut chunk = frame(TRANSPORT_PREFIX, b"one");
		chunk.extend(frame(TRANSPORT_PREFIX, b"two"));
		chunk.extend(frame(TRANSPORT_PREFIX, b"three"));
		let messages = r.push_and_drain(&chunk);
		assert_eq!(
			messages,
			vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
		);
	}

	#[test]
	fn holds_a_partial_trailing_message() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let mut chunk = frame(TRANSPORT_PREFIX, b"complete");
		chunk.extend_from_slice(&[0, 10, b'p', b'a', b'r', b't']); // header + 4 of 10 bytes
		let messages = r.push_and_drain(&chunk);
		assert_eq!(messages, vec![b"complete".to_vec()]);
		// The rest of the partial message completes it later.
		let messages = r.push_and_drain(b"ial!!!");
		assert_eq!(messages, vec![b"partial!!!".to_vec()]);
	}

	#[test]
	fn a_two_byte_prefix_cannot_claim_more_than_a_noise_message() {
		// The largest length a two-byte prefix can express is exactly the largest Noise message, so a
		// reassembler at this width can never be asked to buffer more than the maximum.
		let mut len = [0u8; 8];
		len[6..].copy_from_slice(&[0xff, 0xff]);
		assert_eq!(u64::from_be_bytes(len) as usize, 65535);
	}

	#[test]
	fn a_three_byte_prefix_reaches_sixteen_mebibytes() {
		// The largest length a three-byte message prefix can express is one byte short of 16 MiB, which
		// is the structural bound the removed ceiling used to state as a rule.
		let mut len = [0u8; 8];
		len[5..].copy_from_slice(&[0xff, 0xff, 0xff]);
		assert_eq!(u64::from_be_bytes(len) as usize, 16 * 1024 * 1024 - 1);
	}

	#[test]
	fn reassembles_a_message_wider_than_a_two_byte_prefix_allows() {
		// A message past what a two-byte prefix could express round-trips under the three-byte one, which
		// is the point of the message layer carrying its own wider prefix.
		let mut r = Reassembler::new(MESSAGE_PREFIX);
		let big = vec![b'x'; 70_000];
		let out = r.push_and_drain(&frame(MESSAGE_PREFIX, &big));
		assert_eq!(out, vec![big]);
	}

	#[test]
	#[should_panic(expected = "does not fit")]
	fn framing_a_message_too_large_for_the_prefix_is_a_caller_error() {
		// A message larger than the width can express is a caller error the structural bounds of the two
		// layers prevent; the debug assertion catches it in tests rather than truncating the length.
		let _ = frame(TRANSPORT_PREFIX, &vec![0u8; 65536]);
	}

	#[test]
	fn round_trips_a_message_at_the_three_byte_maximum() {
		// The largest message the prefix can express, framed and reassembled byte for byte. This is the
		// bound itself rather than a value near it: one byte more is unrepresentable, which is what lets
		// the ceiling be a property of the format instead of a rule a receiver has to enforce.
		let max = (1 << 24) - 1;
		let message = vec![0xa5u8; max];
		let mut r = Reassembler::new(MESSAGE_PREFIX);
		let out = r.push_and_drain(&frame(MESSAGE_PREFIX, &message));
		assert_eq!(out.len(), 1);
		assert_eq!(out[0].len(), max);
		assert_eq!(out[0], message);
	}

	#[test]
	#[should_panic(expected = "does not fit")]
	fn a_message_past_the_three_byte_maximum_cannot_be_framed() {
		let _ = frame(MESSAGE_PREFIX, &vec![0u8; 1 << 24]);
	}
}
