//! Testing the path to another machine on the tailnet (TSC, "Ping").

use std::net::IpAddr;

use serde_json::{Map, Value as Json, json};

use super::{
	client::{LocalApi, Node, Pong, Status},
	relays::Names,
};

/// How many probes are sent before giving up on finding a direct path.
const PROBES: usize = 6;

/// What a ping found, for a result, or why it could not be run.
pub async fn run(
	api: &dyn LocalApi,
	peer: &str,
	names: &Names,
) -> Result<Map<String, Json>, String> {
	let status = api
		.status()
		.await
		.map_err(|err| format!("cannot read the tailnet: {err}"))?;
	let (machine, address) = resolve(&status, peer)
		.ok_or_else(|| format!("{peer} is not a machine on this device's tailnet"))?;
	let named = machine.dns_name.trim_end_matches('.').to_owned();

	let mut answered = Vec::new();
	for _ in 0..PROBES {
		let pong = api
			.ping(address)
			.await
			.map_err(|err| format!("cannot ping {named}: {err}"))?;
		if pong.err.contains("no matching peer") {
			return Err(format!("{named} is not a machine on this device's tailnet"));
		}
		if pong.err.is_empty() {
			let direct = !pong.endpoint.is_empty();
			answered.push(pong);
			if direct {
				break;
			}
		}
	}
	Ok(report(&named, &answered, names))
}

/// The machine `peer` names, bare or fully qualified, and its first address. The name is the one on
/// the tailnet, never the hostname the machine's system gives it, which two machines can share.
fn resolve<'a>(status: &'a Status, peer: &str) -> Option<(&'a Node, IpAddr)> {
	let peer = peer.trim_end_matches('.');
	status
		.peer
		.iter()
		.flatten()
		.map(|(_, node)| node)
		.find(|node| {
			let qualified = node.dns_name.trim_end_matches('.');
			let bare = qualified.split('.').next().unwrap_or_default();
			[qualified, bare]
				.into_iter()
				.any(|name| !name.is_empty() && name.eq_ignore_ascii_case(peer))
		})
		.and_then(|node| Some((node, *node.tailscale_ips.as_ref()?.first()?)))
}

/// The result of TSC for the probes that were answered.
fn report(peer: &str, answered: &[Pong], names: &Names) -> Map<String, Json> {
	let direct = answered.iter().rfind(|pong| !pong.endpoint.is_empty());
	let relayed = answered.iter().rfind(|pong| pong.endpoint.is_empty());
	let (path, shown) = match (direct, relayed) {
		(Some(pong), _) => (Some("direct"), Some(pong)),
		(None, Some(pong)) => (Some("relayed"), Some(pong)),
		(None, None) => (None, None),
	};
	let relay = shown
		.filter(|_| path == Some("relayed"))
		.filter(|pong| pong.relay_region != 0)
		.map(|pong| json!({ "id": pong.relay_region, "name": names.name(pong.relay_region) }));

	let mut result = Map::new();
	result.insert("peer".into(), json!(peer));
	result.insert("reachable".into(), json!(shown.is_some()));
	result.insert("path".into(), json!(path));
	result.insert("relay".into(), json!(relay));
	result.insert(
		"latency".into(),
		json!(shown.map(|pong| pong.latency_seconds)),
	);
	result
}

#[cfg(test)]
mod tests;
