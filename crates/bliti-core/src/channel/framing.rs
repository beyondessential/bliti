//! Length-prefix framing over a byte transport.
//!
//! Two layers delimit messages by length, at different widths, and this carries both. The transport
//! prefixes each Noise message with two bytes (CHN, "Transport"); the application layer prefixes each
//! message with three (MSG). The widths differ so that code reading one layer's framing cannot read
//! the other's, and each is a const parameter of the read and write pair here, so which layer a call
//! site is reading is part of its type rather than a convention it has to keep.
//!
//! A prefix of a given width can express exactly the messages that fit in it, so the width is itself
//! the bound: a two-byte prefix cannot claim more than a Noise message may be, and a three-byte prefix
//! cannot claim more than a message may be. There is nothing to refuse and no maximum to carry
//! alongside the width.
//!
//! That bound holds only if a sender never writes a length it has narrowed to fit. A truncated length
//! is not a dropped message: the receiver reads the body as framing, and every message after it is off
//! by the difference. So the width is checked where a length is encoded, in the shipped binary rather
//! than in an assertion, and a message too wide for its prefix is an error to the caller.
//!
//! GATT carries reliable, ordered bytes, but a client writes and a device notifies in chunks no larger
//! than the negotiated attribute size, and a Noise message may span several. [`Reassembler`] buffers
//! those chunks and yields whole Noise messages, so a message is not limited by the attribute size.
//! Above the handshake nothing reassembles: yamux streams deliver bytes in order, and
//! [`read_delimited`] reads a message off one directly.

use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The transport framing prefix: two bytes, one Noise message (CHN, "Transport").
pub const TRANSPORT_PREFIX: usize = 2;

/// The application message framing prefix: three bytes, one application message (MSG).
pub const MESSAGE_PREFIX: usize = 3;

/// How much of a message body is pulled off a stream per read while reading it.
const READ_CHUNK: usize = 8192;

/// A width a length prefix can be: at least one byte, and narrow enough that the length it expresses
/// fits a `usize` on every target this runs on. Checked where a width is used, so a width that could
/// not be decoded fails the build rather than the connection.
const fn usable_width<const PREFIX: usize>() {
	assert!(
		PREFIX >= 1 && PREFIX <= 4,
		"a length prefix is one to four bytes wide"
	);
}

/// Decode a big-endian length from a `PREFIX`-byte prefix.
pub fn decode_prefix<const PREFIX: usize>(bytes: &[u8; PREFIX]) -> usize {
	const { usable_width::<PREFIX>() };
	let mut len = [0u8; 8];
	len[8 - PREFIX..].copy_from_slice(bytes);
	u64::from_be_bytes(len) as usize
}

/// Encode `len` as a `PREFIX`-byte big-endian length.
///
/// Fails when the width cannot express it, rather than narrowing the length to fit. The structural
/// bounds of the two layers mean a conforming caller cannot reach that: a Noise message is at most
/// `u16::MAX` bytes against a two-byte prefix, and an application message at most `2^24 - 1` against
/// a three-byte one.
pub fn encode_prefix<const PREFIX: usize>(len: usize) -> io::Result<[u8; PREFIX]> {
	const { usable_width::<PREFIX>() };
	let be = (len as u64).to_be_bytes();
	if be[..8 - PREFIX].iter().any(|byte| *byte != 0) {
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			format!("a {len}-byte message does not fit a {PREFIX}-byte length prefix"),
		));
	}
	Ok(be[8 - PREFIX..].try_into().expect("the width is checked"))
}

/// Write a length-delimited message to a stream and flush it.
///
/// The prefix and the body are written separately rather than copied into one buffer first, so a
/// large message is not duplicated in memory on the way out.
pub async fn write_delimited<const PREFIX: usize, W: AsyncWrite + Unpin>(
	stream: &mut W,
	message: &[u8],
) -> io::Result<()> {
	let prefix = encode_prefix::<PREFIX>(message.len())?;
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
	// A prefix that never began is the end of the stream; one that began and stopped is a message the
	// peer did not finish writing, and reads as the truncation it is rather than as a clean ending.
	// Read for the whole prefix, so the usual case where it has already arrived costs one pass down
	// the stack rather than one per byte.
	let n = stream.read(&mut prefix).await?;
	if n == 0 {
		return Ok(None);
	}
	stream.read_exact(&mut prefix[n..]).await?;

	let mut message = Vec::new();
	let mut remaining = decode_prefix(&prefix);
	while remaining > 0 {
		// Read into the tail of the message itself: an intermediate buffer would copy every byte twice
		// and, at the width's maximum, put a chunk-sized array in this future for the whole read.
		let take = remaining.min(READ_CHUNK);
		let at = message.len();
		message.resize(at + take, 0);
		stream.read_exact(&mut message[at..]).await?;
		remaining -= take;
	}
	Ok(Some(message))
}

