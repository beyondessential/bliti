//! The compression layer: one zlib stream per direction, spanning the connection.
//!
//! Behaviour is specified in CHN, "Compression". [`CompressStream`] wraps the encrypted byte stream
//! and presents another `AsyncRead + AsyncWrite`, deflating everything written and inflating
//! everything read through a single context per direction that lasts as long as the connection. yamux
//! runs on top of it, so the redundancy that lives across messages rather than within one is what the
//! context sees: every sample a device sends repeats the labels, units and state strings of the one
//! before it, and only a shared context squeezes that out.
//!
//! It sits above Noise and below yamux, so it compresses plaintext before it is encrypted: encrypted
//! bytes do not compress, so the order is the only one that works. A decompression failure is a fault
//! in the peer and surfaces as an I/O error carrying a [`ChannelError`], which is what tells the host a
//! fault from an ordinary ending. It tears the whole connection down rather than one stream, because
//! the context is shared by every stream and is unrecoverable once it has diverged.
//!
//! Three rules of the pipeline are load-bearing, and each is a way to deadlock or leak a connection:
//!
//! - A read drains the decompressor before it pulls from the transport. The decompressor holds output
//!   of its own: a caller asking for twelve bytes of yamux header leaves the frame body sitting inside
//!   it with the transport's buffer already empty. Pulling from the transport first strands those bytes
//!   and reads a busy connection as idle, which deadlocks the moment a frame body arrives with its
//!   header in one chunk.
//! - A flush must leave everything written so far readable by the peer. That is a zlib sync flush, and
//!   it is what the message-boundary guarantee of CHN rests on. The yamux driver flushes the socket on
//!   every iteration of its poll loop, so a flush with nothing written since the last one must emit no
//!   bytes, or an idle connection would dribble empty stored blocks and spend the notification budget
//!   and the radio on them. `miniz_oxide` does emit on a redundant sync flush, so the `dirty` flag here
//!   carries that property rather than the backend.
//! - An inflating read that has consumed input without producing output yet returns `Pending`, never
//!   `Ok(0)`. `Ok(0)` is end of stream to every `AsyncRead` caller, so returning it while merely
//!   waiting for the rest of a deflate block would tear the connection down at random. A transport that
//!   ended mid-block gets an error rather than `Ok(0)` for the mirror of that reason: the block can
//!   never be finished, and a clean end of stream would read a truncated conversation as a whole one.

use std::{
	io,
	pin::Pin,
	task::{Context, Poll},
};

use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use futures::{AsyncRead, AsyncWrite};

use super::{ChannelError, framing::WriteBacklog};

/// The scratch buffer size for both directions: how much compressed output is produced per compressor
/// call, and how much is pulled off the inner transport per read.
const CHUNK: usize = 8192;

/// An encrypted byte stream with one zlib stream layered over each direction (CHN, "Compression").
///
/// Compresses on write and decompresses on read, through a deflate context and an inflate context
/// that are established with the connection and last as long as it. Neither is ever reset, and neither
/// is per stream or per message: yamux multiplexes above this, so the one context carries every
/// stream's bytes.
pub struct CompressStream<T> {
	inner: T,
	compress: Compress,
	decompress: Decompress,

	/// Compressed bytes produced but not yet written to the inner transport.
	out: WriteBacklog,
	out_chunk: Box<[u8]>,
	/// Whether anything has been compressed since the last flush, so an idle flush emits nothing.
	dirty: bool,
	/// Whether the deflate stream has been finished, so `poll_close` finishes it only once.
	finished: bool,

	/// Compressed bytes pulled off the inner transport but not yet inflated.
	in_buf: Vec<u8>,
	in_start: usize,
	in_chunk: Box<[u8]>,
	/// Whether the inner transport has reached end of stream.
	read_eof: bool,
	/// Whether the inflate stream reached its end, which tells a clean close from a truncated one.
	stream_end: bool,
}

