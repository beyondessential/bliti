//! One session of a device in provisioning mode: the handshake with the provisioning keys, the hello
//! stream, and the provisioning stream (PRV, PRS).

use bliti_core::key_schedule::DeviceKeys;
use futures::{AsyncRead, AsyncWrite};

use super::Provisioner;
use crate::session::SessionError;

/// Run a session to completion over a transport, as a device in provisioning mode.
pub async fn run<S>(
	_transport: S,
	_keys: &DeviceKeys,
	_provisioner: Provisioner,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
	todo!()
}
