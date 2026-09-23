//! What a radio can do, read from the attributes nl80211 describes its wiphy with.
//!
//! Pure: [`parse`] takes the attributes of one wiphy, every message of a split dump concatenated,
//! and the facts that do not come from the wiphy (its station interface, its model, whether its
//! driver answered a survey).
//!
//! HE capabilities are not read. wl-nl80211 0.7.0 parses `NL80211_BAND_ATTR_IFTYPE_DATA` one level
//! too shallow, taking each nested iftype-data entry for one of its own attributes, so the HE PHY
//! capabilities it yields are bytes from the wrong place. Widths come from HT and VHT alone, which
//! leaves 6 GHz, where neither applies, at 20 MHz.

use std::collections::BTreeMap;

use wl_nl80211::{
	Ieee80211AkmSuite, Ieee80211CipherSuite, Ieee80211HtCaps, Ieee80211VhtCapInfo, Nl80211Attr,
	Nl80211BandInfo, Nl80211BandType, Nl80211ExtFeature, Nl80211Features, Nl80211FrequencyInfo,
	Nl80211IfMode, Nl80211IfaceComb, Nl80211IfaceCombAttribute, Nl80211IfaceCombLimitAttribute,
	Nl80211InterfaceType,
};

use super::{Band, BandInfo, Channel, RadioInfo, number};
use crate::network::select::Alongside;

/// What the radio whose wiphy carries `attributes` can do.
pub(super) fn parse(
	attributes: &[Nl80211Attr],
	station: String,
	model: String,
	survey: bool,
) -> RadioInfo {
	let mut bands: BTreeMap<Band, Vec<Nl80211BandInfo>> = BTreeMap::new();
	let mut access_point = false;
	let mut combinations = Vec::new();
	let mut ciphers = Vec::new();
	let mut akms: Option<Vec<Ieee80211AkmSuite>> = None;
	let mut features = Nl80211Features::empty();
	let mut extended = Vec::new();
	let mut scan = false;

	for attribute in attributes {
		match attribute {
			// A split dump sends a band over several messages, each with some of its channels.
			Nl80211Attr::WiphyBands(list) => {
				for band in list {
					if let Some(kind) = band_of(band.kind) {
						bands
							.entry(kind)
							.or_default()
							.extend(band.info.iter().cloned());
					}
				}
			}
			Nl80211Attr::SupportedIftypes(modes) => {
				access_point |= modes.contains(&Nl80211IfMode::Ap);
			}
			Nl80211Attr::InterfaceCombination(list) => combinations.extend(list.iter().cloned()),
			Nl80211Attr::CipherSuites(list) => ciphers.extend(list.iter().copied()),
			Nl80211Attr::AkmSuites(list) => {
				akms.get_or_insert_default().extend(list.iter().copied())
			}
			Nl80211Attr::Features(flags) => features |= *flags,
			Nl80211Attr::ExtFeatures(list) => extended.extend(list.iter().copied()),
			Nl80211Attr::MaxNumScanSsids(count) => scan = *count > 0,
			_ => {}
		}
	}

	let bands = bands
		.into_iter()
		.map(|(band, info)| (band, band_info(band, &info)))
		.filter(|(_, info)| !info.channels.is_empty())
		.collect();

	RadioInfo {
		station,
		model,
		bands,
		alongside: access_point.then(|| alongside(&combinations)),
		sae: sae(&ciphers, akms.as_deref(), features, &extended),
		scan,
		survey,
	}
}

fn band_of(kind: Nl80211BandType) -> Option<Band> {
	match kind {
		Nl80211BandType::Band2GHz => Some(Band::TwoPointFour),
		Nl80211BandType::Band5GHz => Some(Band::Five),
		Nl80211BandType::Band6GHz => Some(Band::Six),
		_ => None,
	}
}

/// The enabled channels of one band, and the widths the radio's HT and VHT capabilities give it
/// there that some enabled channel permits.
fn band_info(band: Band, info: &[Nl80211BandInfo]) -> BandInfo {
	let mut channels = Vec::new();
	let mut ht40 = false;
	let mut vht: Option<Ieee80211VhtCapInfo> = None;
	for item in info {
		match item {
			Nl80211BandInfo::Freqs(frequencies) => {
				channels.extend(
					frequencies
						.iter()
						.filter_map(|frequency| channel(band, &frequency.info)),
				);
			}
			Nl80211BandInfo::HtCapa(caps) => ht40 |= caps.contains(Ieee80211HtCaps::SupWidth2040),
			Nl80211BandInfo::VhtCap(caps) => vht = Some(*caps),
			_ => {}
		}
	}
	channels.sort_by_key(|channel| channel.frequency);
	channels.dedup_by_key(|channel| channel.frequency);

	let mut widths = vec![20];
	if ht40 {
		widths.push(40);
	}
	// VHT is a 5 GHz amendment; a driver reporting it on 2.4 GHz is describing a vendor extension.
	if band == Band::Five
		&& let Some(vht) = vht
	{
		widths.push(80);
		if vht.intersects(Ieee80211VhtCapInfo::SuppChanWidthMask) {
			widths.push(160);
		}
	}
	widths.retain(|&width| channels.iter().any(|channel| channel.max_width >= width));
	BandInfo { channels, widths }
}

