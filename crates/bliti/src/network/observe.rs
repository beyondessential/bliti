//! What the running system says, and what the network backend asks of it.
//!
//! Observers watch the kernel and iwd and send what they see as [`Observation`]s, each as it
//! happens: nothing here polls (LINK). The backend turns them into the selector's events, and into
//! the verification of each attempt.
//!
//! What the backend asks of the system beyond applying files goes through three small traits, so the
//! backend is tested against fakes: [`Iwd`] joins, scans and runs WPS, [`Air`] reads what the radios
//! hear and what they can do, and [`Gateway`] asks whether a link's gateway answers. On Linux,
//! [`linux`] builds the real ones.

use std::{collections::BTreeMap, net::IpAddr};

use futures::future::BoxFuture;

use super::{
	probe::{self, RadioInfo},
	render,
};

#[cfg(target_os = "linux")]
pub use self::linux::linux;

pub mod bss;
#[cfg(target_os = "linux")]
mod gateway;
#[cfg(target_os = "linux")]
mod iwd;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod nl80211;
#[cfg(target_os = "linux")]
mod rtnl;

/// Something the running system said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
	/// A link has carrier, or has lost it. Sent for every link the kernel has, as it first appears
	/// and whenever carrier changes.
	Carrier {
		/// The interface.
		interface: String,
		/// Whether it has carrier.
		up: bool,
	},
	/// An address of global scope was added to a link or removed from it. Link-local addresses are
	/// not sent: they say nothing about a network.
	Address {
		/// The interface.
		interface: String,
		/// The address.
		address: IpAddr,
		/// Whether it has a lifetime, as a leased or autoconfigured address does and a configured one
		/// does not.
		dynamic: bool,
		/// Whether it was added, rather than removed.
		present: bool,
	},
	/// A default route through a gateway was added to a link or removed from it.
	Route {
		/// The interface.
		interface: String,
		/// The gateway.
		gateway: IpAddr,
		/// Whether it was added, rather than removed.
		present: bool,
	},
	/// What a radio hears: each network by SSID, at the strongest signal any of its access points
	/// was heard at, in dBm. Hidden networks are not among them.
	Heard {
		/// The radio's station interface.
		interface: String,
		/// The networks.
		networks: BTreeMap<String, i32>,
		/// Whether a scan that has just finished heard these. Otherwise they are what iwd happened to
		/// hold, as when it starts or a station appears, and a network missing from them has not
		/// gone out of range: iwd restarting forgets every result without the radio hearing less.
		scanned: bool,
	},
	/// A radio's wireless client changed state.
	Station {
		/// The radio's station interface.
		interface: String,
		/// Its state now.
		station: Station,
	},
}

/// Where a wireless client stands, as iwd reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Station {
	/// Joined to no network.
	Disconnected,
	/// Joining, leaving or roaming.
	Busy,
	/// Joined.
	Connected(Joined),
}

/// A network a wireless client is joined to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
	/// The network's SSID.
	pub ssid: String,
	/// The frequency of the access point it is joined through, in MHz, where iwd says.
	pub frequency: Option<u32>,
	/// How it joined, as iwd names it (`WPA2-Personal`, `WPA3-Personal`, ...), where iwd says.
	pub security: Option<String>,
}

impl Joined {
	/// Whether it joined by SAE (WLAN).
	pub fn by_sae(&self) -> bool {
		self.security
			.as_deref()
			.is_some_and(|security| security.starts_with("WPA3-Personal"))
	}
}

/// What iwd calls a network's type, which with its SSID names one network among those a station
/// sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkType {
	/// A pre-shared key, whether WPA2 or WPA3.
	Psk,
	/// 802.1X.
	Enterprise,
}

impl NetworkType {
	/// The type as iwd names it on `net.connman.iwd.Network`.
	pub fn as_iwd(self) -> &'static str {
		match self {
			Self::Psk => "psk",
			Self::Enterprise => "8021x",
		}
	}
}

/// A network for a station to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
	/// Its SSID.
	pub ssid: String,
	/// Its type.
	pub kind: NetworkType,
	/// Whether it hides its SSID, and so has to be looked for by name.
	pub hidden: bool,
}

/// What the backend has iwd do. Every error is the reason, in words an operator can read.
pub trait Iwd: Send + Sync + 'static {
	/// Join `target` on `station`, resolving once joined or once joining failed.
	fn connect(&self, station: &str, target: &Target)
	-> BoxFuture<'static, Result<Joined, String>>;

	/// Leave whatever `station` is joined to, succeeding where it is joined to nothing.
	fn disconnect(&self, station: &str) -> BoxFuture<'static, Result<(), String>>;

	/// Scan on `station`, resolving once the scan has finished with what it heard, as
	/// [`Observation::Heard`] carries it.
	fn scan(&self, station: &str) -> BoxFuture<'static, Result<BTreeMap<String, i32>, String>>;

	/// Join by WPS push-button on `station`, resolving with what was joined.
	fn push_button(&self, station: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>>;

	/// A PIN for WPS, generated by iwd with its check digit.
	fn generate_pin(&self, station: &str) -> BoxFuture<'static, Result<String, String>>;

	/// Join by WPS with `pin` on `station`, resolving with what was joined.
	fn start_pin(&self, station: &str, pin: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>>;

	/// Stop a WPS join in progress on `station`, where there is one.
	fn cancel_wps(&self, station: &str) -> BoxFuture<'static, ()>;

	/// The passphrase iwd holds for the pre-shared-key network `ssid`, as a WPS join left it.
	fn passphrase(&self, ssid: &str) -> BoxFuture<'static, Result<String, String>>;

	/// Have iwd forget the pre-shared-key network `ssid`, as a WPS join left it, so none of its
	/// credentials remain. Succeeds where iwd holds nothing for it.
	fn forget(&self, ssid: &str) -> BoxFuture<'static, Result<(), String>>;
}

/// Why a WPS join failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WpsFailed {
	/// Whether an access point offering WPS was found, so the join stopped at association rather than
	/// at carrier.
	pub found: bool,
	/// What iwd said.
	pub reason: String,
}

