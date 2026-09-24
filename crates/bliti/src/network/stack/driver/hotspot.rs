//! Holding the hotspot back on a shared-channel radio until it has a channel it may use (HOT).

use bliti_core::channel::config::AttachmentKind;

use super::Driver;
use crate::network::{probe, render, select::Alongside};

impl Driver {
	/// Whether the radio whose station is `station` runs its access point only on its client's
	/// channel.
	pub(super) fn shares_channel(&self, station: &str) -> bool {
		self.shared
			.render
			.radio(station)
			.and_then(|radio| radio.access_point.as_ref())
			.is_some_and(|access_point| access_point.alongside == Alongside::SharedChannel)
	}

	/// The shared-channel radio the hotspot is placed on, where a pending proposal's scan of it is
	/// still out and a wireless candidate could go on it. Until the scan says what the radio hears, the
	/// hotspot cannot tell whether it will have a client's channel to follow (HOT).
	pub(super) fn hotspot_awaits_scan(&self) -> Option<String> {
		let radio = &self.selector.decision().hotspot.as_ref()?.radio;
		if !self.scanning.contains_key(radio) || !self.shares_channel(radio) {
			return None;
		}
		let candidate = self.document.attachments.iter().any(|attachment| {
			matches!(&attachment.kind, AttachmentKind::Wireless(wireless)
				if wireless.interface.as_ref().is_none_or(|pin| pin == radio))
		});
		candidate.then(|| radio.clone())
	}

	/// Why the hotspot cannot run, where the wireless client of the shared-channel radio running it
	/// is on a channel no access point may start on, so the hotspot has no channel it may use (HOT).
	pub(super) fn hotspot_barred(&self, selection: &render::Selection) -> Option<String> {
		let station = selection.hotspot.as_ref()?;
		if !self.shares_channel(station) {
			return None;
		}
		let channel = selection.channels.get(station)?;
		let radio = self
			.shared
			.radios()
			.into_iter()
			.find(|radio| radio.station == *station)?;
		let (band, name) = match channel.band {
			render::Band::TwoPointFour => (probe::Band::TwoPointFour, "2.4 GHz"),
			render::Band::Five => (probe::Band::Five, "5 GHz"),
		};
		let flags = radio
			.bands
			.get(&band)
			.and_then(|info| info.channels.iter().find(|c| c.number == channel.number));
		let why = match flags {
			Some(flags) if flags.can_start_ap() => return None,
			Some(flags) if flags.radar => {
				"needs radar detection before an access point may start on it"
			}
			_ => "is one the regulatory domain lets no access point start on",
		};
		Some(format!(
			"the hotspot has to share {station}'s channel, {name} channel {}, which {why}",
			channel.number
		))
	}
}
