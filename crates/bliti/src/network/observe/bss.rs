//! One access point a radio heard, read from the information elements of its beacon or probe
//! response as nl80211 hands them over.
//!
//! Pure. iwd exposes no per-access-point channel, width, signal or security over D-Bus (its
//! `BasicServiceSet` carries the address alone), so the answer to `scan` is read from the kernel's
//! scan results instead, after iwd has scanned.

use serde_json::{Value as Json, json};

use super::channel;

/// An access point, as a scan heard it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
	/// Its radio address.
	pub bssid: [u8; 6],
	/// The network it names in its beacons, `None` where it hides it.
	pub ssid: Option<String>,
	/// Its primary channel's centre frequency, in MHz.
	pub frequency: u32,
	/// The width it occupies, in MHz.
	pub width: u32,
	/// Its secondary 20 MHz channel's centre frequency, in MHz, where it occupies more than 20 MHz
	/// and its HT operation element says which side of the primary that is.
	pub secondary: Option<u32>,
	/// How strongly it was heard, in dBm.
	pub signal: i32,
	/// What it advertises, in CFG's vocabulary, each named once.
	pub security: Vec<&'static str>,
}

/// The element IDs read here (IEEE 802.11-2020, 9.4.2).
const SSID: u8 = 0;
const HT_OPERATION: u8 = 61;
const RSN: u8 = 48;
const VHT_OPERATION: u8 = 192;
const VENDOR: u8 = 221;

/// The OUI and type of the WPA vendor element, which WPA1 access points carry instead of RSN.
const WPA: [u8; 4] = [0x00, 0x50, 0xf2, 0x01];

/// The OUI the AKM suites of RSN are numbered under.
const IEEE: [u8; 3] = [0x00, 0x0f, 0xac];

/// The capability bit saying an access point requires privacy, which with no RSN or WPA element is
/// WEP.
const PRIVACY: u16 = 1 << 4;

impl AccessPoint {
	/// Read an access point from what nl80211 reports of it: `elements` being its information
	/// elements, `capability` its capability field and `signal_mbm` its signal in hundredths of a dBm.
	pub fn read(
		bssid: [u8; 6],
		frequency: u32,
		signal_mbm: i32,
		capability: u16,
		elements: &[u8],
	) -> Self {
		let elements: Vec<(u8, &[u8])> = Elements(elements).collect();
		let find = |id: u8| {
			elements
				.iter()
				.find(|(of, _)| *of == id)
				.map(|(_, body)| *body)
		};

		let ssid = find(SSID).and_then(|raw| {
			// A hidden network sends an empty SSID, or one of the right length that is all zeros.
			(!raw.is_empty() && raw.iter().any(|&b| b != 0))
				.then(|| String::from_utf8_lossy(raw).into_owned())
		});

		let mut security = Vec::new();
		let mut push = |kind: &'static str| {
			if !security.contains(&kind) {
				security.push(kind);
			}
		};
		if let Some(rsn) = find(RSN) {
			for suite in akm_suites(rsn, 2) {
				push(suite);
			}
		} else if let Some(wpa) = elements
			.iter()
			.find(|(id, body)| *id == VENDOR && body.starts_with(&WPA))
		{
			for suite in akm_suites(&wpa.1[WPA.len()..], 2) {
				push(suite);
			}
		} else if capability & PRIVACY != 0 {
			push("wep");
		} else {
			push("open");
		}

		Self {
			bssid,
			ssid,
			frequency,
			width: width(find(HT_OPERATION), find(VHT_OPERATION)),
			secondary: secondary(find(HT_OPERATION), frequency),
			signal: signal_mbm.div_euclid(100),
			security,
		}
	}

	/// The access point as an entry of `access-points` (CFG), heard on `interface`. `None` where its
	/// frequency is on no band CFG names.
	pub fn entry(&self, interface: &str) -> Option<Json> {
		let (band, number) = channel(self.frequency)?;
		let mut entry = json!({
			"interface": interface,
			"bssid": self.address(),
			"ssid": self.ssid,
			"hidden": self.ssid.is_none(),
			"security": self.security,
			"band": band.as_str(),
			"channel": number,
			"channel-width": self.width,
			"signal": self.signal,
		});
		if let Some((_, secondary)) = self.secondary.and_then(channel) {
			entry["secondary-channel"] = secondary.into();
		}
		Some(entry)
	}

	/// Its radio address, lower case and colon-separated.
	pub fn address(&self) -> String {
		self.bssid
			.iter()
			.map(|byte| format!("{byte:02x}"))
			.collect::<Vec<_>>()
			.join(":")
	}
}

