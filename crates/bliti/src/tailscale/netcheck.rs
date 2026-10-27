//! The network check: Tailscale's `netcheck`, run as a command because Tailscale runs that probe in
//! its command line and not in the daemon, which is why it works for a device that is not signed in
//! (TSC, "Network check").

use std::{collections::BTreeMap, time::Duration};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use super::relays::Names;

/// How long the command may take before it is given up on.
const TIMEOUT: Duration = Duration::from_secs(60);

/// What `tailscale netcheck --format=json` reports, as far as a result uses it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Report {
	/// Whether UDP got through to a relay.
	#[serde(rename = "UDP", default)]
	udp: bool,
	/// Whether the internet was reached over IPv4.
	#[serde(rename = "IPv4", default)]
	ipv4: bool,
	/// Whether the internet was reached over IPv6.
	#[serde(rename = "IPv6", default)]
	ipv6: bool,
	/// Whether the NAT presents the device at a different address to each destination.
	#[serde(rename = "MappingVariesByDestIP", default)]
	nat_varies: Option<bool>,
	/// Whether the network intercepts web traffic.
	#[serde(default)]
	captive_portal: Option<bool>,
	/// The round trip to each relay region that answered, in nanoseconds.
	#[serde(default)]
	region_latency: Option<BTreeMap<String, u64>>,
}

/// Runs the check.
#[async_trait]
pub trait Runner: Send + Sync + 'static {
	/// Whether the device can run the check at all.
	fn available(&self) -> bool;

	/// Run the check, giving the report as Tailscale printed it, or say why it could not be run.
	async fn run(&self) -> Result<String, String>;
}

/// The `tailscale` command.
#[derive(Debug, Clone)]
pub struct Command {
	program: String,
}

impl Command {
	/// The command named `program`.
	pub fn new(program: impl Into<String>) -> Self {
		Self {
			program: program.into(),
		}
	}

	/// The `tailscale` command on the path.
	pub fn on_path() -> Self {
		Self::new("tailscale")
	}

	fn found(&self) -> bool {
		if self.program.contains('/') {
			return std::path::Path::new(&self.program).is_file();
		}
		std::env::var_os("PATH").is_some_and(|path| {
			std::env::split_paths(&path).any(|directory| directory.join(&self.program).is_file())
		})
	}
}

#[async_trait]
impl Runner for Command {
	fn available(&self) -> bool {
		self.found()
	}

	async fn run(&self) -> Result<String, String> {
		let output = tokio::time::timeout(
			TIMEOUT,
			tokio::process::Command::new(&self.program)
				.args(["netcheck", "--format=json"])
				.kill_on_drop(true)
				.output(),
		)
		.await
		.map_err(|_| "the network check took too long".to_owned())?
		.map_err(|err| format!("cannot run the network check: {err}"))?;
		if !output.status.success() {
			let stderr = String::from_utf8_lossy(&output.stderr);
			let reason = stderr
				.lines()
				.rev()
				.map(str::trim)
				.find(|line| !line.is_empty())
				.unwrap_or("the network check failed");
			return Err(reason.chars().take(200).collect());
		}
		String::from_utf8(output.stdout)
			.map_err(|err| format!("the network check gave something unreadable: {err}"))
	}
}

/// The result of TSC for the report Tailscale printed, naming relays from `names` and carrying the
/// report itself as `report`.
pub fn result(text: &str, names: &Names) -> Result<Map<String, Json>, String> {
	let report: Report = serde_json::from_str(text)
		.map_err(|err| format!("the network check gave something unreadable: {err}"))?;
	let mut relays: Vec<(u32, f64)> = report
		.region_latency
		.iter()
		.flatten()
		.filter_map(|(id, nanos)| Some((id.parse().ok()?, *nanos as f64 / 1e9)))
		.collect();
	relays.sort_by(|a, b| a.1.total_cmp(&b.1));
	let relays: Vec<Json> = relays
		.into_iter()
		.map(|(id, latency)| json!({ "id": id, "name": names.name(id), "latency": latency }))
		.collect();
	let mut result = Map::new();
	result.insert("udp".into(), json!(report.udp && !relays.is_empty()));
	result.insert("ipv4".into(), json!(report.ipv4));
	result.insert("ipv6".into(), json!(report.ipv6));
	result.insert("nat-varies".into(), json!(report.nat_varies));
	result.insert("captive-portal".into(), json!(report.captive_portal));
	result.insert("relays".into(), Json::Array(relays));
	result.insert("report".into(), json!(text));
	Ok(result)
}

#[cfg(test)]
mod tests;