/// Reassembles Noise messages from the chunks the link delivers.
///
/// Chunks are pushed in as they arrive, in any sizes, and complete messages are taken out as they
/// become available. Fixed at the transport width, which is the only layer that needs this: above the
/// handshake, yamux delivers a stream's bytes in order and [`read_delimited`] reads straight off it.
#[derive(Debug, Default)]
pub struct Reassembler {
	buf: Vec<u8>,
}

impl Reassembler {
	/// A reassembler reading a two-byte big-endian length before each Noise message.
	pub fn new() -> Self {
		Self::default()
	}

	/// Add a chunk as delivered by the link.
	pub fn push(&mut self, chunk: &[u8]) {
		self.buf.extend_from_slice(chunk);
	}

	/// Take the next complete message, if one is available, or `None` when more bytes are needed.
	pub fn take(&mut self) -> Option<Vec<u8>> {
		let prefix: &[u8; TRANSPORT_PREFIX] = self
			.buf
			.get(..TRANSPORT_PREFIX)?
			.try_into()
			.expect("the width of the slice taken");
		let end = TRANSPORT_PREFIX + decode_prefix(prefix);
		if self.buf.len() < end {
			return None;
		}
		let message = self.buf[TRANSPORT_PREFIX..end].to_vec();
		self.buf.drain(..end);
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
	use futures::{executor::block_on, io::Cursor};

	use super::*;

	/// A Noise message as the sending side puts it on the link: its two-byte length, then its bytes.
	fn framed(message: &[u8]) -> Vec<u8> {
		let mut out = encode_prefix::<TRANSPORT_PREFIX>(message.len())
			.unwrap()
			.to_vec();
		out.extend_from_slice(message);
		out
	}

	#[test]
	fn a_prefix_encodes_and_decodes_at_its_width() {
		assert_eq!(encode_prefix::<TRANSPORT_PREFIX>(2).unwrap(), [0, 2]);
		assert_eq!(encode_prefix::<MESSAGE_PREFIX>(2).unwrap(), [0, 0, 2]);
		assert_eq!(decode_prefix(&[0, 2]), 2);
		assert_eq!(decode_prefix(&[0, 0, 2]), 2);
	}

	#[test]
	fn a_two_byte_prefix_cannot_claim_more_than_a_noise_message() {
		// The largest length a two-byte prefix can express is exactly the largest Noise message, so a
		// reassembler at this width can never be asked to buffer more than the maximum.
		assert_eq!(decode_prefix(&[0xff, 0xff]), 65535);
		assert!(encode_prefix::<TRANSPORT_PREFIX>(65535).is_ok());
		assert!(encode_prefix::<TRANSPORT_PREFIX>(65536).is_err());
	}

	#[test]
	fn a_three_byte_prefix_reaches_sixteen_mebibytes() {
		// The largest length a three-byte message prefix can express is one byte short of 16 MiB, which
		// is the structural bound the removed ceiling used to state as a rule.
		assert_eq!(decode_prefix(&[0xff, 0xff, 0xff]), 16 * 1024 * 1024 - 1);
		assert!(encode_prefix::<MESSAGE_PREFIX>((1 << 24) - 1).is_ok());
		assert!(encode_prefix::<MESSAGE_PREFIX>(1 << 24).is_err());
	}

	#[test]
	fn a_message_too_large_for_the_prefix_is_refused() {
		// Refused rather than narrowed to fit: a truncated length would have the receiver read the body
		// as framing, and the check is in the shipped binary rather than in a debug assertion.
		let err = encode_prefix::<TRANSPORT_PREFIX>(65536).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidInput);

		let mut wire = Vec::new();
		let err = block_on(write_delimited::<TRANSPORT_PREFIX, _>(
			&mut wire,
			&vec![0u8; 65536],
		))
		.unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
		assert!(wire.is_empty(), "nothing goes out on a refused message");
	}

	#[test]
	fn reassembles_a_message_delivered_in_one_chunk() {
		let mut r = Reassembler::new();
		assert_eq!(r.push_and_drain(&framed(b"hello")), vec![b"hello".to_vec()]);
	}