/// The elements of a buffer, each an ID and its body, stopping at the first that overruns it.
struct Elements<'a>(&'a [u8]);

impl<'a> Iterator for Elements<'a> {
	type Item = (u8, &'a [u8]);

	fn next(&mut self) -> Option<Self::Item> {
		let [id, len, rest @ ..] = self.0 else {
			return None;
		};
		let body = rest.get(..usize::from(*len))?;
		self.0 = &rest[body.len()..];
		Some((*id, body))
	}
}

/// What the AKM suites of an RSN or WPA element advertise, starting after its `skip`-byte version.
///
/// Both lay out a group cipher, a counted list of pairwise ciphers, then a counted list of AKM
/// suites, four bytes each.
fn akm_suites(body: &[u8], skip: usize) -> Vec<&'static str> {
	let count = |at: usize| -> Option<usize> {
		Some(usize::from(u16::from_le_bytes([
			*body.get(at)?,
			*body.get(at + 1)?,
		])))
	};
	let pairwise = skip + 4;
	let Some(ciphers) = count(pairwise) else {
		return Vec::new();
	};
	let akms = pairwise + 2 + ciphers * 4;
	let Some(suites) = count(akms) else {
		// An element stopping before its AKM list defaults to 802.1X, as the standard has it.
		return vec!["enterprise"];
	};
	(0..suites)
		.filter_map(|index| body.get(akms + 2 + index * 4..akms + 6 + index * 4))
		.filter(|suite| suite[..3] == IEEE || suite[..3] == WPA[..3])
		.filter_map(|suite| match suite[3] {
			1 | 3 | 5 | 11 | 12 | 13 => Some("enterprise"),
			2 | 4 | 6 => Some("psk"),
			8 | 9 | 24 | 25 => Some("sae"),
			18 => Some("owe"),
			_ => None,
		})
		.collect()
}

/// The width an access point occupies, from its HT and VHT operation elements.
fn width(ht: Option<&[u8]>, vht: Option<&[u8]>) -> u32 {
	if let Some(&[chwidth, seg0, seg1, ..]) = vht {
		match chwidth {
			1 if seg1 != 0 && seg0.abs_diff(seg1) == 8 => return 160,
			1 if seg1 != 0 && seg0.abs_diff(seg1) > 16 => return 160,
			1 => return 80,
			2 | 3 => return 160,
			_ => {}
		}
	}
	match ht {
		// The second byte's low two bits are the secondary channel's offset, and bit 2 says a
		// station may use the pair.
		Some(&[_, info, ..]) if info & 0b11 != 0 && info & 0b100 != 0 => 40,
		_ => 20,
	}
}

