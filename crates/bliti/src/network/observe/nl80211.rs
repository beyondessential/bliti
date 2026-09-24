//! What the radios heard, found busy and are doing, over nl80211: `NL80211_CMD_GET_SCAN`,
//! `NL80211_CMD_GET_SURVEY`, `NL80211_CMD_GET_INTERFACE` and `NL80211_CMD_GET_STATION`, all dumps
//! that read what the kernel holds and change nothing.

use std::{collections::BTreeMap, fs, io};

use futures::{StreamExt as _, TryStreamExt as _, future::BoxFuture};
use wl_nl80211::{
	Nl80211Attr, Nl80211BssInfo, Nl80211ChannelWidth, Nl80211Event, Nl80211Handle,
	Nl80211MulticastGroup, Nl80211Survey, Nl80211SurveyInfo,
};

use super::{Operating, Scans, Surveyed, bss::AccessPoint};
use crate::network::probe::{Nl80211, RadioInfo};

/// The radios, over nl80211.
pub(super) struct Air {
	probe: Nl80211,
	handle: Nl80211Handle,
}

impl Air {
	/// Open the connections, driven by tasks on the current tokio runtime.
	pub(super) fn connect() -> io::Result<Self> {
		let (connection, handle, _) = wl_nl80211::new_connection()?;
		tokio::spawn(connection);
		Ok(Self {
			probe: Nl80211::connect()?,
			handle,
		})
	}
}

fn index(interface: &str) -> Result<u32, String> {
	fs::read_to_string(format!("/sys/class/net/{interface}/ifindex"))
		.map_err(|error| format!("{interface}: {error}"))?
		.trim()
		.parse()
		.map_err(|error| format!("{interface}: {error}"))
}

/// An access point from one entry of a scan dump.
fn access_point(info: &[Nl80211BssInfo]) -> Option<AccessPoint> {
	let (mut bssid, mut frequency, mut signal, mut capability) = (None, None, 0, 0);
	let (mut elements, mut probe, mut beacon) = (None, None, None);
	for item in info {
		match item {
			Nl80211BssInfo::Bssid(value) => bssid = Some(*value),
			Nl80211BssInfo::Frequency(value) => frequency = Some(*value),
			Nl80211BssInfo::SignalMbm(value) => signal = *value,
			Nl80211BssInfo::Capability(value) => capability = value.bits(),
			Nl80211BssInfo::RawInformationElements(value) => elements = Some(value),
			Nl80211BssInfo::RawProbeResponseInformationElements(value) => probe = Some(value),
			Nl80211BssInfo::RawBeaconInformationElements(value) => beacon = Some(value),
			_ => {}
		}
	}
	let elements = elements.or(probe).or(beacon).map_or(&[][..], Vec::as_slice);
	Some(AccessPoint::read(
		bssid?, frequency?, signal, capability, elements,
	))
}

