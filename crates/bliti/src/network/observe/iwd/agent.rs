//! The agent iwd asks for a network's secrets (`doc/agent-api.txt` in iwd's tree).
//!
//! iwd reads a passphrase from its known-network file until an attempt with it fails, and from then
//! on asks its agent for one, refusing to connect with `NoAgent` where none is registered. bliti's
//! agent answers from the known-network file it rendered, so the document stays the only source of
//! a secret: a passphrase that was wrong is answered again and fails again, and one the document has
//! since corrected is answered as corrected.

use std::sync::Arc;

use dbus::{
	Message, Path as ObjectPath,
	channel::MatchingReceiver as _,
	message::MatchRule,
	nonblock::{SyncConnection, stdintf::org_freedesktop_dbus::Properties as _},
};

use super::{CALL, NETWORK, SERVICE, failed, render};

const AGENT: &str = "net.connman.iwd.Agent";
const AGENT_MANAGER: &str = "net.connman.iwd.AgentManager";
const CANCELED: &str = "net.connman.iwd.Agent.Error.Canceled";

/// Where bliti's agent sits on the bus.
const PATH: &str = "/bliti/iwd/agent";

/// Answer iwd's calls to the agent, for as long as the bus connection lasts.
pub(super) fn serve(bus: &Arc<SyncConnection>, paths: render::Paths) {
	let rule = MatchRule::new_method_call()
		.with_path(PATH)
		.with_interface(AGENT);
	let owner = bus.clone();
	bus.start_receive(
		rule,
		Box::new(move |call, _| {
			let (bus, paths) = (owner.clone(), paths.clone());
			tokio::spawn(async move {
				if let Some(reply) = answer(&bus, &paths, call).await {
					let _ = dbus::channel::Sender::send(&*bus, reply);
				}
			});
			true
		}),
	);
}

/// Register the agent with iwd. Called whenever iwd comes up, since iwd forgets agents as it stops.
pub(super) async fn register(bus: &Arc<SyncConnection>) {
	let manager = dbus::nonblock::Proxy::new(SERVICE, "/net/connman/iwd", CALL, bus.clone());
	match manager
		.method_call::<(), _, _, _>(AGENT_MANAGER, "RegisterAgent", (ObjectPath::from(PATH),))
		.await
	{
		Ok(()) => tracing::debug!("registered with iwd as its agent"),
		Err(error) => tracing::warn!(
			reason = failed(error),
			"cannot register with iwd as its agent; a network whose passphrase once failed cannot be joined"
		),
	}
}

/// The reply to one call, or `None` for the calls iwd sends without wanting one.
async fn answer(
	bus: &Arc<SyncConnection>,
	paths: &render::Paths,
	call: Message,
) -> Option<Message> {
	let refused = call.error(&CANCELED.into(), c"bliti holds no such secret");
	match &*call.member()? {
		"Release" | "Cancel" => None,
		"RequestPassphrase" => {
			let asker = call.sender().map(|sender| sender.to_string());
			let network = call.read1::<ObjectPath<'static>>();
			let reply = call.method_return();
			Some(match passphrase(bus, paths, asker, network).await {
				Ok(passphrase) => reply.append1(passphrase),
				Err(reason) => {
					tracing::warn!(reason, "cannot answer iwd for a passphrase");
					refused
				}
			})
		}
		// Enterprise networks carry every secret in their own file, so iwd asks for none of these.
		_ => Some(refused),
	}
}

/// The passphrase of `network`, from bliti's own file for it, where `asker` is iwd.
///
/// Only iwd is answered: a passphrase is not given to any other client of the bus.
async fn passphrase(
	bus: &Arc<SyncConnection>,
	paths: &render::Paths,
	asker: Option<String>,
	network: Result<ObjectPath<'static>, dbus::arg::TypeMismatchError>,
) -> Result<String, String> {
	let bus_proxy = dbus::nonblock::Proxy::new(
		"org.freedesktop.DBus",
		"/org/freedesktop/DBus",
		CALL,
		bus.clone(),
	);
	let (iwd,): (String,) = bus_proxy
		.method_call("org.freedesktop.DBus", "GetNameOwner", (SERVICE,))
		.await
		.map_err(failed)?;
	if asker.as_deref() != Some(iwd.as_str()) {
		return Err(format!(
			"{} asked for a passphrase, and is not iwd",
			asker.as_deref().unwrap_or("a client with no name")
		));
	}

	let network = network.map_err(|error| format!("iwd asked about no network: {error}"))?;
	let network = dbus::nonblock::Proxy::new(SERVICE, network, CALL, bus.clone());
	let ssid: String = network.get(NETWORK, "Name").await.map_err(failed)?;
	let kind: String = network.get(NETWORK, "Type").await.map_err(failed)?;
	if kind != "psk" {
		return Err(format!(
			"iwd asked for a passphrase for {ssid:?}, which is {kind}"
		));
	}
	from_file(paths, &ssid)
}

/// What bliti's known-network file for `ssid` gives as its passphrase.
fn from_file(paths: &render::Paths, ssid: &str) -> Result<String, String> {
	let file = render::known_psk(paths, ssid);
	let contents = std::fs::read_to_string(&file).map_err(|error| {
		format!(
			"{ssid:?} is not a network bliti knows ({}: {error})",
			file.display()
		)
	})?;
	render::known_passphrase(&contents)
		.ok_or_else(|| format!("{ssid:?} holds a raw key and no passphrase"))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn paths(root: &std::path::Path) -> render::Paths {
		render::Paths {
			iwd_state: root.to_owned(),
			..render::Paths::system()
		}
	}

	#[test]
	fn a_passphrase_comes_from_blitis_file() {
		let root = std::env::temp_dir().join(format!("bliti-agent-{}", std::process::id()));
		std::fs::create_dir_all(&root).unwrap();
		std::fs::write(
			root.join("Clinic.psk"),
			"[Settings]\nAutoConnect=false\n\n[Security]\nPassphrase=\\sleading space\n",
		)
		.unwrap();
		std::fs::write(root.join("Raw.psk"), "[Security]\nPreSharedKey=00ff\n").unwrap();

		assert_eq!(
			from_file(&paths(&root), "Clinic").as_deref(),
			Ok(" leading space")
		);
		assert!(from_file(&paths(&root), "Raw").is_err());
		assert!(from_file(&paths(&root), "Nowhere").is_err());
		std::fs::remove_dir_all(&root).unwrap();
	}
}