/// The channel a frequency's attributes describe, where the regulatory domain leaves it enabled.
fn channel(band: Band, info: &[Nl80211FrequencyInfo]) -> Option<Channel> {
	let mut frequency = None;
	let mut disabled = false;
	let mut no_ir = false;
	let mut radar = false;
	let (mut no_minus, mut no_plus, mut no_80, mut no_160) = (false, false, false, false);
	for item in info {
		match item {
			Nl80211FrequencyInfo::Freq(mhz) => frequency = Some(*mhz),
			Nl80211FrequencyInfo::Disabled => disabled = true,
			Nl80211FrequencyInfo::NoIr => no_ir = true,
			Nl80211FrequencyInfo::Radar => radar = true,
			Nl80211FrequencyInfo::NoHt40Minus => no_minus = true,
			Nl80211FrequencyInfo::NoHt40Plus => no_plus = true,
			Nl80211FrequencyInfo::No80Mhz => no_80 = true,
			Nl80211FrequencyInfo::No160Mhz => no_160 = true,
			_ => {}
		}
	}
	if disabled {
		return None;
	}
	let frequency = frequency?;
	let max_width = if no_minus && no_plus {
		20
	} else if no_80 {
		40
	} else if no_160 {
		80
	} else {
		160
	};
	Some(Channel {
		number: number(band, frequency)?,
		frequency,
		max_width,
		no_ir,
		radar,
	})
}

/// How the radio runs an access point beside a wireless client, from its interface combinations.
///
/// A combination admitting a station and an access point at once runs both independently where it
/// allows more than one channel, and on one channel otherwise. With none, the radio runs one at a
/// time.
fn alongside(combinations: &[Nl80211IfaceComb]) -> Alongside {
	let mut best = Alongside::OneAtATime;
	for combination in combinations {
		let mut limits = Vec::new();
		let mut total = 0;
		let mut channels = 1;
		for attribute in &combination.attributes {
			match attribute {
				Nl80211IfaceCombAttribute::Limits(list) => {
					for limit in list {
						let mut max = 0;
						let mut types = Vec::new();
						for attribute in &limit.attributes {
							match attribute {
								Nl80211IfaceCombLimitAttribute::Max(count) => max = *count,
								Nl80211IfaceCombLimitAttribute::Iftypes(list) => {
									types.clone_from(list);
								}
								_ => {}
							}
						}
						limits.push((max, types));
					}
				}
				Nl80211IfaceCombAttribute::Maxnum(count) => total = *count,
				Nl80211IfaceCombAttribute::NumChannels(count) => channels = *count,
				_ => {}
			}
		}
		if total < 2 || !station_beside_ap(&limits) {
			continue;
		}
		let found = if channels > 1 {
			Alongside::Independent
		} else {
			Alongside::SharedChannel
		};
		if rank(found) > rank(best) {
			best = found;
		}
	}
	best
}

/// Whether the limits of one combination leave room for a station and an access point together.
fn station_beside_ap(limits: &[(u32, Vec<Nl80211InterfaceType>)]) -> bool {
	let holds = |types: &[Nl80211InterfaceType], kind| types.contains(&kind);
	limits
		.iter()
		.enumerate()
		.any(|(i, (station_max, station))| {
			holds(station, Nl80211InterfaceType::Station)
				&& *station_max >= 1
				&& limits.iter().enumerate().any(|(j, (ap_max, ap))| {
					holds(ap, Nl80211InterfaceType::Ap)
						&& if i == j { *ap_max >= 2 } else { *ap_max >= 1 }
				})
		})
}

fn rank(alongside: Alongside) -> u8 {
	match alongside {
		Alongside::OneAtATime => 0,
		Alongside::SharedChannel => 1,
		Alongside::Independent => 2,
	}
}

/// Whether the radio can hold a connection to SAE, which iwd pins only where the radio has the SAE
/// AKM, CCMP and BIP-CMAC-128 (it ignores the pin silently otherwise).
///
/// The AKM is taken as supported where the wiphy lists no AKMs at all, which nl80211 says means all
/// of them. SAE itself needs a way to run, and these are the ways iwd runs it (`wiphy_can_connect_sae`
/// in iwd): `NL80211_FEATURE_SAE`, run in userspace either through `NL80211_CMD_AUTHENTICATE` and
/// `NL80211_CMD_ASSOCIATE` or, on a FullMAC driver that only connects, through external
/// authentication; or `NL80211_EXT_FEATURE_SAE_OFFLOAD`, run in the driver. The Pi's brcmfmac is the
/// external-authentication case.
fn sae(
	ciphers: &[Ieee80211CipherSuite],
	akms: Option<&[Ieee80211AkmSuite]>,
	features: Nl80211Features,
	extended: &[Nl80211ExtFeature],
) -> bool {
	let akm = akms.is_none_or(|list| list.contains(&Ieee80211AkmSuite::Sae));
	let in_userspace = features.contains(Nl80211Features::Sae);
	let offloaded = extended.contains(&Nl80211ExtFeature::SaeOffload);
	akm && (in_userspace || offloaded)
		&& ciphers.contains(&Ieee80211CipherSuite::Ccmp128)
		&& ciphers.contains(&Ieee80211CipherSuite::BipCmac128)
}

#[cfg(test)]
mod tests;
