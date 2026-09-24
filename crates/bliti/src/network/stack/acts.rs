//! The acts of CFG: scanning, surveying, and joining by WPS.

use bliti_core::channel::config::{Document, Invalid, Segment, path};
use serde_json::{Map, Value as Json, json};
use tokio::sync::{mpsc, oneshot};

use super::{Shared, Stack, driver::Command};
use crate::network::{
	observe::{Iwd, WpsFailed, channel},
	probe::RadioInfo,
	select::Stage,
	session::{Backend, Wps},
};

/// An act that could not be carried out, before anything was applied.
fn refused(at: &[Segment<'_>], reason: impl Into<String>) -> Invalid {
	Invalid {
		at: path(at),
		reason: reason.into(),
		reached: None,
	}
}

/// The radios an act runs on: the one named, else every one `able` says can.
fn radios(
	shared: &Shared,
	interface: Option<&str>,
	able: impl Fn(&RadioInfo) -> bool,
) -> Vec<RadioInfo> {
	shared
		.radios()
		.into_iter()
		.filter(|radio| able(radio) && interface.is_none_or(|name| radio.station == name))
		.collect()
}

/// Every access point each radio scanned heard, leaving out the device's own hotspot (CFG).
pub(super) async fn scan(shared: &Shared, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
	let radios = radios(shared, interface, |radio| radio.scan);
	if radios.is_empty() {
		return Err(refused(
			&[Segment::Name("interface")],
			"no radio here can scan",
		));
	}
	let platform = &shared.platform;
	let own = platform.air.address(&shared.config.access_point);
	let mut entries = Vec::new();
	for radio in radios {
		let station = &radio.station;
		platform
			.iwd
			.scan(station)
			.await
			.map_err(|reason| refused(&[], format!("{station} could not scan: {reason}")))?;
		let heard = platform
			.air
			.access_points(station)
			.await
			.map_err(|reason| {
				refused(&[], format!("{station}'s scan could not be read: {reason}"))
			})?;
		entries.extend(
			heard
				.iter()
				.filter(|ap| own.as_deref() != Some(ap.address().as_str()))
				.filter_map(|ap| ap.entry(station)),
		);
	}
	Ok(entries)
}

/// The spectrum each radio able to survey sees: every channel usable under the regulatory domain in
/// force, with how many access points were heard on it and how busy it was found (CFG).
///
/// A channel the survey carries nothing for is reported as never found busy.
pub(super) async fn survey(
	shared: &Shared,
	interface: Option<&str>,
) -> Result<Option<Map<String, Json>>, Invalid> {
	let radios = radios(shared, interface, |radio| radio.survey);
	if radios.is_empty() {
		return Ok(None);
	}
	let platform = &shared.platform;
	let mut channels = Vec::new();
	for radio in radios {
		let station = &radio.station;
		platform
			.iwd
			.scan(station)
			.await
			.map_err(|reason| refused(&[], format!("{station} could not scan: {reason}")))?;
		let heard = platform
			.air
			.access_points(station)
			.await
			.map_err(|reason| {
				refused(&[], format!("{station}'s scan could not be read: {reason}"))
			})?;
		let surveyed = platform
			.air
			.survey(station)
			.await
			.map_err(|reason| refused(&[], format!("{station} could not survey: {reason}")))?;
		for (band, info) in &radio.bands {
			for usable in &info.channels {
				let networks = heard
					.iter()
					.filter(|ap| channel(ap.frequency) == Some((*band, usable.number)))
					.count();
				let busy = surveyed
					.iter()
					.find(|entry| entry.frequency == usable.frequency && entry.active > 0)
					.map_or(0.0, |entry| {
						(entry.busy as f64 / entry.active as f64).clamp(0.0, 1.0)
					});
				channels.push(json!({
					"interface": station,
					"band": band.as_str(),
					"channel": usable.number,
					"networks": networks,
					"busy": busy,
				}));
			}
		}
	}
	Ok(Some(Map::from_iter([(
		"channels".to_owned(),
		Json::Array(channels),
	)])))
}

/// Join by WPS, then apply `base` with the joined network added first, as a proposal (CFG).
///
/// iwd's WPS takes no network, so a join for a named network is held to it on what the exchange
/// yielded: credentials for any other are forgotten before the radio goes back (WLAN).
pub(super) async fn wps(
	stack: &mut Stack,
	asked: &Wps,
	base: &Map<String, Json>,
	pin: oneshot::Sender<String>,
) -> Result<Map<String, Json>, Invalid> {
	let (method, interface) = (asked.method.as_str(), asked.interface.as_deref());
	let Some(station) = radios(&stack.shared, interface, |_| true)
		.first()
		.map(|radio| radio.station.clone())
	else {
		return Err(refused(
			&[Segment::Name("interface")],
			"no radio here can join by WPS",
		));
	};
	let iwd = stack.shared.platform.iwd.clone();

	let hold = Hold::take(&stack.commands, &station, iwd.clone()).await;
	let joined = match method {
		"push-button" => iwd.push_button(&station).await,
		"pin" => {
			let code = iwd
				.generate_pin(&station)
				.await
				.map_err(|reason| refused(&[], format!("no PIN could be generated: {reason}")))?;
			let _ = pin.send(code.clone());
			iwd.start_pin(&station, &code).await
		}
		other => {
			return Err(refused(
				&[Segment::Name("method")],
				format!("{other:?} is not a WPS method"),
			));
		}
	};
	let joined = joined.map_err(|WpsFailed { found, reason }| {
		let stage = if found {
			Stage::Association
		} else {
			Stage::Carrier
		};
		stage.failed(path(&[]), format!("WPS on {station} failed: {reason}"))
	})?;
	if let Some(ssid) = asked.ssid.as_deref()
		&& joined.ssid != ssid
	{
		// spec: WLAN
		let _ = iwd.disconnect(&station).await;
		let forgotten = iwd.forget(&joined.ssid).await;
		hold.finished();
		let handed = format!(
			"the access point handed over credentials for {:?}, not {ssid:?}",
			joined.ssid
		);
		let reason = match forgotten {
			Ok(()) => format!("{handed}, and they were discarded"),
			Err(error) => {
				tracing::error!(
					ssid = joined.ssid,
					error,
					"could not forget a network WPS joined"
				);
				format!("{handed}, and discarding them failed: {error}")
			}
		};
		// Not the joined network's verification failing, so it reaches no stage (CFG).
		return Err(refused(&[Segment::Name("ssid")], reason));
	}
	hold.finished();

	// Not the joined network's verification failing, so it reaches no stage (CFG).
	let passphrase = iwd.passphrase(&joined.ssid).await.map_err(|reason| {
		refused(
			&[],
			format!(
				"{:?} was joined, but its credentials cannot be carried: {reason}",
				joined.ssid
			),
		)
	})?;
	let raw = joined_document(base, &joined.ssid, &passphrase, interface);
	let document = Document::parse(&raw)?;
	stack.check(&document)?;
	stack.apply(&document).await?;
	Ok(raw)
}

/// `base` with the network WPS joined added first, replacing a candidate for the same SSID that
/// iwd would hold in the same file (WLAN).
pub(super) fn joined_document(
	base: &Map<String, Json>,
	ssid: &str,
	passphrase: &str,
	interface: Option<&str>,
) -> Map<String, Json> {
	let mut document = base.clone();
	let mut attachments = match document.remove("attachments") {
		Some(Json::Array(attachments)) => attachments,
		_ => Vec::new(),
	};
	attachments.retain(|candidate| {
		let personal = matches!(
			candidate["security"]["kind"].as_str(),
			Some("psk" | "sae" | "psk-sae")
		);
		!(candidate["kind"] == "wireless" && candidate["ssid"] == ssid && personal)
	});
	let mut joined = json!({
		"kind": "wireless",
		"label": ssid,
		"verify": true,
		"ssid": ssid,
		"security": { "kind": "psk", "passphrase": passphrase },
	});
	if let Some(interface) = interface {
		joined["interface"] = json!(interface);
	}
	attachments.insert(0, joined);
	document.insert("attachments".to_owned(), Json::Array(attachments));
	document
}

/// A radio held from the driver while WPS runs on it. Dropped before WPS finished, it cancels WPS;
/// dropped at all, it hands the radio back.
struct Hold {
	commands: mpsc::UnboundedSender<Command>,
	station: String,
	iwd: std::sync::Arc<dyn Iwd>,
	running: bool,
}

impl Hold {
	async fn take(
		commands: &mpsc::UnboundedSender<Command>,
		station: &str,
		iwd: std::sync::Arc<dyn Iwd>,
	) -> Self {
		let (reply, answer) = oneshot::channel();
		let _ = commands.send(Command::Hold {
			station: station.to_owned(),
			held: true,
			reply,
		});
		let _ = answer.await;
		Self {
			commands: commands.clone(),
			station: station.to_owned(),
			iwd,
			running: true,
		}
	}

	/// WPS finished: hand the radio back. The driver takes commands in order, so it has the radio
	/// before any proposal sent after.
	fn finished(mut self) {
		self.running = false;
	}
}

impl Drop for Hold {
	fn drop(&mut self) {
		if self.running {
			let (iwd, station) = (self.iwd.clone(), self.station.clone());
			if let Ok(runtime) = tokio::runtime::Handle::try_current() {
				runtime.spawn(async move { iwd.cancel_wps(&station).await });
			}
		}
		let (reply, _) = oneshot::channel();
		let _ = self.commands.send(Command::Hold {
			station: self.station.clone(),
			held: false,
			reply,
		});
	}
}
