//! Signing in by a tailscaled of the step's own, whose state is kept on the disk's system (PTS).

use std::{
	path::{Path, PathBuf},
	process::Stdio,
	time::Duration,
};

use async_trait::async_trait;
use bliti_core::channel::messages::Secret;
use tokio::{
	process::{Child, Command},
	time::{sleep, timeout},
};

use crate::tailscale::{Options, Tailscale};

/// What the step needs of Tailscale, which tests stand in for.
#[async_trait]
pub trait Node: Send + Sync {
	/// Sign in with `key`, registering as `name`, keeping Tailscale's state in `state`, which stays
	/// there once this returns, with no Tailscale left running. Whether the node then waits on an
	/// administrator's approval.
	async fn sign_in(
		&self,
		state: &Path,
		name: &str,
		key: Secret,
		tailnet: Option<String>,
	) -> Result<bool, String>;
}

/// Where the private tailscaled listens.
const SOCKET: &str = "/run/bliti-provision/tailscaled.sock";

/// How long tailscaled has to start listening.
const STARTUP: Duration = Duration::from_secs(30);

/// How long tailscaled has to stop once asked.
const SHUTDOWN: Duration = Duration::from_secs(15);

/// A tailscaled run for the sign-in alone, with its state on the disk's system and no tunnel, so
/// the provisioning system's own network is left as it is.
pub struct Tailscaled;

#[async_trait]
impl Node for Tailscaled {
	// spec: PTS
	async fn sign_in(
		&self,
		state: &Path,
		name: &str,
		key: Secret,
		tailnet: Option<String>,
	) -> Result<bool, String> {
		let socket = PathBuf::from(SOCKET);
		if let Some(parent) = socket.parent() {
			std::fs::create_dir_all(parent)
				.map_err(|err| format!("cannot make a place for Tailscale's socket: {err}"))?;
		}
		let _ = std::fs::remove_file(&socket);
		let mut daemon = Command::new("tailscaled")
			.args(["--tun=userspace-networking", "--port=0"])
			.arg(format!("--statedir={}", state.display()))
			.arg(format!("--socket={}", socket.display()))
			.stdin(Stdio::null())
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.kill_on_drop(true)
			.spawn()
			.map_err(|err| format!("cannot run tailscaled: {err}"))?;

		let signed_in = async {
			wait_for(&socket).await?;
			let options = Options {
				tailnet,
				..Options::default()
			};
			let tailscale = Tailscale::on_socket(socket.clone(), options).await;
			tailscale.sign_in_by_key(key, name).await
		}
		.await;

		stop(&mut daemon).await;
		let _ = std::fs::remove_file(&socket);
		signed_in
	}
}

async fn wait_for(socket: &Path) -> Result<(), String> {
	timeout(STARTUP, async {
		while !socket.exists() {
			sleep(Duration::from_millis(200)).await;
		}
	})
	.await
	.map_err(|_| "tailscaled did not start in time".to_owned())
}

/// Ask `daemon` to stop, so it writes its state out, and make it if it will not.
async fn stop(daemon: &mut Child) {
	if let Some(pid) = daemon.id() {
		let _ = Command::new("kill")
			.args(["-TERM", &pid.to_string()])
			.status()
			.await;
	}
	if timeout(SHUTDOWN, daemon.wait()).await.is_err() {
		tracing::warn!("tailscaled did not stop when asked; killing it");
		let _ = daemon.kill().await;
	}
}
