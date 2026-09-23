//! hostapd: the hotspot's access point.

use std::fmt::Write as _;

use bliti_core::channel::config::{Hotspot, Invalid, Segment};

use super::{Band, Channel, File, Hardware, SECRET, header, invalid};

/// Where hostapd listens for its control client.
const CONTROL: &str = "/run/hostapd";

/// The channel a hotspot takes where nothing chooses one: the middle of the three non-overlapping
/// 2.4 GHz channels, which every regulatory domain permits.
const DEFAULT_CHANNEL: Channel = Channel {
	band: Band::TwoPointFour,
	number: 6,
};

fn at(member: &str) -> [Segment<'_>; 2] {
	[Segment::Name("hotspot"), Segment::Name(member)]
}

/// Read `band`, in the vocabulary proposed with the wire shape and shared with NFO's `channel`
/// trait. Kept to this one function because HOT does not pin its values.
pub(super) fn band(band: &str) -> Result<Band, String> {
	match band {
		"2.4ghz" => Ok(Band::TwoPointFour),
		"5ghz" => Ok(Band::Five),
		other => Err(format!(
			"{other:?} is not a band; bands are \"2.4ghz\" and \"5ghz\""
		)),
	}
}

/// Whether `number` is a 20 MHz channel on `band`.
pub(super) fn exists(band: Band, number: u32) -> bool {
	match band {
		Band::TwoPointFour => (1..=14).contains(&number),
		Band::Five => {
			number % 4 == 0 && ((36..=64).contains(&number) || (100..=144).contains(&number))
				|| number % 4 == 1 && (149..=177).contains(&number)
		}
	}
}

/// How the channel is widened, as the `ht_capab` and VHT keys that say so.
fn widen(out: &mut String, channel: Channel, width: u32) -> Result<(), String> {
	match (channel.band, width) {
		(_, 20) => Ok(()),
		(Band::TwoPointFour, 40) => {
			let secondary = if channel.number <= 7 { "+" } else { "-" };
			let _ = writeln!(out, "ht_capab=[HT40{secondary}]");
			Ok(())
		}
		(Band::Five, 40 | 80) => {
			// 5 GHz channels pair from 36 and from 149, and group in fours for 80 MHz.
			let base = if channel.number >= 149 { 149 } else { 36 };
			let offset = channel.number - base;
			let secondary = if (offset / 4) % 2 == 0 { "+" } else { "-" };
			let _ = writeln!(out, "ht_capab=[HT40{secondary}]");
			if width == 80 {
				let centre = base + offset / 16 * 16 + 6;
				let _ = write!(
					out,
					"vht_oper_chwidth=1\nvht_oper_centr_freq_seg0_idx={centre}\n"
				);
			}
			Ok(())
		}
		(Band::TwoPointFour, _) => Err(format!("{width} MHz is not a 2.4 GHz channel width")),
		(Band::Five, _) => Err(format!("{width} MHz is not a 5 GHz channel width")),
	}
}

/// The channel and width the hotspot operates on.
///
/// A shared-channel radio's hotspot runs on the station's channel whenever it is associated, and
/// accepts no choice of its own (HOT). hostapd cannot follow a channel by itself, so the applier
/// renders again, and restarts hostapd, whenever the station's channel changes.
fn operating(
	hotspot: &Hotspot,
	shared: bool,
	station: Option<Channel>,
) -> Result<(Channel, u32), Invalid> {
	if shared {
		let chosen = [
			("band", hotspot.band.is_some()),
			("channel", hotspot.channel.is_some()),
			("channel-width", hotspot.channel_width.is_some()),
		];
		if let Some((member, _)) = chosen.into_iter().find(|(_, set)| *set) {
			return Err(invalid(
				&at(member),
				"this radio runs its hotspot on the channel its wireless client is using",
			));
		}
		return Ok((station.unwrap_or(DEFAULT_CHANNEL), 20));
	}

	let chosen_band = hotspot
		.band
		.as_deref()
		.map(band)
		.transpose()
		.map_err(|reason| invalid(&at("band"), reason))?;
	let channel = match (chosen_band, hotspot.channel) {
		(None, None) => DEFAULT_CHANNEL,
		(Some(Band::TwoPointFour), None) => DEFAULT_CHANNEL,
		(Some(Band::Five), None) => Channel {
			band: Band::Five,
			number: 36,
		},
		(Some(band), Some(number)) => Channel { band, number },
		(None, Some(number)) => Channel {
			band: if number <= 14 {
				Band::TwoPointFour
			} else {
				Band::Five
			},
			number,
		},
	};
	if !exists(channel.band, channel.number) {
		return Err(invalid(
			&at("channel"),
			format!("{} is not a channel on this band", channel.number),
		));
	}
	Ok((channel, hotspot.channel_width.unwrap_or(20)))
}

