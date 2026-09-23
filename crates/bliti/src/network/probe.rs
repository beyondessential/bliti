//! Asking each radio what it can do, over nl80211, and stating it as the capabilities of NET.
//!
//! [`RadioInfo`] is what one radio can do. On Linux, [`Nl80211`] fetches it for every radio, with the
//! wiphy's attributes read by a pure parser, and sets what the applier sets over nl80211: the
//! regulatory domain and the access point interface. [`capabilities`] turns the radios, the wired
//! interfaces and what the stack above them offers into the capabilities object of NET, and
//! [`select_hardware`] and [`render_hardware`] into what selection and rendering run on.

#![cfg_attr(
	not(test),
	expect(dead_code, reason = "wired in by the backend that applies selections")
)]

use std::collections::BTreeMap;

use super::{
	render,
	select::{self, Alongside},
};

#[expect(
	unused_imports,
	reason = "wired in by the backend that applies selections"
)]
pub use self::capabilities::{Backend, capabilities};
#[cfg(target_os = "linux")]
#[expect(
	unused_imports,
	reason = "wired in by the backend that applies selections"
)]
pub use self::nl80211::Nl80211;

mod capabilities;
mod model;
#[cfg(target_os = "linux")]
mod nl80211;
#[cfg(test)]
mod tests;
#[cfg(target_os = "linux")]
mod wiphy;

/// What one wireless radio can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioInfo {
	/// The interface its wireless client runs on, which is how a document names the radio.
	pub station: String,
	/// What the adapter is: its driver, and the product where sysfs says.
	pub model: String,
	/// The bands it can use, each with at least one channel the regulatory domain leaves enabled.
	pub bands: BTreeMap<Band, BandInfo>,
	/// How it runs an access point beside a wireless client, or `None` where it cannot run one.
	pub alongside: Option<Alongside>,
	/// Whether it can hold a connection to SAE (WLAN).
	pub sae: bool,
	/// Whether it can scan.
	pub scan: bool,
	/// Whether its driver reports a survey of the channels it can see (CFG).
	pub survey: bool,
}

/// A band, as HOT names bands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
	/// 2.4 GHz.
	TwoPointFour,
	/// 5 GHz.
	Five,
	/// 6 GHz.
	Six,
}

impl Band {
	/// The band as HOT names it.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::TwoPointFour => "2.4ghz",
			Self::Five => "5ghz",
			Self::Six => "6ghz",
		}
	}
}

/// What a radio can do on one band.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BandInfo {
	/// The channels the regulatory domain in force leaves enabled, in frequency order.
	pub channels: Vec<Channel>,
	/// The channel widths the radio supports here and some enabled channel permits, in megahertz,
	/// ascending.
	pub widths: Vec<u32>,
}

/// One 20 MHz channel the regulatory domain leaves enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
	/// Its number within the band.
	pub number: u32,
	/// Its centre frequency, in megahertz.
	pub frequency: u32,
	/// The widest channel the regulatory domain lets it be part of, in megahertz.
	pub max_width: u32,
	/// Whether the radio may not initiate radiation on it, as an access point must.
	pub no_ir: bool,
	/// Whether it needs radar detection before use.
	pub radar: bool,
}

impl Channel {
	/// Whether an access point can start on it without first listening for radar or for another
	/// transmitter.
	pub fn can_start_ap(&self) -> bool {
		!self.no_ir && !self.radar
	}
}

/// How an access point runs beside a wireless client, as HOT has it reported.
pub fn alongside_str(alongside: Alongside) -> &'static str {
	match alongside {
		Alongside::Independent => "independent",
		Alongside::SharedChannel => "shared-channel",
		Alongside::OneAtATime => "one-at-a-time",
	}
}

/// What selection runs on, from the probed radios in the order they were probed.
pub fn select_hardware(radios: &[RadioInfo], wired: &[String]) -> select::Hardware {
	select::Hardware {
		wired: wired.to_vec(),
		radios: radios
			.iter()
			.map(|radio| select::Radio {
				station: radio.station.clone(),
				access_point: radio.alongside,
			})
			.collect(),
	}
}

/// What rendering runs on, from the one radio it describes.
///
/// [`render::Hardware`] describes a single radio, so the caller names which, where the device has
/// one. `access_point` is the interface bliti creates for the hotspot, which is bliti's to name
/// rather than anything the radio reports; it is carried only where the radio can run one.
pub fn render_hardware(
	radio: Option<&RadioInfo>,
	wired: &[String],
	access_point: &str,
	paths: render::Paths,
) -> render::Hardware {
	let alongside = radio.and_then(|radio| radio.alongside);
	render::Hardware {
		wired: wired.to_vec(),
		station: radio.map(|radio| radio.station.clone()),
		access_point: alongside.map(|_| access_point.to_owned()),
		shared_channel: alongside == Some(Alongside::SharedChannel),
		paths,
	}
}
