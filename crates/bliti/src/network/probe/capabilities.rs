//! The capabilities of NET, built from the probed radios, the wired interfaces, and what the stack
//! above them offers.
//!
//! Pure. Everything that differs by radio is keyed under `interface`, a hotspot's channels under
//! `band`, and every kind under `kind`, as [`bliti_core::channel::capabilities::check`] reads them.

use std::collections::BTreeMap;

use serde_json::{Map, Value as Json, json};

use super::{Band, RadioInfo, alongside_str};
use crate::network::{render, select::Alongside};

/// What the stack above the radios offers, whatever radio it runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
	/// The WPS methods the wireless client offers on every radio it runs on.
	pub wps: Vec<&'static str>,
	/// The enterprise EAP methods the wireless client authenticates by.
	pub eap: Vec<&'static str>,
	/// The bands the hotspot can be put on, each with the widths it can be run at.
	pub hotspot: BTreeMap<Band, Vec<u32>>,
}

impl Backend {
	/// iwd as the wireless client, and hostapd as rendered for the hotspot: 20 and 40 MHz on
	/// 2.4 GHz, up to 80 MHz on 5 GHz, and nothing on 6 GHz.
	pub fn stack() -> Self {
		Self {
			wps: vec!["push-button", "pin"],
			eap: vec!["peap", "ttls", "tls", "pwd"],
			hotspot: BTreeMap::from([
				(Band::TwoPointFour, vec![20, 40]),
				(Band::Five, vec![20, 40, 80]),
			]),
		}
	}
}

/// The capabilities object of NET, carrying `document`, `radios` and `acts`.
pub fn capabilities(
	radios: &[RadioInfo],
	wired: &[String],
	backend: &Backend,
) -> Map<String, Json> {
	let mut out = Map::new();
	out.insert("document".into(), document(radios, wired, backend).into());
	if !radios.is_empty() {
		let described = radios
			.iter()
			.map(|radio| (radio.station.clone(), describe(radio).into()))
			.collect::<Map<_, _>>();
		out.insert("radios".into(), described.into());
	}
	out.insert("acts".into(), acts(radios, backend).into());
	out
}

fn document(radios: &[RadioInfo], wired: &[String], backend: &Backend) -> Map<String, Json> {
	let mut kinds = Map::new();
	if !radios.is_empty() {
		let interfaces = radios
			.iter()
			.map(|radio| {
				let under = json!({
					"security": { "kind": security(radio, backend) },
					"hidden": true,
				});
				(radio.station.clone(), under)
			})
			.collect::<Map<_, _>>();
		kinds.insert(
			"wireless".into(),
			json!({ "interface": interfaces, "nameservers": true }),
		);
	}
	if !wired.is_empty() {
		for kind in ["wired-dynamic", "wired-static"] {
			kinds.insert(
				kind.into(),
				json!({ "interface": wired, "nameservers": true }),
			);
		}
	}

	let mut document = Map::new();
	document.insert("attachments".into(), json!({ "kind": kinds }));
	let hotspots = radios
		.iter()
		.filter_map(|radio| Some((radio.station.clone(), hotspot(radio, backend)?.into())))
		.collect::<Map<_, _>>();
	if !hotspots.is_empty() {
		document.insert(
			"hotspot".into(),
			json!({
				"interface": hotspots,
				"share-upstream": true,
				"isolate-clients": true,
				"dhcp-range": true,
			}),
		);
	}
	if !radios.is_empty() {
		document.insert("regulatory-domain".into(), true.into());
	}
	document
}

/// The security kinds a candidate on `radio` may carry: `sae` and `psk-sae` only where the radio
/// can hold a connection to SAE (WLAN).
fn security(radio: &RadioInfo, backend: &Backend) -> Map<String, Json> {
	let mut kinds = Map::new();
	kinds.insert("psk".into(), json!({}));
	if radio.sae {
		kinds.insert("sae".into(), json!({}));
		kinds.insert("psk-sae".into(), json!({}));
	}
	if !backend.eap.is_empty() {
		kinds.insert("enterprise".into(), json!({ "eap": backend.eap }));
	}
	kinds
}

/// What a hotspot on `radio` may carry, or `None` where the radio cannot run one.
///
/// Each band carries the channels an access point can start on and the renderer renders, and the
/// widths the radio, the regulatory domain and the stack all allow. A shared-channel radio offers
/// them too, for a hotspot with no client to follow (HOT), and runs one on its client's channel
/// where it offers none.
fn hotspot(radio: &RadioInfo, backend: &Backend) -> Option<Map<String, Json>> {
	let alongside = radio.alongside?;
	let mut bands = Map::new();
	for (band, info) in &radio.bands {
		let Some(carried) = backend.hotspot.get(band) else {
			continue;
		};
		let channels: Vec<_> = info
			.channels
			.iter()
			.filter(|channel| channel.can_start_ap())
			.filter(|channel| render::hotspot_channel(band.as_str(), channel.number))
			.collect();
		let widths: Vec<u32> = info
			.widths
			.iter()
			.copied()
			.filter(|width| carried.contains(width))
			.filter(|width| channels.iter().any(|channel| channel.max_width >= *width))
			.collect();
		if channels.is_empty() {
			continue;
		}
		let numbers: Vec<u32> = channels.iter().map(|channel| channel.number).collect();
		bands.insert(
			band.as_str().into(),
			json!({ "channel": numbers, "channel-width": widths }),
		);
	}
	match (bands.is_empty(), alongside) {
		(false, _) => Some(Map::from_iter([("band".to_owned(), bands.into())])),
		(true, Alongside::SharedChannel) => Some(Map::new()),
		(true, Alongside::Independent | Alongside::OneAtATime) => None,
	}
}

fn describe(radio: &RadioInfo) -> Map<String, Json> {
	let mut out = Map::new();
	out.insert("model".into(), radio.model.clone().into());
	let bands: Vec<&str> = radio.bands.keys().map(|band| band.as_str()).collect();
	out.insert("bands".into(), bands.into());
	if let Some(alongside) = radio.alongside {
		out.insert("alongside".into(), alongside_str(alongside).into());
	}
	out
}

fn acts(radios: &[RadioInfo], backend: &Backend) -> Map<String, Json> {
	let keyed = |under: &dyn Fn(&RadioInfo) -> Option<Json>| {
		let interfaces = radios
			.iter()
			.filter_map(|radio| Some((radio.station.clone(), under(radio)?)))
			.collect::<Map<_, _>>();
		(!interfaces.is_empty()).then(|| json!({ "interface": interfaces }))
	};
	let mut acts = Map::new();
	let scan = keyed(&|radio| radio.scan.then(|| json!({})));
	let survey = keyed(&|radio| radio.survey.then(|| json!({})));
	// Any network may be named: the device holds the join to it on what the exchange yields (WLAN).
	let wps = keyed(&|_| {
		(!backend.wps.is_empty()).then(|| json!({ "method": backend.wps, "ssid": true }))
	});
	for (act, value) in [("scan", scan), ("survey", survey), ("wps", wps)] {
		if let Some(value) = value {
			acts.insert(act.into(), value);
		}
	}
	acts
}

#[cfg(test)]
mod tests;