/// hostapd's configuration for the hotspot: WPA2/WPA3 transitional, no WPS (WLAN), clients isolated
/// unless the document says otherwise (HOT).
pub(super) fn conf(
	hotspot: &Hotspot,
	interface: &str,
	hardware: &Hardware,
	station: Option<Channel>,
	domain: Option<&str>,
) -> Result<File, Invalid> {
	if hotspot.ssid.is_empty() || hotspot.ssid.len() > 32 {
		return Err(invalid(&at("ssid"), "an SSID is 1 to 32 bytes"));
	}
	let passphrase = &hotspot.passphrase;
	if !(8..=63).contains(&passphrase.len())
		|| !passphrase.bytes().all(|b| (b' '..=b'~').contains(&b))
	{
		return Err(invalid(
			&at("passphrase"),
			"a passphrase is 8 to 63 printable ASCII characters",
		));
	}
	let (channel, width) = operating(hotspot, hardware.shared_channel, station)?;

	let mut out = header("the hotspot");
	let _ = write!(
		out,
		"interface={interface}\ndriver=nl80211\nctrl_interface={CONTROL}\n\
		 ssid2={}\nutf8_ssid=1\n",
		hex::encode(&hotspot.ssid)
	);
	if let Some(domain) = domain {
		let _ = write!(out, "country_code={domain}\nieee80211d=1\n");
	}
	let mode = match channel.band {
		Band::TwoPointFour => "g",
		Band::Five => "a",
	};
	let _ = write!(
		out,
		"hw_mode={mode}\nchannel={}\nwmm_enabled=1\nieee80211n=1\n",
		channel.number
	);
	if channel.band == Band::Five {
		out.push_str("ieee80211ac=1\n");
	}
	widen(&mut out, channel, width).map_err(|reason| invalid(&at("channel-width"), reason))?;
	let _ = write!(
		out,
		"wpa=2\nwpa_key_mgmt=WPA-PSK SAE\nrsn_pairwise=CCMP\nieee80211w=1\nsae_require_mfp=1\n\
		 wpa_passphrase={passphrase}\nap_isolate={}\nwps_state=0\n",
		u8::from(hotspot.isolate_clients.unwrap_or(true))
	);

	Ok(File {
		path: hardware.paths.hostapd.clone(),
		contents: out,
		mode: SECRET,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	fn widened(band: Band, number: u32, width: u32) -> String {
		let mut out = String::new();
		widen(&mut out, Channel { band, number }, width).unwrap();
		out
	}

	/// The secondary channel sits on the side the pairing allows, and 80 MHz names its centre.
	#[test]
	fn widths_pick_their_secondary_and_centre() {
		assert_eq!(widened(Band::TwoPointFour, 1, 40), "ht_capab=[HT40+]\n");
		assert_eq!(widened(Band::TwoPointFour, 11, 40), "ht_capab=[HT40-]\n");
		assert_eq!(widened(Band::Five, 44, 40), "ht_capab=[HT40+]\n");
		assert_eq!(widened(Band::Five, 153, 40), "ht_capab=[HT40-]\n");
		assert_eq!(
			widened(Band::Five, 112, 80),
			"ht_capab=[HT40-]\nvht_oper_chwidth=1\nvht_oper_centr_freq_seg0_idx=106\n"
		);
		assert_eq!(
			widened(Band::Five, 161, 80),
			"ht_capab=[HT40-]\nvht_oper_chwidth=1\nvht_oper_centr_freq_seg0_idx=155\n"
		);
		assert!(widen(&mut String::new(), DEFAULT_CHANNEL, 80).is_err());
	}

	/// Only real 20 MHz channels are accepted.
	#[test]
	fn channels_exist_on_their_band() {
		assert!(exists(Band::TwoPointFour, 13));
		assert!(!exists(Band::TwoPointFour, 36));
		assert!(exists(Band::Five, 36));
		assert!(exists(Band::Five, 165));
		assert!(!exists(Band::Five, 38));
		assert!(!exists(Band::Five, 68));
	}

	/// A band is `2.4` or `5`.
	#[test]
	fn bands_parse() {
		assert_eq!(band("2.4ghz"), Ok(Band::TwoPointFour));
		assert_eq!(band("5ghz"), Ok(Band::Five));
		assert!(band("6ghz").is_err());
	}
}