	#[test]
	fn reassembles_a_message_split_across_chunks() {
		let mut r = Reassembler::new();
		let wire = framed(b"a longer message than one chunk");
		// Deliver a byte at a time; only the final byte completes the message.
		for (i, byte) in wire.iter().enumerate() {
			let out = r.push_and_drain(&[*byte]);
			if i + 1 < wire.len() {
				assert!(out.is_empty());
			} else {
				assert_eq!(out, vec![b"a longer message than one chunk".to_vec()]);
			}
		}
	}

	#[test]
	fn separates_several_messages_in_one_chunk() {
		let mut r = Reassembler::new();
		let mut chunk = framed(b"one");
		chunk.extend(framed(b"two"));
		chunk.extend(framed(b"three"));
		assert_eq!(
			r.push_and_drain(&chunk),
			vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()]
		);
	}

	#[test]
	fn holds_a_partial_trailing_message() {
		let mut r = Reassembler::new();
		let mut chunk = framed(b"complete");
		chunk.extend_from_slice(&[0, 10, b'p', b'a', b'r', b't']); // header + 4 of 10 bytes
		assert_eq!(r.push_and_drain(&chunk), vec![b"complete".to_vec()]);
		// The rest of the partial message completes it later.
		assert_eq!(r.push_and_drain(b"ial!!!"), vec![b"partial!!!".to_vec()]);
	}

	#[test]
	fn a_delimited_message_round_trips_through_a_stream() {
		let mut wire = Vec::new();
		block_on(write_delimited::<MESSAGE_PREFIX, _>(
			&mut wire,
			b"a reading",
		))
		.unwrap();
		block_on(write_delimited::<MESSAGE_PREFIX, _>(
			&mut wire,
			b"and another",
		))
		.unwrap();

		let mut stream = Cursor::new(wire);
		let mut read = || block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream)).unwrap();
		assert_eq!(read(), Some(b"a reading".to_vec()));
		assert_eq!(read(), Some(b"and another".to_vec()));
		assert_eq!(read(), None);
	}

	#[test]
	fn a_stream_that_never_began_a_message_reads_as_the_end_of_it() {
		let mut stream = Cursor::new(Vec::new());
		assert_eq!(
			block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream)).unwrap(),
			None
		);
	}

	#[test]
	fn a_message_the_peer_did_not_finish_writing_is_not_a_clean_ending() {
		// A prefix begun and then abandoned: two of the three bytes, and nothing after.
		let mut stream = Cursor::new(vec![0u8, 0]);
		let err = block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream)).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

		// And a body that stops short of what its own prefix claimed.
		let mut stream = Cursor::new(vec![0u8, 0, 10, b'p', b'a', b'r', b't']);
		let err = block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream)).unwrap_err();
		assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
	}

	#[test]
	fn a_message_wider_than_a_two_byte_prefix_allows_round_trips_at_the_message_width() {
		// A message past what the transport width could express crosses a stream under the three-byte
		// one, which is the point of the message layer carrying its own wider prefix.
		let big = vec![b'x'; 70_000];
		let mut wire = Vec::new();
		block_on(write_delimited::<MESSAGE_PREFIX, _>(&mut wire, &big)).unwrap();

		let mut stream = Cursor::new(wire);
		let out = block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream)).unwrap();
		assert_eq!(out, Some(big));
	}

	#[test]
	fn round_trips_a_message_at_the_three_byte_maximum() {
		// The largest message the prefix can express, written and read back. This is the bound itself
		// rather than a value near it: one byte more is unrepresentable, which is what lets the ceiling
		// be a property of the format instead of a rule a receiver has to enforce. Held one buffer at a
		// time, and checked by length and content rather than by comparing two 16 MiB vectors.
		let max = (1 << 24) - 1;
		let mut wire = Vec::with_capacity(MESSAGE_PREFIX + max);
		block_on(write_delimited::<MESSAGE_PREFIX, _>(
			&mut wire,
			&vec![0xa5u8; max],
		))
		.unwrap();
		assert_eq!(&wire[..MESSAGE_PREFIX], &[0xff, 0xff, 0xff]);

		let mut stream = Cursor::new(wire);
		let out = block_on(read_delimited::<MESSAGE_PREFIX, _>(&mut stream))
			.unwrap()
			.unwrap();
		assert_eq!(out.len(), max);
		assert!(out.iter().all(|byte| *byte == 0xa5));
	}
}
