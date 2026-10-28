//! What carries a message stream, and so how its messages are delimited: a stream of the channel, or a
//! connection to a daemon's local socket.
//!
//! The curve stream and the provisioning stream are each served the same way whichever carries them,
//! so the command line and the console reach the running daemon with the messages a client sends.

use std::io;

use bliti_core::channel::{
	messages::Message,
	stream::{read_message, write_message},
};
use futures::{
	AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Stream,
	io::BufReader,
};

/// The longest line the socket reads, the most a message on the channel may be.
const LINE_LIMIT: u64 = 1 << 24;

/// What carries a curve stream, and so how its messages are delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
	/// A stream of the channel, each message behind its length (MSG).
	Channel,
	/// A connection to the daemon's local socket, each message a line of JSON.
	Socket,
}

impl Carrier {
	pub(crate) fn name(self) -> &'static str {
		match self {
			Self::Channel => "channel",
			Self::Socket => "socket",
		}
	}

	/// The next message, `None` at the end of the stream.
	pub(crate) async fn read<R: AsyncBufRead + Unpin>(
		self,
		reader: &mut R,
	) -> io::Result<Option<Vec<u8>>> {
		match self {
			Self::Channel => read_message(reader).await,
			Self::Socket => loop {
				let mut line = Vec::new();
				let n = (&mut *reader)
					.take(LINE_LIMIT)
					.read_until(b'\n', &mut line)
					.await?;
				if n == 0 {
					return Ok(None);
				}
				if line.last() != Some(&b'\n') && n as u64 == LINE_LIMIT {
					return Err(io::Error::new(
						io::ErrorKind::InvalidData,
						"a line longer than a message may be",
					));
				}
				while line.last().is_some_and(u8::is_ascii_whitespace) {
					line.pop();
				}
				if !line.is_empty() {
					return Ok(Some(line));
				}
			},
		}
	}

	pub(crate) async fn write<W: AsyncWrite + Unpin>(
		self,
		writer: &mut W,
		message: &Message,
	) -> io::Result<()> {
		let bytes = message.to_json();
		match self {
			Self::Channel => write_message(writer, &bytes).await,
			Self::Socket => {
				// Compact JSON, whose strings escape any newline, so a message is always one line.
				writer.write_all(&bytes).await?;
				writer.write_all(b"\n").await?;
				writer.flush().await
			}
		}
	}
}

/// The messages arriving on what `carrier` carries, read from `reader`.
///
/// Reading a message is not cancel-safe, and the curve stream waits on the curves changing at the
/// same time as it reads, so the read in progress is held here between polls rather than dropped.
/// Ends after the first error.
pub(crate) fn incoming<R: AsyncRead + Unpin>(
	reader: R,
	carrier: Carrier,
) -> impl Stream<Item = io::Result<Vec<u8>>> {
	futures::stream::unfold(Some(BufReader::new(reader)), move |reader| async move {
		let mut reader = reader?;
		match carrier.read(&mut reader).await {
			Ok(Some(raw)) => Some((Ok(raw), Some(reader))),
			Ok(None) => None,
			Err(err) => Some((Err(err), None)),
		}
	})
}