/// The centre frequency of an access point's secondary 20 MHz channel, from its HT operation
/// element and its primary's `frequency`.
fn secondary(ht: Option<&[u8]>, frequency: u32) -> Option<u32> {
	match ht {
		Some(&[_, info, ..]) if info & 0b100 != 0 => match info & 0b11 {
			1 => Some(frequency + 20),
			3 => frequency.checked_sub(20),
			_ => None,
		},
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn element(id: u8, body: &[u8]) -> Vec<u8> {
		let mut out = vec![id, body.len() as u8];
		out.extend_from_slice(body);
		out
	}

	/// An RSN element with CCMP and the given AKM suite types.
	fn rsn(akms: &[u8]) -> Vec<u8> {
		let mut body = vec![1, 0, 0x00, 0x0f, 0xac, 4, 1, 0, 0x00, 0x0f, 0xac, 4];
		body.extend_from_slice(&(akms.len() as u16).to_le_bytes());
		for akm in akms {
			body.extend_from_slice(&[0x00, 0x0f, 0xac, *akm]);
		}
		element(RSN, &body)
	}

	fn read(elements: &[u8], capability: u16) -> AccessPoint {
		AccessPoint::read([0x02, 0, 0, 0, 0, 0xab], 5180, -6150, capability, elements)
	}

	#[test]
	fn a_transitional_access_point_advertises_both() {
		let mut elements = element(SSID, b"clinic");
		elements.extend(rsn(&[2, 8]));
		let ap = read(&elements, PRIVACY);
		assert_eq!(ap.ssid.as_deref(), Some("clinic"));
		assert_eq!(ap.security, ["psk", "sae"]);
		assert_eq!(ap.signal, -62);
		assert_eq!(ap.address(), "02:00:00:00:00:ab");
	}

	#[test]
	fn a_hidden_network_names_nothing() {
		let mut elements = element(SSID, &[0; 6]);
		elements.extend(rsn(&[1]));
		let ap = read(&elements, PRIVACY);
		assert_eq!(ap.ssid, None);
		assert_eq!(ap.security, ["enterprise"]);
		assert_eq!(read(&element(SSID, &[]), 0).ssid, None);
	}

	#[test]
	fn no_rsn_is_wpa_wep_or_open() {
		let mut wpa = WPA.to_vec();
		wpa.extend_from_slice(&[1, 0, 0x00, 0x50, 0xf2, 2, 1, 0, 0x00, 0x50, 0xf2, 2]);
		wpa.extend_from_slice(&[1, 0, 0x00, 0x50, 0xf2, 2]);
		assert_eq!(read(&element(VENDOR, &wpa), PRIVACY).security, ["psk"]);
		assert_eq!(read(&[], PRIVACY).security, ["wep"]);
		assert_eq!(read(&[], 0).security, ["open"]);
		assert_eq!(read(&rsn(&[18]), PRIVACY).security, ["owe"]);
	}

	#[test]
	fn width_comes_from_ht_and_vht_operation() {
		let ht40 = element(HT_OPERATION, &[36, 0b101, 0, 0, 0, 0]);
		assert_eq!(read(&ht40, 0).width, 40);
		let mut vht80 = ht40.clone();
		vht80.extend(element(VHT_OPERATION, &[1, 42, 0, 0, 0]));
		assert_eq!(read(&vht80, 0).width, 80);
		let mut vht160 = ht40.clone();
		vht160.extend(element(VHT_OPERATION, &[1, 42, 50, 0, 0]));
		assert_eq!(read(&vht160, 0).width, 160);
		assert_eq!(read(&element(HT_OPERATION, &[6, 0, 0]), 0).width, 20);
	}

	#[test]
	fn a_wide_entry_names_its_secondary_channel() {
		let below = element(HT_OPERATION, &[6, 0b111, 0, 0, 0, 0]);
		let ap = AccessPoint::read([2, 0, 0, 0, 0, 1], 2437, -5000, 0, &below);
		assert_eq!(ap.width, 40);
		assert_eq!(ap.entry("wld0").unwrap()["secondary-channel"], 2);
		let above = element(HT_OPERATION, &[36, 0b101, 0, 0, 0, 0]);
		assert_eq!(
			read(&above, 0).entry("wld0").unwrap()["secondary-channel"],
			40
		);
		// No secondary channel, or one a station may not use, is none.
		assert_eq!(
			read(&element(HT_OPERATION, &[36, 0b001, 0]), 0).secondary,
			None
		);
		assert_eq!(read(&element(HT_OPERATION, &[36, 0, 0]), 0).secondary, None);
	}

	#[test]
	fn a_truncated_element_ends_the_list() {
		let mut elements = element(SSID, b"ok");
		elements.extend_from_slice(&[RSN, 40, 1]);
		let ap = read(&elements, 0);
		assert_eq!(ap.ssid.as_deref(), Some("ok"));
		assert_eq!(ap.security, ["open"]);
	}

	#[test]
	fn an_entry_is_the_shape_cfg_gives() {
		let ap = read(&element(SSID, b"clinic"), 0);
		assert_eq!(
			ap.entry("wld0").unwrap(),
			json!({
				"interface": "wld0", "bssid": "02:00:00:00:00:ab", "ssid": "clinic",
				"hidden": false, "security": ["open"], "band": "5ghz", "channel": 36,
				"channel-width": 20, "signal": -62,
			})
		);
	}
}
