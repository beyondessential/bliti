//! The daemon's local socket, through which the console reaches the one run clients also drive
//! (PCON).
//!
//! Root-only, since a run changes the board. It speaks the provisioning stream one message a line: a
//! connection opens it with `provision`, optionally naming itself with `hello` first so the daemon's
//! log says what asked.

use std::{
	fs, io,
	os::unix::fs::{FileTypeExt, PermissionsExt},
	path::Path,
	time::Duration,
};

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::Message,
};
use futures::StreamExt;
use tokio::{
	net::{UnixListener, UnixStream},
	task::JoinSet,
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::Instrument;

use super::{Provisioner, stream::serve};
use crate::{
	carrier::{Carrier, incoming},
	session::{AbortOnDrop, Peer, SessionError},
};

/// How long to wait before accepting again after accepting failed, so a failure that persists does
/// not spin.
const ACCEPT_RETRY: Duration = Duration::from_secs(1);

/// Listen on `path` for the console, serving each connection the provisioning stream on
/// `provisioner`, until the handle returned is dropped.
///
/// A socket left at `path` by a daemon that has gone is removed first. One a daemon still answers
/// on is left alone, and so is anything at `path` that is not a socket.
pub fn listen(path: &Path, provisioner: Provisioner) -> io::Result<AbortOnDrop> {
	match fs::symlink_metadata(path) {
		Ok(metadata) if metadata.file_type().is_socket() => {
			if std::os::unix::net::UnixStream::connect(path).is_ok() {
				return Err(io::Error::new(
					io::ErrorKind::AddrInUse,
					format!("a daemon already answers on {}", path.display()),
				));
			}
			fs::remove_file(path)?;
		}
		Ok(_) => {
			return Err(io::Error::new(
				io::ErrorKind::AlreadyExists,
				format!("{} is there and is not a socket", path.display()),
			));
		}
		Err(err) if err.kind() == io::ErrorKind::NotFound => {}
		Err(err) => return Err(err),
	}
	let listener = UnixListener::bind(path)?;
	fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
	tracing::info!(path = %path.display(), "listening for the console");
	Ok(AbortOnDrop(
		tokio::spawn(accept(listener, provisioner).in_current_span()).abort_handle(),
	))
}

async fn accept(listener: UnixListener, provisioner: Provisioner) {
	// Held here, so the connections end with the listener.
	let mut connections = JoinSet::new();
	loop {
		let accepted = tokio::select! {
			accepted = listener.accept() => accepted,
			Some(_) = connections.join_next(), if !connections.is_empty() => continue,
		};
		match accepted {
			Ok((stream, _)) => {
				let provisioner = provisioner.clone();
				connections.spawn(
					async move {
						if let Err(err) = connection(stream, &provisioner).await {
							tracing::debug!(%err, "a console connection ended");
						}
					}
					.instrument(tracing::info_span!("console")),
				);
			}
			Err(err) => {
				tracing::warn!(%err, "could not accept a console connection");
				tokio::time::sleep(ACCEPT_RETRY).await;
			}
		}
	}
}

/// Serve one connection: its `hello`, if any, then the provisioning stream its `provision` opens.
async fn connection(stream: UnixStream, provisioner: &Provisioner) -> Result<(), SessionError> {
	let (reader, writer) = stream.into_split();
	let mut writer = writer.compat_write();
	let mut incoming = std::pin::pin!(incoming(reader.compat(), Carrier::Socket));
	let peer = Peer::default();
	while let Some(raw) = incoming.next().await {
		let raw = raw.map_err(|err| SessionError::Fault(err.to_string()))?;
		match read::<Message>(&raw) {
			Ok(Reading::Message(Message::Hello { name, version })) => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
				peer.name(name, version);
			}
			Ok(Reading::Message(Message::Provision)) => {
				return serve(incoming, &mut writer, Carrier::Socket, provisioner, &peer).await;
			}
			Ok(Reading::Message(_)) => {
				tracing::debug!(
					"a message before the provisioning stream is opened; nothing to do"
				);
			}
			Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
			Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
			Err(fault) => {
				tracing::warn!(%fault, "protocol fault; closing the connection");
				return Err(SessionError::Fault(fault.to_string()));
			}
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests;