impl super::Air for Air {
	fn radios(
		&self,
		surveyed: BTreeMap<String, bool>,
	) -> BoxFuture<'static, anyhow::Result<Vec<RadioInfo>>> {
		let probe = self.probe.clone();
		Box::pin(async move { probe.radios(&surveyed).await })
	}

	fn access_points(&self, station: &str) -> BoxFuture<'static, Result<Vec<AccessPoint>, String>> {
		let handle = self.handle.clone();
		let station = station.to_owned();
		Box::pin(async move {
			let index = index(&station)?;
			let messages: Vec<_> = handle
				.scan()
				.dump(index)
				.execute()
				.await
				.try_collect()
				.await
				.map_err(|error| error.to_string())?;
			Ok(messages
				.iter()
				.flat_map(|message| &message.payload.attributes)
				.filter_map(|attribute| match attribute {
					Nl80211Attr::Bss(info) => access_point(info),
					_ => None,
				})
				.collect())
		})
	}

	fn survey(&self, station: &str) -> BoxFuture<'static, Result<Vec<Surveyed>, String>> {
		let handle = self.handle.clone();
		let station = station.to_owned();
		Box::pin(async move {
			let index = index(&station)?;
			let messages: Vec<_> = handle
				.survey()
				.dump(Nl80211Survey::new(index).build())
				.execute()
				.await
				.try_collect()
				.await
				.map_err(|error| error.to_string())?;
			Ok(messages
				.iter()
				.flat_map(|message| &message.payload.attributes)
				.filter_map(|attribute| {
					let Nl80211Attr::SurveyInfo(info) = attribute else {
						return None;
					};
					let (mut frequency, mut active, mut busy) = (None, 0, 0);
					for item in info {
						match item {
							Nl80211SurveyInfo::Frequency(value) => frequency = Some(*value),
							Nl80211SurveyInfo::ActiveTime(value) => active = *value,
							Nl80211SurveyInfo::BusyTime(value) => busy = *value,
							_ => {}
						}
					}
					Some(Surveyed {
						frequency: frequency?,
						active,
						busy,
					})
				})
				.collect())
		})
	}

	fn scans(&self, station: &str) -> Result<Scans, String> {
		let (connection, _handle, mut messages) =
			wl_nl80211::new_multicast_connection(&[Nl80211MulticastGroup::Scan])
				.map_err(|error| format!("cannot watch {station}'s scans: {error}"))?;
		let connection = tokio::spawn(connection);
		let (finished, rx) = tokio::sync::mpsc::unbounded_channel();
		let listen = tokio::spawn(async move {
			while let Some((message, _)) = messages.next().await {
				// The event names no interface that wl-nl80211 hands on. Another radio finishing only
				// has this one read again, which what it heard absorbs.
				if Nl80211Event::parse(message) == Some(Nl80211Event::NewScanResults)
					&& finished.send(()).is_err()
				{
					return;
				}
			}
		});
		Ok(Scans::new(
			rx,
			vec![connection.abort_handle(), listen.abort_handle()],
		))
	}

	fn address(&self, interface: &str) -> Option<String> {
		let text = fs::read_to_string(format!("/sys/class/net/{interface}/address")).ok()?;
		Some(text.trim().to_ascii_lowercase())
	}

	fn operating(&self, interface: &str) -> BoxFuture<'static, Result<Option<Operating>, String>> {
		let handle = self.handle.clone();
		let interface = interface.to_owned();
		Box::pin(async move {
			let messages: Vec<_> = handle
				.interface()
				.get(Vec::new())
				.execute()
				.await
				.try_collect()
				.await
				.map_err(|error| error.to_string())?;
			for message in messages {
				let attributes = &message.payload.attributes;
				let named = attributes.iter().any(
					|attribute| matches!(attribute, Nl80211Attr::IfName(name) if *name == interface),
				);
				if !named {
					continue;
				}
				let (mut frequency, mut width) = (None, None);
				for attribute in attributes {
					match attribute {
						Nl80211Attr::WiphyFreq(value) => frequency = Some(*value),
						Nl80211Attr::ChannelWidth(Nl80211ChannelWidth::NoHt20) => width = Some(20),
						Nl80211Attr::ChannelWidth(Nl80211ChannelWidth::Mhz(value)) => {
							width = Some(*value);
						}
						Nl80211Attr::ChannelWidth(Nl80211ChannelWidth::Mhz80Plus80) => {
							width = Some(160);
						}
						_ => {}
					}
				}
				return Ok(frequency.map(|frequency| Operating { frequency, width }));
			}
			Ok(None)
		})
	}

	fn clients(&self, interface: &str) -> BoxFuture<'static, Result<usize, String>> {
		let handle = self.handle.clone();
		let interface = interface.to_owned();
		Box::pin(async move {
			let index = index(&interface)?;
			let stations: Vec<_> = handle
				.station()
				.dump(index)
				.execute()
				.await
				.try_collect()
				.await
				.map_err(|error| error.to_string())?;
			Ok(stations.len())
		})
	}
}
