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
//! That bound holds only if a sender never writes a length it has narrowed to fit. A truncated length
//! is not a dropped message: the receiver reads the body as framing, and every message after it is off
//! by the difference. So the width is checked where a message is framed, in the shipped binary rather
//! than in an assertion, and a message too wide for its prefix is an error to the caller.
//!
//! GATT carries reliable, ordered bytes, but a client writes and a device notifies in chunks no larger
//! than the negotiated attribute size, and a message may span several. [`Reassembler`] buffers those
//! chunks and yields whole messages, so a message is not limited by the attribute size.

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The transport framing prefix: two bytes, one Noise message (CHN, "Transport").
pub const TRANSPORT_PREFIX: usize = 2;

/// The application message framing prefix: three bytes, one application message (MSG).
pub const MESSAGE_PREFIX: usize = 3;

/// How much of a message body is pulled off a stream per read while reading it.
const READ_CHUNK: usize = 8192;

/// Decode a big-endian length from a prefix of `bytes.len()` bytes, which is at most eight.
pub fn decode_prefix(bytes: &[u8]) -> usize {
	debug_assert!(bytes.len() <= 8, "a length prefix is at most eight bytes");
	let mut len = [0u8; 8];
	len[8 - bytes.len()..].copy_from_slice(bytes);
	u64::from_be_bytes(len) as usize
}

/// Encode `len` as a `PREFIX`-byte big-endian length, or `None` when the width cannot express it.
pub fn encode_prefix<const PREFIX: usize>(len: usize) -> Option<[u8; PREFIX]> {
	let be = (len as u64).to_be_bytes();
	if PREFIX > 8 || be[..8 - PREFIX].iter().any(|byte| *byte != 0) {
		return None;
	}
	Some(be[8 - PREFIX..].try_into().expect("the width is checked"))
}

fn too_wide<const PREFIX: usize>(len: usize) -> io::Error {
	io::Error::new(
		io::ErrorKind::InvalidInput,
		format!("a {len}-byte message does not fit a {PREFIX}-byte length prefix"),
	)
}

/// Frame a message: its length as `PREFIX` big-endian bytes, then the message bytes.
///
/// Fails when the message is wider than the prefix can express. The structural bounds of the two
/// layers mean a conforming caller cannot reach that: a Noise message is at most `u16::MAX` bytes
/// against a two-byte prefix, and an application message at most `2^24 - 1` against a three-byte one.
pub fn frame<const PREFIX: usize>(message: &[u8]) -> io::Result<Vec<u8>> {
	let prefix =
		encode_prefix::<PREFIX>(message.len()).ok_or_else(|| too_wide::<PREFIX>(message.len()))?;
	let mut framed = Vec::with_capacity(PREFIX + message.len());
	framed.extend_from_slice(&prefix);
	framed.extend_from_slice(message);
	Ok(framed)
}

/// Write a length-delimited message to a stream and flush it.
///
/// The prefix and the body are written separately rather than copied into one buffer first, so a
/// large message is not duplicated in memory on the way out.
pub async fn write_delimited<const PREFIX: usize, W: AsyncWrite + Unpin>(
	stream: &mut W,
	message: &[u8],
) -> io::Result<()> {
	let prefix =
		encode_prefix::<PREFIX>(message.len()).ok_or_else(|| too_wide::<PREFIX>(message.len()))?;
	stream.write_all(&prefix).await?;
	stream.write_all(message).await?;
	stream.flush().await
}

