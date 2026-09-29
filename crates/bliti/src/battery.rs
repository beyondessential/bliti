//! The curve stream: the battery curves read out of a device, loaded into it, and reset (CRV).
//!
//! Behaviour is specified in `.workhorse/specs/battery/curve.md` (CRV). One [`Supply`] holds the
//! curves in force for the whole device, so every open curve stream is told of each change, whether
//! a client, the command line or a refinement made it.
//!
//! A curve stream is served the same way whatever carries it: a stream of the channel, its messages
//! delimited as MSG delimits them, or a connection to the daemon's local socket, one message a line,
//! through which the command line reaches the running daemon.

use std::io;

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::{Message, Span},
	stream::{read_message, write_message},
};
use futures::{
	AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Stream,
	StreamExt, io::BufReader,
};
use serde_json::Value as Json;

use crate::{
	facts::{
		Supply,
		curve::{self, Document},
	},
	session::{Peer, SessionError},
};

#[cfg(unix)]
pub mod socket;

/// Places `lasts` and `recharge` are rounded to, as every number of a curve document is (CRV).
const PLACES: i32 = 4;

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
	fn name(self) -> &'static str {
		match self {
			Self::Channel => "channel",
			Self::Socket => "socket",
		}
	}

	/// The next message, `None` at the end of the stream.
	async fn read<R: AsyncBufRead + Unpin>(self, reader: &mut R) -> io::Result<Option<Vec<u8>>> {
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

	async fn write<W: AsyncWrite + Unpin>(
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
fn incoming<R: AsyncRead + Unpin>(
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

/// Serve a curve stream a client opened with `curve` on the channel, until it ends (CRV).
pub async fn serve_stream<S>(
	stream: &mut S,
	supply: &Supply,
	peer: &Peer,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let (reader, mut writer) = stream.split();
	let incoming = std::pin::pin!(incoming(reader, Carrier::Channel));
	serve(incoming, &mut writer, Carrier::Channel, supply, peer).await
}

/// Serve a curve stream whose opening `curve` has been read, until it ends (CRV, "The curve
/// stream").
///
/// Answers the `curve` with `curves`, then every `load` and `reset` with `accepted` or `refused`,
/// and sends `curves` again each time the curve document changes. The stream that asked for a change
/// is answered first and then sent the `curves` it brought about, as every other stream is.
async fn serve<I, W>(
	mut incoming: I,
	writer: &mut W,
	carrier: Carrier,
	supply: &Supply,
	peer: &Peer,
) -> Result<(), SessionError>
where
	I: Stream<Item = io::Result<Vec<u8>>> + Unpin,
	W: AsyncWrite + Unpin,
{
	let mut changes = supply.curves();
	let current = changes.borrow_and_update().clone();
	send(writer, carrier, &curves(current.as_ref())).await?;

	loop {
		tokio::select! {
			// First, so a change is sent as soon as the answer to what brought it about.
			biased;

			changed = changes.changed() => {
				if changed.is_err() {
					// The supply is gone, and with it the curves; this holds it, so never.
					return Ok(());
				}
				let current = changes.borrow_and_update().clone();
				send(writer, carrier, &curves(current.as_ref())).await?;
			}

			raw = incoming.next() => {
				let raw = match raw {
					Some(Ok(raw)) => raw,
					None => return Ok(()),
					Some(Err(err)) => return Err(SessionError::Fault(err.to_string())),
				};
				match read::<Message>(&raw) {
					Ok(Reading::Message(Message::Curve)) => {
						let current = changes.borrow_and_update().clone();
						send(writer, carrier, &curves(current.as_ref())).await?;
					}
					Ok(Reading::Message(Message::Load { document })) => {
						let answer = settle("load", load(supply, document).await, carrier, peer);
						send(writer, carrier, &answer).await?;
					}
					Ok(Reading::Message(Message::Reset)) => {
						let answer = settle("reset", reset(supply).await, carrier, peer);
						send(writer, carrier, &answer).await?;
					}
					Ok(Reading::Message(Message::Hello { name, version })) => {
						tracing::info!(client = %name, client_version = %version, "client named itself");
						peer.name(name, version);
					}
					Ok(Reading::Message(_)) => {
						// Nothing a curve stream does anything about, which MSG makes a no-op.
						tracing::debug!("a message the curve stream has nothing to do about");
					}
					Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
					Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
					Err(fault) => {
						tracing::warn!(%fault, "protocol fault; closing the stream");
						return Err(SessionError::Fault(fault.to_string()));
					}
				}
			}
		}
	}
}

/// `curves` for the document in force, none where no backup supply is managed (CRV).
fn curves(document: Option<&Document>) -> Message {
	Message::Curves {
		document: document.map(Document::to_json),
		lasts: document.map(|document| span(document.lasts())),
		recharge: document.and_then(Document::recharge).map(span),
	}
}

fn span(span: curve::Span) -> Span {
	Span {
		duration: round(span.duration),
		margin: round(span.margin),
	}
}

fn round(value: f64) -> f64 {
	let scale = 10f64.powi(PLACES);
	(value * scale).round() / scale
}

/// Load `document`, or why not: a document breaking CRV, no backup supply managed, or the curve file
/// not written.
async fn load(supply: &Supply, document: Json) -> Result<(), String> {
	let document = Document::from_json(&document).map_err(|invalid| invalid.to_string())?;
	let supply = supply.clone();
	blocking(move || supply.load(document)).await
}

/// Reset to the curve the build carries, or why not.
async fn reset(supply: &Supply) -> Result<(), String> {
	let supply = supply.clone();
	blocking(move || supply.reset()).await
}

/// A change to the curves, which writes the curve file, off the runtime's worker threads.
async fn blocking<E: std::fmt::Display + Send + 'static>(
	change: impl FnOnce() -> Result<(), E> + Send + 'static,
) -> Result<(), String> {
	match tokio::task::spawn_blocking(change).await {
		Ok(result) => result.map_err(|unchanged| unchanged.to_string()),
		Err(err) => Err(format!("the curves could not be changed: {err}")),
	}
}

/// The answer to a `load` or `reset`, logged with the client that asked (CRV).
fn settle(asked: &str, result: Result<(), String>, carrier: Carrier, peer: &Peer) -> Message {
	let (client, client_version) = peer.named();
	let over = carrier.name();
	match result {
		Ok(()) => {
			tracing::info!(%asked, %client, %client_version, %over, "curve change asked for and accepted");
			Message::Accepted
		}
		Err(reason) => {
			tracing::info!(%asked, %client, %client_version, %over, %reason, "curve change asked for and refused");
			Message::Refused { reason }
		}
	}
}

async fn send<W: AsyncWrite + Unpin>(
	writer: &mut W,
	carrier: Carrier,
	message: &Message,
) -> Result<(), SessionError> {
	carrier
		.write(writer, message)
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}
