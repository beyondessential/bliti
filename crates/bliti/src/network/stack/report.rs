//! What the backend knows of the wireless networks joined and the hotspot run, as NFO's
//! `wireless-network`, `hotspot` and `hotspot-clients`.
//!
//! The backend records what each station joined and which hotspot it brought up; the channels and
//! the client count are read from nl80211 as the entries are taken, so they say what the radio is
//! doing then, whatever the backend last asked of it.

use std::{
	collections::BTreeMap,
	sync::{Arc, Mutex, PoisonError},
};

use bliti_core::channel::readings::{Entry, kind};
use futures::future::BoxFuture;
use serde_json::{Map, Value as Json, json};

use crate::network::observe::{Air, Joined, Operating, channel};

/// A handle on what the backend reports, which the sampler reads from.
#[derive(Clone)]
pub struct Report {
	joined: Arc<Mutex<Joins>>,
	air: Arc<dyn Air>,
}

impl std::fmt::Debug for Report {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Report")
			.field("joined", &*self.joins())
			.finish_non_exhaustive()
	}
}

#[derive(Debug, Default)]
struct Joins {
	/// What each station is joined to.
	stations: BTreeMap<String, Joined>,
	/// The hotspot running, where one is.
	hotspot: Option<Hotspot>,
}

/// A hotspot brought up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Hotspot {
	/// The network it advertises.
	pub(super) ssid: String,
	/// The access point interface it runs on.
	pub(super) interface: String,
}

impl Report {
	pub(super) fn new(air: Arc<dyn Air>) -> Self {
		Self {
			joined: Arc::default(),
			air,
		}
	}

	fn joins(&self) -> std::sync::MutexGuard<'_, Joins> {
		self.joined.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Record what `station` is joined to, or that it is joined to nothing.
	pub(super) fn station(&self, station: &str, joined: Option<Joined>) {
		let mut joins = self.joins();
		match joined {
			Some(joined) => joins.stations.insert(station.to_owned(), joined),
			None => joins.stations.remove(station),
		};
	}

	/// Record the hotspot running, or that none is.
	pub(super) fn hotspot(&self, hotspot: Option<Hotspot>) {
		self.joins().hotspot = hotspot;
	}

	/// The entries as of `at`: the client count every time, and where `slow` the wireless networks
	/// and the hotspot as well, each `interface` trait built by `interface`.
	///
	/// Blocks on nl80211, so it is called from the blocking pool the sampler gathers on.
	pub fn entries(&self, at: u64, slow: bool, interface: impl Fn(&str) -> Json) -> Vec<Entry> {
		let (stations, hotspot) = {
			let joins = self.joins();
			(joins.stations.clone(), joins.hotspot.clone())
		};
		let Ok(runtime) = tokio::runtime::Handle::try_current() else {
			return Vec::new();
		};

		let mut entries = Vec::new();
		if slow {
			for (station, joined) in &stations {
				let operating = block(&runtime, self.air.operating(station)).ok().flatten();
				let operating = operating.or(joined.frequency.map(|frequency| Operating {
					frequency,
					width: None,
				}));
				let mut entry = Entry::text(at, "wireless-network", joined.ssid.clone())
					.with_trait("interface", interface(station))
					.with_trait("security", security(joined.security.as_deref()).into());
				if let Some(channel) = operating.and_then(channel_trait) {
					entry = entry.with_trait("channel", channel);
				}
				entries.push(entry);
			}
		}
		let Some(Hotspot { ssid, interface }) = hotspot else {
			return entries;
		};
		if slow {
			let mut entry = Entry::text(at, "hotspot", ssid);
			match block(&runtime, self.air.operating(&interface)) {
				Ok(Some(operating)) => {
					if let Some(channel) = channel_trait(operating) {
						entry = entry.with_trait("channel", channel);
					}
				}
				Ok(None) => entry = entry.warning("the hotspot's interface is on no channel"),
				Err(reason) => {
					entry =
						entry.warning(format!("the hotspot's channel cannot be read: {reason}"));
				}
			}
			entries.push(entry);
		}
		entries.push(match block(&runtime, self.air.clients(&interface)) {
			Ok(clients) => Entry::quantity(at, "hotspot-clients", "clients", clients as f64),
			Err(reason) => Entry::broken(at, "hotspot-clients", kind::QUANTITY, reason),
		});
		entries
	}
}

fn block<T>(runtime: &tokio::runtime::Handle, future: BoxFuture<'static, T>) -> T {
	runtime.block_on(future)
}

/// How a link is secured, in the words a document uses for it, from what iwd calls it.
fn security(iwd: Option<&str>) -> String {
	let Some(iwd) = iwd else {
		return "unknown".to_owned();
	};
	let named = [
		("WPA3-Personal", "sae"),
		("WPA2-Personal", "psk"),
		("WPA1-Personal", "psk"),
		("WPA2-Enterprise", "enterprise"),
		("OWE", "owe"),
		("Open", "open"),
	];
	named
		.iter()
		.find(|(prefix, _)| iwd.starts_with(prefix))
		.map_or_else(|| iwd.to_owned(), |(_, name)| (*name).to_owned())
}

/// The `channel` trait of NFO: `number`, `band`, and `width` where nl80211 names one.
fn channel_trait(operating: Operating) -> Option<Json> {
	let (band, number) = channel(operating.frequency)?;
	let mut trait_ = Map::new();
	trait_.insert("number".into(), json!(number));
	trait_.insert("band".into(), json!(band.as_str()));
	if let Some(width) = operating.width {
		trait_.insert("width".into(), json!(width));
	}
	Some(Json::Object(trait_))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn iwd_names_its_security_as_a_document_does() {
		assert_eq!(security(Some("WPA3-Personal + FT")), "sae");
		assert_eq!(security(Some("WPA2-Personal")), "psk");
		assert_eq!(security(Some("WPA2-Enterprise")), "enterprise");
		assert_eq!(security(Some("FILS")), "FILS");
		assert_eq!(security(None), "unknown");
	}

	#[test]
	fn a_channel_carries_its_band_and_width() {
		assert_eq!(
			channel_trait(Operating {
				frequency: 5180,
				width: Some(80)
			}),
			Some(json!({"number": 36, "band": "5ghz", "width": 80}))
		);
		assert_eq!(
			channel_trait(Operating {
				frequency: 2437,
				width: None
			}),
			Some(json!({"number": 6, "band": "2ghz"}))
		);
	}
}
