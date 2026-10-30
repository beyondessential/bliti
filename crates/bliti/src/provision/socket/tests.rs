use std::{os::unix::fs::PermissionsExt, path::Path, time::Duration};

use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::Message,
	},
	provisioning::report::RunState,
};
use futures::StreamExt;
use tokio::net::UnixStream;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::listen;
use crate::{
	carrier::{Carrier, incoming},
	provision::Provisioner,
};

const PATIENCE: Duration = Duration::from_secs(5);

async fn idle() -> Provisioner {
	Provisioner::start(Vec::new(), None, crate::provision::fake::BUILT).await
}

/// Send `messages` down a connection to `path`, and read the first message that comes back.
async fn exchange(path: &Path, messages: &[Message]) -> Message {
	let (reader, writer) = UnixStream::connect(path).await.unwrap().into_split();
	let mut writer = writer.compat_write();
	for message in messages {
		Carrier::Socket.write(&mut writer, message).await.unwrap();
	}
	let mut incoming = std::pin::pin!(incoming(reader.compat(), Carrier::Socket));
	let raw = tokio::time::timeout(PATIENCE, incoming.next())
		.await
		.expect("the daemon answered nothing")
		.expect("the daemon closed the connection")
		.unwrap();
	match read::<Message>(&raw).unwrap() {
		Reading::Message(message) => message,
		other => panic!("the daemon sent {other:?}"),
	}
}

/// The console is root-only, and gets the state back for `provision`, `hello` first or not (PCON).
#[tokio::test]
async fn a_connection_opens_the_stream_with_provision() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("provision.sock");
	let _listening = listen(&path, idle().await).unwrap();
	assert_eq!(
		std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
		0o600
	);

	let hello = Message::Hello {
		name: "test".to_owned(),
		version: "0".to_owned(),
	};
	for opening in [
		vec![Message::Provision],
		vec![hello.clone(), Message::Provision],
		vec![hello, Message::Curve, Message::Provision],
	] {
		match exchange(&path, &opening).await {
			Message::ProvisionState { run, steps, .. } => {
				assert_eq!(run, RunState::Idle);
				assert!(steps.is_empty());
			}
			other => panic!("the daemon answered `provision` with {other:?}"),
		}
	}
}

/// A socket a daemon left behind is replaced, one a daemon still answers on is not, and a file that
/// is not a socket is not touched.
#[tokio::test]
async fn a_stale_socket_is_replaced_and_a_live_one_or_a_file_kept() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("provision.sock");
	drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
	assert!(path.exists());

	let _listening = listen(&path, idle().await).unwrap();
	let err = listen(&path, idle().await)
		.err()
		.expect("a live socket is refused");
	assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
	assert!(matches!(
		exchange(&path, &[Message::Provision]).await,
		Message::ProvisionState { .. }
	));

	let file = dir.path().join("not-a-socket");
	std::fs::write(&file, "keep").unwrap();
	let err = listen(&file, idle().await)
		.err()
		.expect("a file is refused");
	assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
	assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
}

/// Dropping the handle stops the daemon answering.
#[tokio::test]
async fn dropping_the_handle_closes_the_socket() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("provision.sock");
	let listening = listen(&path, idle().await).unwrap();
	drop(listening);
	tokio::time::timeout(PATIENCE, async {
		while UnixStream::connect(&path).await.is_ok() {
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
	})
	.await
	.expect("the socket kept answering");
}