/// Read the next length-delimited message from a stream, or `None` at end of stream.
///
/// The body grows with what has actually arrived rather than being reserved from the claimed length:
/// a peer can claim the width's maximum while sending almost nothing, on as many streams as it likes,
/// and the device is the thing that has to stay reachable.
pub async fn read_delimited<const PREFIX: usize, R: AsyncRead + Unpin>(
	stream: &mut R,
) -> io::Result<Option<Vec<u8>>> {
	let mut prefix = [0u8; PREFIX];
	match stream.read_exact(&mut prefix).await {
		Ok(()) => {}
		Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
		Err(err) => return Err(err),
	}
	let mut message = Vec::new();
	let mut chunk = [0u8; READ_CHUNK];
	let mut remaining = decode_prefix(&prefix);
	while remaining > 0 {
		let take = remaining.min(chunk.len());
		stream.read_exact(&mut chunk[..take]).await?;
		message.extend_from_slice(&chunk[..take]);
		remaining -= take;
	}
	Ok(Some(message))
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
		let claimed = decode_prefix(&self.buf[..self.prefix]);
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
		assert_eq!(
			frame::<TRANSPORT_PREFIX>(b"hi").unwrap(),
			vec![0, 2, b'h', b'i']
		);
		assert_eq!(frame::<TRANSPORT_PREFIX>(b"").unwrap(), vec![0, 0]);
	}

	#[test]
	fn frame_prefixes_a_three_byte_length() {
		assert_eq!(
			frame::<MESSAGE_PREFIX>(b"hi").unwrap(),
			vec![0, 0, 2, b'h', b'i']
		);
		assert_eq!(frame::<MESSAGE_PREFIX>(b"").unwrap(), vec![0, 0, 0]);
	}

	#[test]
	fn reassembles_a_message_delivered_in_one_chunk() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let messages = r.push_and_drain(&frame::<TRANSPORT_PREFIX>(b"hello").unwrap());
		assert_eq!(messages, vec![b"hello".to_vec()]);
	}

	#[test]
	fn reassembles_a_message_split_across_chunks() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let framed = frame::<TRANSPORT_PREFIX>(b"a longer message than one chunk").unwrap();
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
		let mut chunk = frame::<TRANSPORT_PREFIX>(b"one").unwrap();
		chunk.extend(frame::<TRANSPORT_PREFIX>(b"two").unwrap());
		chunk.extend(frame::<TRANSPORT_PREFIX>(b"three").unwrap());
		let messages = r.push_and_drain(&chunk);
		assert_eq!(
			messages,
			vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
		);
	}

	#[test]
	fn holds_a_partial_trailing_message() {
		let mut r = Reassembler::new(TRANSPORT_PREFIX);
		let mut chunk = frame::<TRANSPORT_PREFIX>(b"complete").unwrap();
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
		assert_eq!(decode_prefix(&[0xff, 0xff]), 65535);
	}

	#[test]
	fn a_three_byte_prefix_reaches_sixteen_mebibytes() {
		// The largest length a three-byte message prefix can express is one byte short of 16 MiB, which
		// is the structural bound the removed ceiling used to state as a rule.
		assert_eq!(decode_prefix(&[0xff, 0xff, 0xff]), 16 * 1024 * 1024 - 1);
	}

	#[test]
	fn a_prefix_encodes_and_decodes_at_its_width() {
		assert_eq!(encode_prefix::<TRANSPORT_PREFIX>(65535), Some([0xff, 0xff]));
		assert_eq!(encode_prefix::<TRANSPORT_PREFIX>(65536), None);
		assert_eq!(
			encode_prefix::<MESSAGE_PREFIX>((1 << 24) - 1),
			Some([0xff, 0xff, 0xff])
		);
		assert_eq!(encode_prefix::<MESSAGE_PREFIX>(1 << 24), None);
	}

	#[test]
	fn reassembles_a_message_wider_than_a_two_byte_prefix_allows() {
		// A message past what a two-byte prefix could express round-trips under the three-byte one, which
		// is the point of the message layer carrying its own wider prefix.
		let mut r = Reassembler::new(MESSAGE_PREFIX);
		let big = vec![b'x'; 70_000];
		let out = r.push_and_drain(&frame::<MESSAGE_PREFIX>(&big).unwrap());
		assert_eq!(out, vec![big]);
	}

	#[test]
	fn framing_a_message_too_large_for_the_prefix_is_refused() {
		// Refused rather than narrowed to fit: a truncated length would have the receiver read the body
		// as framing, and the check is in the shipped binary rather than in a debug assertion.
		let err = frame::<TRANSPORT_PREFIX>(&vec![0u8; 65536]).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
		let err = frame::<MESSAGE_PREFIX>(&vec![0u8; 1 << 24]).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
	}

	#[test]
	fn round_trips_a_message_at_the_three_byte_maximum() {
		// The largest message the prefix can express, framed and reassembled. This is the bound itself
		// rather than a value near it: one byte more is unrepresentable, which is what lets the ceiling
		// be a property of the format instead of a rule a receiver has to enforce. Held one buffer at a
		// time, and checked by length and content rather than by comparing two 16 MiB vectors.
		let max = (1 << 24) - 1;
		let framed = frame::<MESSAGE_PREFIX>(&vec![0xa5u8; max]).unwrap();
		assert_eq!(&framed[..MESSAGE_PREFIX], &[0xff, 0xff, 0xff]);

		let mut r = Reassembler::new(MESSAGE_PREFIX);
		r.push(&framed);
		drop(framed);
		let out = r.take().unwrap();
		assert_eq!(out.len(), max);
		assert!(out.iter().all(|byte| *byte == 0xa5));
	}
}