impl<T> CompressStream<T> {
	/// Wrap a byte stream, establishing a fresh context in each direction.
	///
	/// No preset dictionary (CHN): one only ever helps the first message, and the vocabulary that
	/// would pay cannot be fixed in the protocol. `zlib_header` is true, so each direction is a zlib
	/// stream of RFC 1950 rather than raw deflate.
	pub fn new(inner: T) -> Self {
		Self {
			inner,
			compress: Compress::new(Compression::default(), true),
			decompress: Decompress::new(true),
			out: WriteBacklog::default(),
			out_chunk: vec![0u8; CHUNK].into_boxed_slice(),
			dirty: false,
			finished: false,
			in_buf: Vec::new(),
			in_start: 0,
			in_chunk: vec![0u8; CHUNK].into_boxed_slice(),
			read_eof: false,
			stream_end: false,
		}
	}
}

impl<T> CompressStream<T> {
	/// Run `input` through the deflate context with `flush`, appending all produced bytes to the
	/// backlog, and return how many bytes of `input` were consumed.
	///
	/// Deflate writes into a scratch buffer that lasts as long as the stream, and only what it produced
	/// is appended. Growing the backlog by a chunk and truncating instead would zero-fill the whole
	/// chunk on every call, which is every write and every flush that has something to say: on a device
	/// that is a chunk-sized memset to carry a yamux frame header.
	fn drive_compress(&mut self, input: &[u8], flush: FlushCompress) -> io::Result<usize> {
		let mut offset = 0;
		loop {
			let before_in = self.compress.total_in();
			let before_out = self.compress.total_out();
			let status = self
				.compress
				.compress(&input[offset..], &mut self.out_chunk, flush)
				.map_err(io::Error::other)?;
			let read = (self.compress.total_in() - before_in) as usize;
			let wrote = (self.compress.total_out() - before_out) as usize;
			self.out.extend(&self.out_chunk[..wrote]);
			offset += read;

			match status {
				Status::StreamEnd => break,
				_ => {
					let filled = wrote == CHUNK;
					// All input taken and the compressor had room to spare, so it has emitted everything it
					// will for this call; or it made no progress at all, which a redundant flush does.
					if (offset >= input.len() && !filled) || (read == 0 && wrote == 0) {
						break;
					}
				}
			}
		}
		Ok(offset)
	}
}

impl<T: AsyncWrite + Unpin> AsyncWrite for CompressStream<T> {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		if buf.is_empty() {
			return Poll::Ready(Ok(0));
		}
		// Clear any backlog first, so a burst cannot grow it without bound.
		std::task::ready!(this.out.poll_drain(&mut this.inner, cx))?;
		let consumed = this.drive_compress(buf, FlushCompress::None)?;
		if consumed == 0 {
			// Unreachable: deflate takes input whenever it has room, and it is given a fresh chunk of it
			// every call. Said plainly rather than as `Ok(0)`, which callers read as a refusal to write.
			return Poll::Ready(Err(io::Error::other(
				"the compressor took no input; refusing to report a zero-length write",
			)));
		}
		this.dirty = true;
		// Best effort; anything left is drained on the next call or on flush.
		let _ = this.out.poll_drain(&mut this.inner, cx)?;
		Poll::Ready(Ok(consumed))
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		let this = self.get_mut();
		// A sync flush makes everything written so far readable by the peer (CHN). Only when something
		// has actually been written since the last flush: a redundant one would emit an empty stored
		// block, and the driver flushes every poll-loop iteration.
		if this.dirty && !this.finished {
			this.drive_compress(&[], FlushCompress::Sync)?;
			this.dirty = false;
		}
		std::task::ready!(this.out.poll_drain(&mut this.inner, cx))?;
		Pin::new(&mut this.inner).poll_flush(cx)
	}

	fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		let this = self.get_mut();
		if !this.finished {
			// Finishing emits everything outstanding, so there is nothing left for a later sync flush to
			// make readable, and running one against a finished compressor would be an error at best.
			this.drive_compress(&[], FlushCompress::Finish)?;
			this.finished = true;
			this.dirty = false;
		}
		std::task::ready!(this.out.poll_drain(&mut this.inner, cx))?;
		Pin::new(&mut this.inner).poll_close(cx)
	}
}