/// What the backend reads from the radios, over nl80211.
pub trait Air: Send + Sync + 'static {
	/// What each radio can do, as the probe reads it, taking whether a radio surveys from
	/// `surveyed` where it is there.
	fn radios(
		&self,
		surveyed: BTreeMap<String, bool>,
	) -> BoxFuture<'static, anyhow::Result<Vec<RadioInfo>>>;

	/// Every access point `station`'s radio heard in its latest scan.
	fn access_points(
		&self,
		station: &str,
	) -> BoxFuture<'static, Result<Vec<bss::AccessPoint>, String>>;

	/// How busy `station`'s radio found each channel it has surveyed.
	fn survey(&self, station: &str) -> BoxFuture<'static, Result<Vec<Surveyed>, String>>;

	/// Each scan `station`'s radio finishes from now on. A backend may scan in parts, each replacing
	/// what the kernel holds, so what a whole scan heard is read after every part.
	fn scans(&self, station: &str) -> Result<Scans, String>;

	/// The radio address of `interface`, lower case and colon-separated, where it has one.
	fn address(&self, interface: &str) -> Option<String>;

	/// The channel `interface` operates on now, where it is on one.
	fn operating(&self, interface: &str) -> BoxFuture<'static, Result<Option<Operating>, String>>;

	/// How many clients are joined to the access point on `interface`.
	fn clients(&self, interface: &str) -> BoxFuture<'static, Result<usize, String>>;
}

/// The scans a radio finishes, as [`Air::scans`] watches them. Dropping it stops the watch.
pub struct Scans {
	finished: tokio::sync::mpsc::UnboundedReceiver<()>,
	watch: Vec<tokio::task::AbortHandle>,
}

impl Scans {
	/// Scans finished as `finished` says, stopping `watch` once dropped.
	pub fn new(
		finished: tokio::sync::mpsc::UnboundedReceiver<()>,
		watch: Vec<tokio::task::AbortHandle>,
	) -> Self {
		Self { finished, watch }
	}

	/// Wait for the next scan to finish. Pends for ever once the watch has stopped.
	pub async fn next(&mut self) {
		if self.finished.recv().await.is_none() {
			std::future::pending::<()>().await;
		}
	}
}

impl Drop for Scans {
	fn drop(&mut self) {
		for task in &self.watch {
			task.abort();
		}
	}
}

/// The channel an interface operates on, as nl80211 reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operating {
	/// The primary channel's centre frequency, in MHz.
	pub frequency: u32,
	/// The width, in MHz, where nl80211 names one.
	pub width: Option<u32>,
}

/// How busy one channel was, as a radio's survey says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Surveyed {
	/// The channel's centre frequency, in MHz.
	pub frequency: u32,
	/// How long the radio spent on it, in milliseconds.
	pub active: u64,
	/// How much of that it found the channel busy, in milliseconds.
	pub busy: u64,
}

/// Whether a link's gateway answers.
pub trait Gateway: Send + Sync + 'static {
	/// Ask `gateway` on `interface`, from `source`, an address the link holds in the gateway's
	/// family. Resolves `Ok` once it answers, and with the reason once it is taken not to.
	fn probe(
		&self,
		interface: &str,
		source: IpAddr,
		gateway: IpAddr,
	) -> BoxFuture<'static, Result<(), String>>;
}

/// The band and channel number of a frequency in MHz.
pub fn channel(frequency: u32) -> Option<(probe::Band, u32)> {
	let band = match frequency {
		2400..2500 => probe::Band::TwoPointFour,
		4900..5925 => probe::Band::Five,
		5925..=7125 => probe::Band::Six,
		_ => return None,
	};
	Some((band, probe::number(band, frequency)?))
}

/// The channel a hotspot can be rendered as following, where `frequency` is on a band the renderer
/// carries.
pub fn render_channel(frequency: u32) -> Option<render::Channel> {
	let (band, number) = channel(frequency)?;
	let band = match band {
		probe::Band::TwoPointFour => render::Band::TwoPointFour,
		probe::Band::Five => render::Band::Five,
		probe::Band::Six => return None,
	};
	Some(render::Channel { band, number })
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn frequencies_are_channels_on_bands() {
		assert_eq!(channel(2412), Some((probe::Band::TwoPointFour, 1)));
		assert_eq!(channel(2484), Some((probe::Band::TwoPointFour, 14)));
		assert_eq!(channel(5180), Some((probe::Band::Five, 36)));
		assert_eq!(channel(5955), Some((probe::Band::Six, 1)));
		assert_eq!(channel(900), None);
		assert_eq!(
			render_channel(2437),
			Some(render::Channel {
				band: render::Band::TwoPointFour,
				number: 6
			})
		);
		assert_eq!(render_channel(5955), None, "the renderer carries no 6 GHz");
	}

	#[test]
	fn sae_is_named_wpa3_personal() {
		let joined = |security: &str| Joined {
			ssid: "x".into(),
			frequency: None,
			security: Some(security.into()),
		};
		assert!(joined("WPA3-Personal").by_sae());
		assert!(joined("WPA3-Personal + FT").by_sae());
		assert!(!joined("WPA2-Personal").by_sae());
	}
}
