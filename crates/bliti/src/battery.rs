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
	config::path,
	envelope::{Reading, read},
	messages::{Message, Span},
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, Stream, StreamExt};
use serde_json::Value as Json;

use crate::{
	carrier::{Carrier, incoming},
	facts::{
		Supply, Unchanged,
		curve::{self, Document},
	},
	session::{Peer, SessionError},
};

pub mod cli;
#[cfg(unix)]
pub mod socket;
#[cfg(test)]
mod tests;

/// Places `lasts` and `recharge` are rounded to, as every number of a curve document is (CRV).
const PLACES: i32 = 4;

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
/// Answers the `curve` with `curves`, then every `load` with `accepted` or `cannot-configure` and
/// every `reset` with `accepted` or `cannot-act`,
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

/// Why a `load` or `reset` was not done: the part at fault, and why.
struct Cannot {
	at: String,
	reason: String,
}

impl Cannot {
	/// Not the fault of any one member.
	fn wholly(reason: impl Into<String>) -> Self {
		Self {
			at: path(&[]),
			reason: reason.into(),
		}
	}
}

/// Load `document`, or why not: a document breaking CRV, no backup supply managed, or the curve file
/// not written.
async fn load(supply: &Supply, document: Json) -> Result<(), Cannot> {
	// A device with nothing to load into says so, rather than finding fault with the document.
	if !supply.managed() {
		return Err(Cannot::wholly(Unchanged::NotManaged.to_string()));
	}
	let document = Document::from_json(&document).map_err(|invalid| Cannot {
		at: invalid.at,
		reason: invalid.reason,
	})?;
	let supply = supply.clone();
	blocking(move || supply.load(document)).await
}

/// Reset to the curve the build carries, or why not.
async fn reset(supply: &Supply) -> Result<(), Cannot> {
	let supply = supply.clone();
	blocking(move || supply.reset()).await
}

/// A change to the curves, which writes the curve file, off the runtime's worker threads.
async fn blocking<E: std::fmt::Display + Send + 'static>(
	change: impl FnOnce() -> Result<(), E> + Send + 'static,
) -> Result<(), Cannot> {
	match tokio::task::spawn_blocking(change).await {
		Ok(result) => result.map_err(|unchanged| Cannot::wholly(unchanged.to_string())),
		Err(err) => Err(Cannot::wholly(format!(
			"the curves could not be changed: {err}"
		))),
	}
}

/// The answer to a `load` or `reset`, logged with the client that asked (CRV): `cannot-configure`
/// for a `load`, which offers a document, and `cannot-act` for a `reset`.
fn settle(asked: &str, result: Result<(), Cannot>, carrier: Carrier, peer: &Peer) -> Message {
	let (client, client_version) = peer.named();
	let over = carrier.name();
	match result {
		Ok(()) => {
			tracing::info!(%asked, %client, %client_version, %over, "curve change asked for and accepted");
			Message::Accepted
		}
		Err(Cannot { at, reason }) => {
			tracing::info!(%asked, %client, %client_version, %over, %at, %reason, "curve change asked for and refused");
			match asked {
				"load" => Message::CannotConfigure { at, reason },
				_ => Message::CannotAct {
					act: asked.to_owned(),
					at,
					reason,
				},
			}
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