impl<T: AsyncRead + Unpin> AsyncRead for CompressStream<T> {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		if buf.is_empty() {
			return Poll::Ready(Ok(0));
		}
		loop {
			// Once the zlib stream has ended, every later read is that same end: the answer does not
			// depend on the backend still saying so, and no transport that may still be open is pulled on.
			if this.stream_end {
				return Poll::Ready(Ok(0));
			}
			// Inflate into the caller's buffer. This runs even when no compressed input is buffered,
			// because the decompressor holds its own output: a small read can leave decoded bytes inside
			// it that a later read must drain before pulling anything more off the transport. Pulling
			// first would strand them and read the connection as idle when it is not.
			let before_in = this.decompress.total_in();
			let before_out = this.decompress.total_out();
			let status = this
				.decompress
				.decompress(&this.in_buf[this.in_start..], buf, FlushDecompress::None)
				.map_err(|err| {
					io::Error::new(
						io::ErrorKind::InvalidData,
						ChannelError::Decompress(err.to_string()),
					)
				})?;
			let read = (this.decompress.total_in() - before_in) as usize;
			let wrote = (this.decompress.total_out() - before_out) as usize;
			this.in_start += read;
			if this.in_start == this.in_buf.len() {
				this.in_buf.clear();
				this.in_start = 0;
			}
			// Recorded from the status rather than from output having run out: the call that completes the
			// stream both emits the tail of the final block and reports the end, so a graceful close
			// arrives with bytes attached. `miniz_oxide` goes on reporting the end on later calls, which
			// would cover a flag set late, but the end of a stream is not the codec's to remember.
			if let Status::StreamEnd = status {
				this.stream_end = true;
			}
			if wrote > 0 {
				return Poll::Ready(Ok(wrote));
			}
			if this.stream_end {
				return Poll::Ready(Ok(0));
			}
			// Consumed input without producing output: mid-block, so try again against whatever input is
			// left, and fall through to pull more once it is exhausted. Never return `Ok(0)` here.
			if read > 0 {
				continue;
			}

			if this.read_eof {
				// The transport ended. `Ok(0)` only when the zlib stream ended with it, or when nothing
				// ever arrived: otherwise the peer stopped mid-block and whatever was consumed of it
				// cannot be decoded. Reporting that as a clean end of stream would have every caller
				// treat a truncated conversation as a complete one.
				if this.stream_end || this.decompress.total_in() == 0 {
					return Poll::Ready(Ok(0));
				}
				return Poll::Ready(Err(io::ErrorKind::UnexpectedEof.into()));
			}

			let n = std::task::ready!(Pin::new(&mut this.inner).poll_read(cx, &mut this.in_chunk))?;
			if n == 0 {
				this.read_eof = true;
			} else {
				this.in_buf.extend_from_slice(&this.in_chunk[..n]);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use std::collections::VecDeque;

	use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress};
	use futures::{AsyncReadExt, AsyncWriteExt};
	use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

	use super::*;

	type Duplex = CompressStream<Compat<tokio::io::DuplexStream>>;

	/// Two compression streams facing each other over an in-memory duplex.
	fn pair() -> (Duplex, Duplex) {
		let (a, b) = tokio::io::duplex(1 << 20);
		(
			CompressStream::new(a.compat()),
			CompressStream::new(b.compat()),
		)
	}

	/// A message written and flushed with nothing following it is readable by the receiver without any
	/// further input. This is the flush guarantee of CHN, and the most important regression test for
	/// the whole design: were it broken, the read below would hang forever.
	#[tokio::test]
	async fn a_flushed_message_is_readable_without_more_input() {
		let (mut tx, mut rx) = pair();
		tx.write_all(b"a lonely reading").await.unwrap();
		tx.flush().await.unwrap();

		let mut buf = [0u8; 16];
		rx.read_exact(&mut buf).await.unwrap();
		assert_eq!(&buf, b"a lonely reading");
	}

	/// A burst written back to back, flushed once at the end, arrives whole: deferring the flush while
	/// there is more to write compresses the burst as one run.
	#[tokio::test]
	async fn a_deferred_burst_arrives_whole() {
		let (mut tx, mut rx) = pair();
		for _ in 0..10 {
			tx.write_all(b"cpu 41C, mem 63%, net up; ").await.unwrap();
		}
		tx.flush().await.unwrap();

		let mut buf = vec![0u8; 260];
		rx.read_exact(&mut buf).await.unwrap();
		assert_eq!(buf, b"cpu 41C, mem 63%, net up; ".repeat(10));
	}

	/// Both directions carry a compressed stream, not only device to client.
	#[tokio::test]
	async fn both_directions_compress() {
		let (mut a, mut b) = pair();

		a.write_all(b"client to device").await.unwrap();
		a.flush().await.unwrap();
		let mut buf = [0u8; 16];
		b.read_exact(&mut buf).await.unwrap();
		assert_eq!(&buf, b"client to device");

		b.write_all(b"device to client").await.unwrap();
		b.flush().await.unwrap();
		let mut buf = [0u8; 16];
		a.read_exact(&mut buf).await.unwrap();
		assert_eq!(&buf, b"device to client");
	}

	/// A repeat of a message costs far less than the first time it was sent: the second carries almost
	/// nothing but back-references into the window the first filled. Measured on the bytes emitted.
	#[tokio::test]
	async fn one_context_spans_messages_so_a_repeat_costs_less() {
		let sample =
			br#"{"type":"system-sample","at":20308140,"readings":[{"label":"cpu","unit":"C","value":41}]}"#;

		let tap = Tap::default();
		let mut tx = CompressStream::new(tap.clone());
		tx.write_all(sample).await.unwrap();
		tx.flush().await.unwrap();
		let first = tap.len();
		tx.write_all(sample).await.unwrap();
		tx.flush().await.unwrap();
		let second = tap.len() - first;

		assert!(
			second * 2 < first,
			"a warm-context repeat ({second} bytes) should be far smaller than the first ({first} bytes)"
		);

		// And no preset dictionary is needed to read it: a decompressor built with none reads the whole
		// stream back (CHN).
		let mut decompress = Decompress::new(true);
		let mut out = vec![0u8; sample.len() * 2 + 64];
		decompress
			.decompress(&tap.bytes(), &mut out, FlushDecompress::Sync)
			.unwrap();
		out.truncate(decompress.total_out() as usize);
		assert_eq!(out, [sample.as_slice(), sample.as_slice()].concat());
	}

	/// A run of samples reaches a ratio in the region the card measured, flushing after every message as
	/// the message-boundary default does. A regression that reset the context or compressed each message
	/// alone would collapse the ratio toward the per-message figure, so this guards the point of the
	/// design rather than any one rule of it.
	#[tokio::test]
	async fn a_run_of_samples_reaches_a_healthy_ratio() {
		let tap = Tap::default();
		let mut tx = CompressStream::new(tap.clone());
		let mut raw = 0usize;
		for i in 0..300u64 {
			let sample = format!(
				r#"{{"type":"system-sample","at":{},"readings":[{{"label":"cpu","unit":"C","value":{}}},{{"label":"mem","unit":"%","value":{}}},{{"label":"net","unit":"state","value":"up"}}]}}"#,
				20_308_140 + i * 1000,
				40 + i % 5,
				60 + i % 7
			);
			raw += sample.len();
			tx.write_all(sample.as_bytes()).await.unwrap();
			tx.flush().await.unwrap();
		}
		let ratio = raw as f64 / tap.len() as f64;
		assert!(
			ratio > 6.0,
			"a shared context should reach a healthy ratio, got {ratio:.1}x ({raw} -> {})",
			tap.len()
		);
	}

	/// The third rule of the module doc: a partial deflate block reads as `Pending`, not as `Ok(0)`.
	#[test]
	fn a_partial_block_reads_as_pending_not_end_of_stream() {
		// The zlib header and one byte of a block: enough to consume, not enough to emit.
		let mut compress = Compress::new(Compression::default(), true);
		let mut full = vec![0u8; 64];
		compress
			.compress(b"hello", &mut full, FlushCompress::Sync)
			.unwrap();
		full.truncate(compress.total_out() as usize);
		let partial = full[..3].to_vec();

		let mut cs = CompressStream::new(Scripted::quiet([partial]));
		let mut buf = [0u8; 64];
		let waker = futures::task::noop_waker();
		let mut cx = Context::from_waker(&waker);
		assert!(matches!(
			Pin::new(&mut cs).poll_read(&mut cx, &mut buf),
			Poll::Pending
		));
	}

	/// A peer that closes properly reads as a clean end of stream, not as a truncation. The call that
	/// completes the zlib stream both emits the tail of the final block and reports the end, so the end
	/// has to be recorded when the status says so rather than when output happens to run out.
	#[tokio::test]
	async fn a_closed_stream_reads_as_a_clean_end_of_stream() {
		let (mut tx, mut rx) = pair();
		tx.write_all(b"the last thing said").await.unwrap();
		tx.close().await.unwrap();

		let mut out = Vec::new();
		rx.read_to_end(&mut out).await.unwrap();
		assert_eq!(out, b"the last thing said");

		// And the end is the same end however often it is asked for.
		let mut buf = [0u8; 8];
		assert_eq!(rx.read(&mut buf).await.unwrap(), 0);
	}

	/// A transport that ends before the zlib stream does is an error, not a clean end of stream. The
	/// peer stopped partway through what it was saying, and whatever was consumed of the unfinished
	/// block cannot be decoded; `Ok(0)` would have every caller above take a truncated exchange for a
	/// complete one. Unlike a corrupt stream it is not laid at the peer's door, because a client that
	/// walks out of range ends a connection the same way.
	#[test]
	fn a_truncated_stream_is_not_a_clean_end_of_stream() {
		let mut compress = Compress::new(Compression::default(), true);
		let mut full = vec![0u8; 256];
		compress
			.compress(
				b"a reading that got cut off",
				&mut full,
				FlushCompress::Sync,
			)
			.unwrap();
		full.truncate(compress.total_out() as usize);
		// Everything but the tail of the sync marker, and then the transport ends.
		let truncated = full[..full.len() - 3].to_vec();

		let mut cs = CompressStream::new(Scripted::ending([truncated]));
		let mut buf = [0u8; 64];
		let waker = futures::task::noop_waker();
		let mut cx = Context::from_waker(&waker);
		// Whatever does decode is served first; the end of the transport is what must not read as clean.
		loop {
			match Pin::new(&mut cs).poll_read(&mut cx, &mut buf) {
				Poll::Ready(Ok(0)) => {
					panic!("a truncated stream must not read as a clean end of stream")
				}
				Poll::Ready(Ok(_)) => continue,
				Poll::Ready(Err(err)) => {
					assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
					break;
				}
				Poll::Pending => panic!("the transport has ended, so a read cannot be pending"),
			}
		}
	}

	/// A transport that ends without a byte ever having arrived is an ordinary end of stream: there was
	/// no zlib stream to truncate.
	#[test]
	fn a_transport_that_ends_before_anything_arrives_is_a_clean_end_of_stream() {
		let mut cs = CompressStream::new(Scripted::ending([]));
		let mut buf = [0u8; 64];
		let waker = futures::task::noop_waker();
		let mut cx = Context::from_waker(&waker);
		assert!(matches!(
			Pin::new(&mut cs).poll_read(&mut cx, &mut buf),
			Poll::Ready(Ok(0))
		));
	}

	/// A corrupt compressed stream is an error, which above yamux tears the whole connection down.
	#[test]
	fn a_corrupt_stream_is_an_error() {
		let mut cs = CompressStream::new(Scripted::quiet([vec![0xff; 32]]));
		let mut buf = [0u8; 64];
		let waker = futures::task::noop_waker();
		let mut cx = Context::from_waker(&waker);
		assert!(matches!(
			Pin::new(&mut cs).poll_read(&mut cx, &mut buf),
			Poll::Ready(Err(_))
		));
	}

	/// The second rule of the module doc: an idle flush emits nothing, so an idle connection stays silent
	/// under the driver's per-iteration flush. Carried by the `dirty` guard, not by the backend.
	#[tokio::test]
	async fn an_idle_flush_emits_nothing() {
		let tap = Tap::default();
		let mut tx = CompressStream::new(tap.clone());

		tx.write_all(b"a reading").await.unwrap();
		tx.flush().await.unwrap();
		let after_first = tap.len();
		assert!(
			after_first > 0,
			"the first flush emits the data and a marker"
		);

		// Flush again with nothing written since: an idle connection must not dribble bytes.
		tx.flush().await.unwrap();
		assert_eq!(tap.len(), after_first, "an idle flush must emit no bytes");
	}

	/// The backend behaviour the `dirty` guard defends against, pinned: if a future `miniz_oxide` stops
	/// emitting on a redundant sync flush, the guard can be reconsidered.
	#[test]
	fn miniz_emits_on_a_redundant_sync_flush() {
		let mut compress = Compress::new(Compression::default(), true);
		let mut out = vec![0u8; 64];
		compress
			.compress(b"a reading", &mut out, FlushCompress::Sync)
			.unwrap();
		let before = compress.total_out();
		compress
			.compress(&[], &mut out, FlushCompress::Sync)
			.unwrap();
		assert!(
			compress.total_out() > before,
			"miniz_oxide emits on a redundant sync flush"
		);
	}

	/// The backend behaviour the end-of-stream flag no longer depends on, pinned alongside the flush one:
	/// `miniz_oxide` reports the end of a stream on the call that emits the tail of the final block, with
	/// output attached, and goes on reporting it afterwards. A wrapper that recorded the end only when
	/// output ran out would be correct by that second half alone, and would report a graceful close as a
	/// truncated stream against a backend that stopped saying so.
	#[test]
	fn miniz_reports_the_end_of_a_stream_with_output_and_keeps_reporting_it() {
		let mut compress = Compress::new(Compression::default(), true);
		let mut full = vec![0u8; 256];
		compress
			.compress(b"the last thing said", &mut full, FlushCompress::None)
			.unwrap();
		let at = compress.total_out() as usize;
		compress
			.compress(&[], &mut full[at..], FlushCompress::Finish)
			.unwrap();
		full.truncate(compress.total_out() as usize);

		let mut decompress = Decompress::new(true);
		let mut out = vec![0u8; 256];
		let status = decompress
			.decompress(&full, &mut out, FlushDecompress::None)
			.unwrap();
		assert_eq!(status, Status::StreamEnd);
		assert_eq!(decompress.total_out(), 19, "the end arrives with output");

		let status = decompress
			.decompress(&[], &mut out, FlushDecompress::None)
			.unwrap();
		assert_eq!(status, Status::StreamEnd, "and is reported again after");
	}

	/// An `AsyncWrite` that keeps every byte written to it, so a test can weigh the compressed output.
	#[derive(Clone, Default)]
	struct Tap(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

	impl Tap {
		fn len(&self) -> usize {
			self.0.borrow().len()
		}

		fn bytes(&self) -> Vec<u8> {
			self.0.borrow().clone()
		}
	}

	impl AsyncWrite for Tap {
		fn poll_write(
			self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
			buf: &[u8],
		) -> Poll<io::Result<usize>> {
			self.0.borrow_mut().extend_from_slice(buf);
			Poll::Ready(Ok(buf.len()))
		}

		fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}

		fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}
	}

