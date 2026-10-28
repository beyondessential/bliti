//! The provisioning stream: `provision-state` out, `provision-act` in, over a stream of the channel
//! or the console's socket (PRS).
//!
//! Served the same way whatever carries it, so a client and the console look at one run.

use std::io;

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::Message,
};
use futures::{
	AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, Stream, StreamExt as _,
};

use super::{Act, CannotAct, Provisioner};
use crate::{
	carrier::{Carrier, incoming},
	session::{Peer, SessionError},
};

/// Serve a provisioning stream a client opened with `provision` on the channel, until it ends.
pub async fn serve_stream<S>(
	stream: &mut S,
	provisioner: &Provisioner,
	peer: &Peer,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let (reader, mut writer) = stream.split();
	let incoming = std::pin::pin!(incoming(reader, Carrier::Channel));
	serve(incoming, &mut writer, Carrier::Channel, provisioner, peer).await
}

/// Serve a provisioning stream whose opening `provision` has been read, until it ends (PRS).
///
/// Sends the state on opening and each time anything it carries changes, and answers every
/// `provision-act` exactly once. The stream that asked for an act is answered first and then sent the
/// state the act brought about, as every other stream is.
pub async fn serve<I, W>(
	mut incoming: I,
	writer: &mut W,
	carrier: Carrier,
	provisioner: &Provisioner,
	peer: &Peer,
) -> Result<(), SessionError>
where
	I: Stream<Item = io::Result<Vec<u8>>> + Unpin,
	W: AsyncWrite + Unpin,
{
	provisioner.attach();
	let mut changes = provisioner.watch();
	let first = changes.borrow_and_update().to_message();
	send(writer, carrier, &first).await?;

	loop {
		tokio::select! {
			biased;

			changed = changes.changed() => {
				if changed.is_err() {
					return Ok(());
				}
				let state = changes.borrow_and_update().to_message();
				send(writer, carrier, &state).await?;
			}

			raw = incoming.next() => {
				let raw = match raw {
					Some(Ok(raw)) => raw,
					None => return Ok(()),
					Some(Err(err)) => return Err(SessionError::Fault(err.to_string())),
				};
				match read::<Message>(&raw) {
					Ok(Reading::Message(Message::Provision)) => {
						provisioner.attach();
						let state = changes.borrow_and_update().to_message();
						send(writer, carrier, &state).await?;
					}
					Ok(Reading::Message(message @ Message::ProvisionAct { .. })) => {
						let act = Act::of(&message).expect("a provision-act is an act");
						let answer = settle(provisioner, &act, carrier, peer);
						send(writer, carrier, &answer).await?;
					}
					Ok(Reading::Message(Message::Hello { name, version })) => {
						tracing::info!(client = %name, client_version = %version, "client named itself");
						peer.name(name, version);
					}
					Ok(Reading::Message(_)) => {
						// Nothing a provisioning stream does anything about, which MSG makes a no-op.
						tracing::debug!(carrier = carrier.name(), "a message the provisioning stream has nothing to do about");
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

/// Decide an act, logging it with the client that asked, and the message that answers it.
fn settle(provisioner: &Provisioner, act: &Act, carrier: Carrier, peer: &Peer) -> Message {
	let (client, client_version) = peer.named();
	match provisioner.act(act) {
		Ok(()) => {
			tracing::info!(act = %act.act, %client, %client_version, carrier = carrier.name(), "act asked for and accepted");
			Message::ProvisionAccepted
		}
		Err(CannotAct { at, reason }) => {
			tracing::info!(act = %act.act, %client, %client_version, carrier = carrier.name(), %at, %reason, "act asked for and refused");
			Message::CannotAct {
				act: act.act.clone(),
				at,
				reason,
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
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	writer
		.flush()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}
