//! A client of tailscaled's local API, over its unix socket.
//!
//! Nothing above this module talks HTTP, so everything above it is tested against [`LocalApi`]
//! implemented by a fake. The shapes here are the ones recorded in `fixtures/`; a member this build
//! does not read is ignored, and one it does is optional unless tailscaled always sends it.

use std::{collections::BTreeMap, net::IpAddr, path::PathBuf, time::Duration};

use async_trait::async_trait;
use bliti_core::channel::messages::Secret;
use bytes::Bytes;
use futures::stream::BoxStream;
use serde::Deserialize;
use serde_json::{Map, Value as Json};

#[cfg(unix)]
mod unix;

#[cfg(unix)]
pub use unix::Unix;

/// Where tailscaled listens by default.
pub const DEFAULT_SOCKET: &str = "/var/run/tailscale/tailscaled.sock";

/// How long a call to the local API may take before it is given up on.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(10);

/// How long one probe of a `ping` may wait for an answer, as long as `tailscale ping` waits.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// What went wrong talking to tailscaled. It carries no request body, so a key sent in one cannot
/// reach a log through it.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
	/// The socket could not be reached, or the connection to it broke.
	#[error("cannot reach tailscaled: {0}")]
	Unreachable(String),

	/// tailscaled did not answer in time.
	#[error("tailscaled did not answer in time")]
	Timeout,

	/// tailscaled answered with a failure.
	#[error("tailscaled refused the request ({status}): {message}")]
	Refused {
		/// The HTTP status.
		status: u16,
		/// What it said, which is tailscaled's own and never echoes what was sent.
		message: String,
	},

	/// tailscaled answered with something this build cannot read.
	#[error("tailscaled answered with something unreadable: {0}")]
	Unreadable(String),
}

/// How tailscaled says it stands, from `status`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Status {
	/// `Running`, `Stopped`, `NeedsLogin` and so on.
	#[serde(default)]
	pub backend_state: String,
	/// Whether the daemon holds a node key, which a device that never signed in or has logged out does not.
	#[serde(default)]
	pub have_node_key: bool,
	/// Problems with the connection, in tailscaled's words.
	#[serde(default)]
	pub health: Option<Vec<String>>,
	/// The tailnet the device is a member of.
	#[serde(default)]
	pub current_tailnet: Option<Tailnet>,
	/// The device itself.
	#[serde(rename = "Self", default)]
	pub this: Option<Node>,
	/// The other machines on the tailnet, by key.
	#[serde(default)]
	pub peer: Option<BTreeMap<String, Node>>,
}

/// A tailnet, from `CurrentTailnet`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Tailnet {
	/// Its name.
	#[serde(default)]
	pub name: String,
}

/// A machine on a tailnet, from `Self` or `Peer`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Node {
	/// Its name on the tailnet, fully qualified, with a trailing dot.
	#[serde(rename = "DNSName", default)]
	pub dns_name: String,
	/// When its sign-in lapses, where it does.
	#[serde(default)]
	pub key_expiry: Option<String>,
	/// Whether its sign-in has lapsed.
	#[serde(default)]
	pub expired: bool,
	/// Its addresses on the tailnet.
	#[serde(rename = "TailscaleIPs", default)]
	pub tailscale_ips: Option<Vec<IpAddr>>,
}

/// One message of the notification bus.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Notify {
	/// Something went wrong, such as a rejected key.
	#[serde(default)]
	pub err_message: Option<String>,
	/// The login link of a sign-in by link.
	#[serde(rename = "BrowseToURL", default)]
	pub browse_to_url: Option<String>,
}

/// The preferences tailscaled holds, kept raw so a preference this build has not heard of is sent
/// back as it was.
pub type Prefs = Map<String, Json>;

/// What one probe of a `ping` found, from the local API's `ping`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Pong {
	/// Why the probe failed, or empty.
	#[serde(default)]
	pub err: String,
	/// The round trip, in seconds.
	#[serde(default)]
	pub latency_seconds: f64,
	/// The peer's address where the probe went direct, or empty where it went through a relay.
	#[serde(default)]
	pub endpoint: String,
	/// The relay's region number, where it went through one.
	#[serde(rename = "DERPRegionID", default)]
	pub relay_region: u32,
}

/// The relay map tailscaled holds, from `derpmap`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DerpMap {
	/// Each region, by number.
	#[serde(default)]
	pub regions: BTreeMap<u32, Region>,
}

/// One region of the relay map.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Region {
	/// The region's name, such as `Sydney`.
	#[serde(default)]
	pub region_name: String,
}

/// The calls the device makes of tailscaled.
#[async_trait]
pub trait LocalApi: Send + Sync + 'static {
	/// How tailscaled stands.
	async fn status(&self) -> Result<Status, ApiError>;

	/// The notification bus, which ends when the connection to tailscaled does.
	async fn watch(&self) -> Result<BoxStream<'static, Result<Notify, ApiError>>, ApiError>;

	/// The preferences tailscaled holds.
	async fn prefs(&self) -> Result<Prefs, ApiError>;

	/// Turn the connection to the tailnet on or off, leaving the sign-in as it is.
	async fn set_running(&self, on: bool) -> Result<(), ApiError>;

	/// Start tailscaled's control client with `prefs`, signing in with `auth_key` where there is one.
	async fn start(&self, prefs: Prefs, auth_key: Option<Secret>) -> Result<(), ApiError>;

	/// Begin a sign-in, whose login link arrives on the bus where there is no key to use.
	async fn login_interactive(&self) -> Result<(), ApiError>;

	/// Remove the sign-in.
	async fn logout(&self) -> Result<(), ApiError>;

	/// Send one probe to `address` on the tailnet.
	async fn ping(&self, address: IpAddr) -> Result<Pong, ApiError>;

	/// The relay map, which a device that has never signed in does not hold.
	async fn derp_map(&self) -> Result<Option<DerpMap>, ApiError>;
}

/// Where tailscaled's socket is, for a build that runs on a system with one.
pub fn socket_path() -> PathBuf {
	PathBuf::from(DEFAULT_SOCKET)
}

/// The body of an answer, read as JSON.
pub(crate) fn read_json<T: for<'de> Deserialize<'de>>(body: &Bytes) -> Result<T, ApiError> {
	serde_json::from_slice(body).map_err(|err| ApiError::Unreadable(err.to_string()))
}