	/// An `AsyncRead` that yields a scripted sequence of chunks, then either stays pending or ends. A
	/// chunk is assumed to fit the caller's buffer, which the tests here guarantee.
	struct Scripted {
		chunks: VecDeque<Vec<u8>>,
		then_eof: bool,
	}

	impl Scripted {
		/// Chunks, then pending forever: a peer that has gone quiet without going away.
		fn quiet(chunks: impl IntoIterator<Item = Vec<u8>>) -> Self {
			Self {
				chunks: chunks.into_iter().collect(),
				then_eof: false,
			}
		}

		/// Chunks, then end of transport: a peer that went away mid-sentence.
		fn ending(chunks: impl IntoIterator<Item = Vec<u8>>) -> Self {
			Self {
				chunks: chunks.into_iter().collect(),
				then_eof: true,
			}
		}
	}

	impl AsyncRead for Scripted {
		fn poll_read(
			mut self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
			buf: &mut [u8],
		) -> Poll<io::Result<usize>> {
			match self.chunks.pop_front() {
				Some(chunk) => {
					let n = chunk.len().min(buf.len());
					buf[..n].copy_from_slice(&chunk[..n]);
					Poll::Ready(Ok(n))
				}
				None if self.then_eof => Poll::Ready(Ok(0)),
				None => Poll::Pending,
			}
		}
	}
}
