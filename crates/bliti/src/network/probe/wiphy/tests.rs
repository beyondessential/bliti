use wl_nl80211::{
	Nl80211Band, Nl80211Command, Nl80211Frequency,
	packet_core::{NlaBuffer, Parseable},
};

use super::*;

const IFTYPE_STATION: u16 = 2;
const IFTYPE_AP: u16 = 3;
const IFTYPE_P2P_CLIENT: u16 = 8;
const IFTYPE_P2P_GO: u16 = 9;

/// One netlink attribute, header and padding included.
fn nla(kind: u16, payload: &[u8]) -> Vec<u8> {
	let length = u16::try_from(4 + payload.len()).unwrap();
	let mut out = Vec::new();
	out.extend(length.to_ne_bytes());
	out.extend(kind.to_ne_bytes());
	out.extend(payload);
	out.resize(out.len().next_multiple_of(4), 0);
	out
}

/// One interface combination: its limits (a maximum count and the interface types it covers), its
/// total, and how many channels it spans.
type Combination<'a> = (&'a [(u32, &'a [u16])], u32, u32);

/// `NL80211_ATTR_INTERFACE_COMBINATIONS`, built from bytes because wl-nl80211 keeps its combination
/// types from being constructed outside it.
fn combinations(list: &[Combination<'_>]) -> Nl80211Attr {
	let mut payload = Vec::new();
	for (index, (limits, total, channels)) in list.iter().enumerate() {
		let mut limit_list = Vec::new();
		for (index, (max, types)) in limits.iter().enumerate() {
			let flags: Vec<u8> = types.iter().flat_map(|kind| nla(*kind, &[])).collect();
			let mut limit = nla(1, &max.to_ne_bytes());
			limit.extend(nla(2, &flags));
			limit_list.extend(nla(u16::try_from(index + 1).unwrap(), &limit));
		}
		let mut combination = nla(1, &limit_list);
		combination.extend(nla(2, &total.to_ne_bytes()));
		combination.extend(nla(4, &channels.to_ne_bytes()));
		payload.extend(nla(u16::try_from(index + 1).unwrap(), &combination));
	}
	let bytes = nla(120, &payload);
	let attribute = Nl80211Attr::parse(&NlaBuffer::new(&bytes)).unwrap();
	assert!(matches!(attribute, Nl80211Attr::InterfaceCombination(_)));
	attribute
}

fn frequency(index: u16, mhz: u32, flags: &[Nl80211FrequencyInfo]) -> Nl80211Frequency {
	let mut info = vec![Nl80211FrequencyInfo::Freq(mhz)];
	info.extend(flags.iter().cloned());
	Nl80211Frequency { index, info }
}

fn band(kind: Nl80211BandType, info: Vec<Nl80211BandInfo>) -> Nl80211Attr {
	Nl80211Attr::WiphyBands(vec![Nl80211Band { kind, info }])
}

fn two_point_four() -> Nl80211Attr {
	band(
		Nl80211BandType::Band2GHz,
		vec![
			Nl80211BandInfo::HtCapa(Ieee80211HtCaps::SupWidth2040 | Ieee80211HtCaps::Sgi20),
			Nl80211BandInfo::Freqs(vec![
				frequency(0, 2412, &[Nl80211FrequencyInfo::NoHt40Minus]),
				frequency(1, 2437, &[]),
				frequency(2, 2462, &[Nl80211FrequencyInfo::NoHt40Plus]),
				frequency(3, 2472, &[Nl80211FrequencyInfo::NoIr]),
				frequency(4, 2484, &[Nl80211FrequencyInfo::Disabled]),
			]),
		],
	)
}

fn five(vht: Ieee80211VhtCapInfo) -> Nl80211Attr {
	band(
		Nl80211BandType::Band5GHz,
		vec![
			Nl80211BandInfo::HtCapa(Ieee80211HtCaps::SupWidth2040),
			Nl80211BandInfo::VhtCap(vht),
			Nl80211BandInfo::Freqs(vec![
				frequency(0, 5180, &[]),
				frequency(1, 5200, &[]),
				frequency(2, 5260, &[Nl80211FrequencyInfo::Radar]),
				frequency(3, 5825, &[Nl80211FrequencyInfo::No80Mhz]),
			]),
		],
	)
}

fn ciphers() -> Nl80211Attr {
	Nl80211Attr::CipherSuites(vec![
		Ieee80211CipherSuite::Tkip,
		Ieee80211CipherSuite::Ccmp128,
		Ieee80211CipherSuite::BipCmac128,
	])
}

/// A mac80211 radio that runs station and access point together on one channel.
fn mac80211() -> Vec<Nl80211Attr> {
	vec![
		Nl80211Attr::Wiphy(0),
		Nl80211Attr::MaxNumScanSsids(4),
		Nl80211Attr::SupportedIftypes(vec![Nl80211IfMode::Station, Nl80211IfMode::Ap]),
		two_point_four(),
		five(Ieee80211VhtCapInfo::ShortGi80),
		combinations(&[(&[(1, &[IFTYPE_STATION]), (1, &[IFTYPE_AP])], 2, 1)]),
		ciphers(),
		Nl80211Attr::Features(Nl80211Features::Sae),
		Nl80211Attr::SupportedCommand(vec![
			Nl80211Command::Authenticate,
			Nl80211Command::Associate,
			Nl80211Command::Connect,
		]),
	]
}

fn radio(attributes: &[Nl80211Attr]) -> RadioInfo {
	parse(attributes, "wlan0".into(), "test".into(), false)
}

fn numbers(info: &BandInfo) -> Vec<u32> {
	info.channels.iter().map(|channel| channel.number).collect()
}

#[test]
fn enabled_channels_are_numbered_and_disabled_ones_left_out() {
	let radio = radio(&mac80211());
	assert_eq!(numbers(&radio.bands[&Band::TwoPointFour]), [1, 6, 11, 13]);
	assert_eq!(numbers(&radio.bands[&Band::Five]), [36, 40, 52, 165]);

	let channel = |band, number| {
		*radio.bands[&band]
			.channels
			.iter()
			.find(|channel| channel.number == number)
			.unwrap()
	};
	assert!(channel(Band::TwoPointFour, 6).can_start_ap());
	assert!(channel(Band::TwoPointFour, 13).no_ir);
	assert!(channel(Band::Five, 52).radar);
	assert!(!channel(Band::Five, 52).can_start_ap());
	assert_eq!(channel(Band::Five, 165).max_width, 40);
	assert_eq!(channel(Band::Five, 36).max_width, 160);
}

/// A split dump sends one band over several messages, which are one band to the parser.
#[test]
fn a_band_split_over_messages_is_one_band() {
	let attributes = vec![
		band(
			Nl80211BandType::Band2GHz,
			vec![Nl80211BandInfo::Freqs(vec![frequency(0, 2412, &[])])],
		),
		band(
			Nl80211BandType::Band2GHz,
			vec![Nl80211BandInfo::Freqs(vec![frequency(1, 2437, &[])])],
		),
	];
	let radio = radio(&attributes);
	assert_eq!(numbers(&radio.bands[&Band::TwoPointFour]), [1, 6]);
}

#[test]
fn a_band_with_no_enabled_channel_is_not_one_the_radio_can_use() {
	let attributes = vec![band(
		Nl80211BandType::Band5GHz,
		vec![Nl80211BandInfo::Freqs(vec![frequency(
			0,
			5180,
			&[Nl80211FrequencyInfo::Disabled],
		)])],
	)];
	assert!(radio(&attributes).bands.is_empty());
}

#[test]
fn six_ghz_channels_are_numbered_from_5950() {
	let attributes = vec![band(
		Nl80211BandType::Band6GHz,
		vec![Nl80211BandInfo::Freqs(vec![
			frequency(0, 5935, &[]),
			frequency(1, 5955, &[]),
			frequency(2, 6415, &[]),
		])],
	)];
	let radio = radio(&attributes);
	let six = &radio.bands[&Band::Six];
	assert_eq!(numbers(six), [2, 1, 93]);
	assert_eq!(six.widths, [20]);
}

#[test]
fn widths_come_from_ht_and_vht() {
	let radio = radio(&mac80211());
	assert_eq!(radio.bands[&Band::TwoPointFour].widths, [20, 40]);
	assert_eq!(radio.bands[&Band::Five].widths, [20, 40, 80]);

	let wide = radio_with(five(Ieee80211VhtCapInfo::SuppChanWidth160mhz));
	assert_eq!(wide.bands[&Band::Five].widths, [20, 40, 80, 160]);

	let no_ht = radio_with(band(
		Nl80211BandType::Band2GHz,
		vec![Nl80211BandInfo::Freqs(vec![frequency(0, 2437, &[])])],
	));
	assert_eq!(no_ht.bands[&Band::TwoPointFour].widths, [20]);
}

/// A width the radio supports is left out where the regulatory domain permits it on no channel.
#[test]
fn a_width_no_channel_permits_is_left_out() {
	let radio = radio_with(band(
		Nl80211BandType::Band5GHz,
		vec![
			Nl80211BandInfo::HtCapa(Ieee80211HtCaps::SupWidth2040),
			Nl80211BandInfo::VhtCap(Ieee80211VhtCapInfo::SuppChanWidth160mhz),
			Nl80211BandInfo::Freqs(vec![frequency(0, 5180, &[Nl80211FrequencyInfo::No160Mhz])]),
		],
	));
	assert_eq!(radio.bands[&Band::Five].widths, [20, 40, 80]);
}

fn radio_with(band: Nl80211Attr) -> RadioInfo {
	radio(&[band])
}

fn with(mut attributes: Vec<Nl80211Attr>, replace: Nl80211Attr) -> Vec<Nl80211Attr> {
	let kind = std::mem::discriminant(&replace);
	attributes.retain(|attribute| std::mem::discriminant(attribute) != kind);
	attributes.push(replace);
	attributes
}

#[test]
fn one_channel_between_station_and_access_point_is_shared_channel() {
	assert_eq!(radio(&mac80211()).alongside, Some(Alongside::SharedChannel));
}

#[test]
fn more_than_one_channel_is_independent() {
	let attributes = with(
		mac80211(),
		combinations(&[
			(&[(1, &[IFTYPE_STATION]), (1, &[IFTYPE_AP])], 2, 1),
			(
				&[(1, &[IFTYPE_STATION]), (1, &[IFTYPE_AP, IFTYPE_P2P_GO])],
				2,
				2,
			),
		]),
	);
	assert_eq!(radio(&attributes).alongside, Some(Alongside::Independent));
}

/// One limit covering both types holds both only where it allows two interfaces.
#[test]
fn one_limit_holds_both_only_where_it_counts_two() {
	let shared = with(
		mac80211(),
		combinations(&[(&[(2, &[IFTYPE_STATION, IFTYPE_AP])], 2, 1)]),
	);
	assert_eq!(radio(&shared).alongside, Some(Alongside::SharedChannel));

	let single = with(
		mac80211(),
		combinations(&[(&[(1, &[IFTYPE_STATION, IFTYPE_AP])], 2, 2)]),
	);
	assert_eq!(radio(&single).alongside, Some(Alongside::OneAtATime));
}

#[test]
fn no_combination_holding_both_is_one_at_a_time() {
	let p2p_only = with(
		mac80211(),
		combinations(&[(
			&[
				(1, &[IFTYPE_STATION]),
				(1, &[IFTYPE_P2P_CLIENT, IFTYPE_P2P_GO]),
			],
			2,
			2,
		)]),
	);
	assert_eq!(radio(&p2p_only).alongside, Some(Alongside::OneAtATime));

	let one_in_total = with(
		mac80211(),
		combinations(&[(&[(1, &[IFTYPE_STATION]), (1, &[IFTYPE_AP])], 1, 2)]),
	);
	assert_eq!(radio(&one_in_total).alongside, Some(Alongside::OneAtATime));

	let mut none = mac80211();
	none.retain(|attribute| !matches!(attribute, Nl80211Attr::InterfaceCombination(_)));
	assert_eq!(radio(&none).alongside, Some(Alongside::OneAtATime));
}

#[test]
fn a_radio_without_access_point_mode_has_no_alongside() {
	let attributes = with(
		mac80211(),
		Nl80211Attr::SupportedIftypes(vec![Nl80211IfMode::Station]),
	);
	assert_eq!(radio(&attributes).alongside, None);
}

#[test]
fn sae_in_userspace_is_holdable() {
	assert!(radio(&mac80211()).sae);
}

#[test]
fn sae_offloaded_to_the_driver_is_holdable() {
	let attributes = with(
		with(mac80211(), Nl80211Attr::Features(Nl80211Features::empty())),
		Nl80211Attr::ExtFeatures(vec![Nl80211ExtFeature::SaeOffload]),
	);
	assert!(radio(&attributes).sae);
}

#[test]
fn sae_needs_ccmp_and_bip_cmac() {
	let attributes = with(
		mac80211(),
		Nl80211Attr::CipherSuites(vec![Ieee80211CipherSuite::Ccmp128]),
	);
	assert!(!radio(&attributes).sae);
}

#[test]
fn sae_needs_the_akm_where_the_wiphy_lists_akms() {
	let without = with(
		mac80211(),
		Nl80211Attr::AkmSuites(vec![Ieee80211AkmSuite::Psk]),
	);
	assert!(!radio(&without).sae);

	let listed = with(
		mac80211(),
		Nl80211Attr::AkmSuites(vec![Ieee80211AkmSuite::Psk, Ieee80211AkmSuite::Sae]),
	);
	assert!(radio(&listed).sae);
}

/// `NL80211_FEATURE_SAE` on a driver that only connects is external authentication, which iwd runs,
/// as it does on the Pi's brcmfmac.
#[test]
fn sae_by_external_authentication_is_holdable() {
	let attributes = with(
		mac80211(),
		Nl80211Attr::SupportedCommand(vec![Nl80211Command::Connect]),
	);
	assert!(radio(&attributes).sae);

	let neither = with(
		with(attributes, Nl80211Attr::Features(Nl80211Features::empty())),
		Nl80211Attr::ExtFeatures(vec![]),
	);
	assert!(!radio(&neither).sae);
}

#[test]
fn a_radio_scans_where_it_can_scan_for_an_ssid() {
	assert!(radio(&mac80211()).scan);
	let attributes = with(mac80211(), Nl80211Attr::MaxNumScanSsids(0));
	assert!(!radio(&attributes).scan);
}

#[test]
fn what_is_not_the_wiphys_is_carried_through() {
	let radio = parse(&mac80211(), "wlp1s0".into(), "iwlwifi".into(), true);
	assert_eq!(radio.station, "wlp1s0");
	assert_eq!(radio.model, "iwlwifi");
	assert!(radio.survey);
}
