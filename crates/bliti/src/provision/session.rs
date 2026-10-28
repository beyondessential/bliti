//! One session of a device in provisioning mode: the handshake with the provisioning keys, the hello
//! stream, and the provisioning stream (PRV, PRS).
//!
//! Beyond the hello stream and the provisioning stream the device serves no stream role or topic: a
//! stream asking for any other is passed over, as MSG requires of a device meeting what it does not
//! serve (PRV, "The mode").

use std::time::Duration;

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::Message,
		stream::{
			Mode, Streams, accept_responder, is_peer_fault, multiplex, read_message, write_message,
		},
	},
	key_schedule::DeviceKeys,
};
use futures::{AsyncRead, AsyncWrite};
use tokio::task::JoinSet;
use tracing::Instrument;

use super::{Provisioner, stream};
use crate::session::{AbortOnDrop, Peer, SessionError};

/// How long a deliberate teardown waits for the connection to close before dropping it.
const CLOSE_TIMEOUT: Duration = Duration::from_millis(500);

/// What the device calls itself to a client, and the version it is at (MSG).
const DEVICE_NAME: &str = env!("CARGO_PKG_NAME");
const DEVICE_VERSION: &str = env!("BLITI_VERSION");

/// Run a session to completion over a transport, as a device in provisioning mode.
pub async fn run<S>(
	transport: S,
	keys: &DeviceKeys,
	provisioner: Provisioner,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	let encrypted = accept_responder(transport, keys)
		.await
		.map_err(|err| SessionError::Handshake(err.to_string()))?;
	tracing::info!("handshake complete");

	let (mut streams, driver) = multiplex(encrypted, Mode::Server);
	let mut driving = tokio::spawn(
		async move {
			if let Err(err) = driver.await {
				if is_peer_fault(&err) {
					tracing::warn!(%err, "the peer is not speaking the protocol; closing the connection");
				} else {
					tracing::debug!(%err, "connection closed");
				}
			}
		}
		.in_current_span(),
	);
	let _driving = AbortOnDrop(driving.abort_handle());

	let result = converse(&mut streams, &provisioner).await;

	drop(streams);
	if tokio::time::timeout(CLOSE_TIMEOUT, &mut driving)
		.await
		.is_err()
	{
		tracing::debug!("the connection did not close in time; dropping it");
		driving.abort();
	}
	result
}

/// The device's half of the conversation: name itself on the hello stream, and serve the provisioning
/// streams the client opens.
async fn converse(streams: &mut Streams, provisioner: &Provisioner) -> Result<(), SessionError> {
	let peer = Peer::default();
	// Every stream this session serves, so that dropping the session ends them all.
	let mut tasks = JoinSet::new();

	let mut hello = streams
		.open()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	let message = Message::Hello {
		name: DEVICE_NAME.to_owned(),
		version: DEVICE_VERSION.to_owned(),
	};
	write_message(&mut hello, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;

	loop {
		let accepted = tokio::select! {
			accepted = streams.accept() => accepted,
			Some(_) = tasks.join_next(), if !tasks.is_empty() => continue,
		};
		let Some(mut opened) = accepted else {
			tracing::info!("client disconnected");
			return Ok(());
		};
		let (provisioner, peer) = (provisioner.clone(), peer.clone());
		tasks.spawn(
			async move {
				if let Err(err) = serve_stream(&mut opened, &provisioner, &peer).await {
					tracing::debug!(%err, "stream ended");
				}
			}
			.in_current_span(),
		);
	}
}

/// Serve one stream a client opened, until it ends: the provisioning stream where its `provision`
/// opens one, and nothing for any other role or topic. Whatever happens here leaves the other
/// streams and the connection alive.
async fn serve_stream<S>(
	opened: &mut S,
	provisioner: &Provisioner,
	peer: &Peer,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	loop {
		let raw = match read_message(opened).await {
			Ok(Some(raw)) => raw,
			Ok(None) => {
				tracing::debug!("client closed a stream");
				return Ok(());
			}
			Err(err) => return Err(SessionError::Fault(err.to_string())),
		};
		match read::<Message>(&raw) {
			Ok(Reading::Message(Message::Provision)) => {
				return stream::serve_stream(opened, provisioner, peer).await;
			}
			Ok(Reading::Message(Message::Hello { name, version })) => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
				peer.name(name, version);
			}
			Ok(Reading::Message(_)) => {
				// A stream role or topic this device does not serve in provisioning mode. It sends
				// nothing and leaves the stream open until the client closes it (PRV, MSG).
				tracing::debug!("a message this device does not serve in provisioning mode");
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

#[cfg(test)]
mod tests;
