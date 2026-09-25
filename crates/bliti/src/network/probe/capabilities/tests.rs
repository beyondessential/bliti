use bliti_core::channel::{
	capabilities::{Rule, admits, check},
	config::Invalid,
};

use super::*;
use crate::network::probe::{BandInfo, Channel};

fn channel(number: u32, max_width: u32) -> Channel {
	Channel {
		number,
		frequency: 0,
		max_width,
		no_ir: false,
		radar: false,
	}
}

fn band(channels: Vec<Channel>, widths: &[u32]) -> BandInfo {
	BandInfo {
		channels,
		widths: widths.to_vec(),
	}
}

/// A Pi's built-in radio: one channel between client and hotspot, no SAE, no survey.
fn builtin() -> RadioInfo {
	RadioInfo {
		station: "wlan0".into(),
		model: "brcmfmac (SDIO 02d0:a9a6)".into(),
		bands: BTreeMap::from([
			(
				Band::TwoPointFour,
				band(vec![channel(1, 40), channel(6, 40)], &[20, 40]),
			),
			(Band::Five, band(vec![channel(36, 80)], &[20, 40, 80])),
		]),
		alongside: Some(Alongside::SharedChannel),
		sae: false,
		scan: true,
		survey: false,
	}
}

/// A USB adapter running client and hotspot on channels of their own.
fn adapter() -> RadioInfo {
	let no_ir = Channel {
		no_ir: true,
		..channel(13, 40)
	};
	let radar = Channel {
		radar: true,
		..channel(52, 160)
	};
	RadioInfo {
		station: "wlx00c0caa1b2c3".into(),
		model: "mt7921u (USB 0e8d:7961, Wireless_Device)".into(),
		bands: BTreeMap::from([
			(
				Band::TwoPointFour,
				band(
					vec![channel(1, 40), channel(6, 40), channel(11, 40), no_ir],
					&[20, 40],
				),
			),
			(
				Band::Five,
				band(
					vec![channel(36, 160), channel(40, 160), radar],
					&[20, 40, 80, 160],
				),
			),
			(Band::Six, band(vec![channel(1, 20)], &[20])),
		]),
		alongside: Some(Alongside::Independent),
		sae: true,
		scan: true,
		survey: true,
	}
}

/// A radio that cannot run an access point.
fn client_only() -> RadioInfo {
	RadioInfo {
		station: "wlan1".into(),
		model: "rtl8xxxu (USB 0bda:8179)".into(),
		bands: BTreeMap::from([(Band::TwoPointFour, band(vec![channel(6, 20)], &[20]))]),
		alongside: None,
		sae: true,
		scan: true,
		survey: false,
	}
}

fn object(value: Json) -> Map<String, Json> {
	let Json::Object(map) = value else {
		panic!("an object")
	};
	map
}

fn device() -> Map<String, Json> {
	capabilities(
		&[builtin(), adapter(), client_only()],
		&["eth0".into()],
		&Backend::stack(),
	)
}

fn document_of(capabilities: &Map<String, Json>) -> &Map<String, Json> {
	capabilities["document"].as_object().unwrap()
}

fn check_document(document: Json) -> Result<(), Invalid> {
	check(&object(document), document_of(&device()))
}

fn check_act(act: &str, message: Json) -> Result<(), Invalid> {
	let capabilities = device();
	let act = capabilities["acts"][act].as_object().unwrap();
	check(&object(message), act)
}

#[test]
fn the_shape_is_the_one_net_gives() {
	let personal = json!({ "psk": {}, "sae": {}, "psk-sae": {},
		"enterprise": { "eap": ["peap", "ttls", "tls", "pwd"] } });
	let expected = json!({
		"document": {
			"attachments": { "kind": {
				"wireless": {
					"interface": {
						"wlan0": {
							"security": { "kind": { "psk": {},
								"enterprise": { "eap": ["peap", "ttls", "tls", "pwd"] } } },
							"hidden": true
						},
						"wlx00c0caa1b2c3": { "security": { "kind": personal }, "hidden": true },
						"wlan1": { "security": { "kind": personal }, "hidden": true }
					},
					"nameservers": true
				},
				"wired-dynamic": { "interface": ["eth0"], "nameservers": true },
				"wired-static": { "interface": ["eth0"], "nameservers": true }
			} },
			"hotspot": {
				"interface": {
					"wlan0": { "band": {
						"2ghz": { "channel": [1, 6], "channel-width": [20, 40] },
						"5ghz": { "channel": [36], "channel-width": [20, 40, 80] }
					} },
					"wlx00c0caa1b2c3": { "band": {
						"2ghz": { "channel": [1, 6, 11], "channel-width": [20, 40] },
						"5ghz": { "channel": [36, 40], "channel-width": [20, 40, 80] }
					} }
				},
				"share-upstream": true,
				"isolate-clients": true,
				"dhcp-range": true
			},
			"regulatory-domain": true
		},
		"radios": {
			"wlan0": { "model": "brcmfmac (SDIO 02d0:a9a6)", "bands": ["2ghz", "5ghz"],
				"alongside": "shared-channel" },
			"wlx00c0caa1b2c3": { "model": "mt7921u (USB 0e8d:7961, Wireless_Device)",
				"bands": ["2ghz", "5ghz", "6ghz"], "alongside": "independent" },
			"wlan1": { "model": "rtl8xxxu (USB 0bda:8179)", "bands": ["2ghz"] }
		},
		"acts": {
			"scan": { "interface": { "wlan0": {}, "wlx00c0caa1b2c3": {}, "wlan1": {} } },
			"survey": { "interface": { "wlx00c0caa1b2c3": {} } },
			"wps": { "interface": {
				"wlan0": { "method": ["push-button", "pin"], "ssid": true },
				"wlx00c0caa1b2c3": { "method": ["push-button", "pin"], "ssid": true },
				"wlan1": { "method": ["push-button", "pin"], "ssid": true }
			} }
		}
	});
	assert_eq!(Json::Object(device()), expected);
}

fn wireless(interface: &str, kind: &str) -> Json {
	json!({ "kind": "wireless", "label": "Clinic", "enabled": true, "verify": true, "ssid": "Clinic", "interface": interface,
		"security": { "kind": kind, "passphrase": "a good long passphrase" } })
}

fn hotspot(extra: Json) -> Json {
	let mut hotspot =
		object(json!({ "enabled": true, "ssid": "setup", "passphrase": "read this aloud" }));
	hotspot.extend(object(extra));
	json!({ "attachments": [], "hotspot": hotspot })
}

#[test]
fn a_document_the_device_can_carry_passes() {
	check_document(json!({
		"attachments": [
			wireless("wlx00c0caa1b2c3", "sae"),
			wireless("wlan0", "psk"),
			{ "kind": "wired-static", "label": "Office", "enabled": true, "verify": true, "interface": "eth0",
			  "addresses": ["192.168.60.20/24"], "gateway": "192.168.60.1",
			  "nameservers": ["1.1.1.1"] }
		],
		"hotspot": { "enabled": true, "ssid": "setup", "passphrase": "read this aloud",
			"interface": "wlx00c0caa1b2c3", "band": "5ghz", "channel": 40, "channel-width": 80,
			"isolate-clients": false },
		"regulatory-domain": "VU"
	}))
	.unwrap();
}

#[test]
fn a_shared_channel_hotspot_passes_without_a_band() {
	check_document(hotspot(
		json!({ "interface": "wlan0", "share-upstream": false }),
	))
	.unwrap();
}

/// A shared-channel radio offers its hotspot a channel, which it may choose only where no wireless
/// candidate could take the radio (HOT).
#[test]
fn a_shared_channel_hotspot_chooses_its_channel_only_with_no_client_to_follow() {
	let chosen = json!({ "interface": "wlan0", "band": "2ghz", "channel": 6, "channel-width": 40 });
	admits(&object(hotspot(chosen.clone())), &device()).unwrap();

	let mut beside = hotspot(chosen);
	beside["attachments"] = json!([wireless("wlan0", "psk")]);
	let refused = admits(&object(beside), &device()).unwrap_err();
	assert_eq!(refused.rule, Rule::SharedChannel);
	assert_eq!(refused.invalid.at, "$['hotspot']['band']");
}

/// A shared-channel radio with no channel to start an access point on still runs a hotspot on its
/// client's.
#[test]
fn a_shared_channel_radio_with_no_channel_of_its_own_still_offers_a_hotspot() {
	let mut radio = builtin();
	for info in radio.bands.values_mut() {
		for channel in &mut info.channels {
			channel.no_ir = true;
		}
	}
	let caps = capabilities(&[radio], &[], &Backend::stack());
	assert_eq!(caps["document"]["hotspot"]["interface"]["wlan0"], json!({}));
}

#[test]
fn sae_is_refused_on_a_radio_that_cannot_hold_it() {
	for kind in ["sae", "psk-sae"] {
		let err = check_document(json!({ "attachments": [wireless("wlan0", kind)] })).unwrap_err();
		assert_eq!(err.at, "$['attachments'][0]['security']['kind']", "{kind}");
	}
}

#[test]
fn a_hotspot_on_a_radio_without_access_point_mode_is_refused() {
	let err = check_document(hotspot(json!({ "interface": "wlan1" }))).unwrap_err();
	assert_eq!(err.at, "$['hotspot']['interface']");
}

#[test]
fn a_width_the_stack_cannot_run_is_refused() {
	let err = check_document(hotspot(
		json!({ "interface": "wlx00c0caa1b2c3", "band": "5ghz",
		"channel": 36, "channel-width": 160 }),
	))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['channel-width']");
}

#[test]
fn a_channel_an_access_point_cannot_start_on_is_refused() {
	for (band, channel) in [("5ghz", 52), ("2ghz", 13)] {
		let err = check_document(hotspot(
			json!({ "interface": "wlx00c0caa1b2c3", "band": band,
			"channel": channel }),
		))
		.unwrap_err();
		assert_eq!(err.at, "$['hotspot']['channel']", "{band} {channel}");
	}
}

/// The radio can use 6 GHz, but the hotspot as rendered cannot.
#[test]
fn a_band_the_stack_cannot_run_a_hotspot_on_is_refused() {
	let err = check_document(hotspot(
		json!({ "interface": "wlx00c0caa1b2c3", "band": "6ghz" }),
	))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['band']");
}

#[test]
fn survey_is_offered_only_on_the_radios_that_can() {
	check_act("survey", json!({ "interface": "wlx00c0caa1b2c3" })).unwrap();
	check_act("survey", json!({})).unwrap();
	let err = check_act("survey", json!({ "interface": "wlan0" })).unwrap_err();
	assert_eq!(err.at, "$['interface']");
}

#[test]
fn wps_is_offered_by_method_on_every_radio() {
	check_act("wps", json!({ "interface": "wlan0", "method": "pin" })).unwrap();
	check_act("wps", json!({ "method": "push-button" })).unwrap();
	let err = check_act("wps", json!({ "method": "nfc" })).unwrap_err();
	assert_eq!(err.at, "$['method']");
}

#[test]
fn wps_may_name_any_network() {
	check_act("wps", json!({ "method": "pin", "ssid": "Clinic" })).unwrap();
	check_act(
		"wps",
		json!({ "interface": "wlan1", "method": "push-button", "ssid": "Anything at all" }),
	)
	.unwrap();
}

#[test]
fn survey_is_omitted_where_no_radio_can() {
	let capabilities = capabilities(&[builtin()], &[], &Backend::stack());
	assert!(capabilities["acts"].get("survey").is_none());
	assert!(capabilities["acts"].get("scan").is_some());
}

#[test]
fn a_device_without_radios_offers_nothing_wireless() {
	let capabilities = capabilities(&[], &["eth0".into()], &Backend::stack());
	assert!(capabilities.get("radios").is_none());
	assert_eq!(capabilities["acts"], json!({}));
	let document = document_of(&capabilities);
	assert!(document.get("hotspot").is_none());
	assert!(document.get("regulatory-domain").is_none());

	let err = check(
		&object(json!({ "attachments": [wireless("wlan0", "psk")] })),
		document,
	)
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['kind']");
	let err = check(&object(hotspot(json!({}))), document).unwrap_err();
	assert_eq!(err.at, "$['hotspot']");
}

/// A channel the radio can start an access point on but the renderer does not render, as the 4.9 GHz
/// channels numbered on the 5 GHz band are, is not offered: capabilities offer nothing the device
/// would then refuse.
#[test]
fn a_channel_the_renderer_refuses_is_not_offered() {
	let mut radio = adapter();
	radio
		.bands
		.get_mut(&Band::Five)
		.unwrap()
		.channels
		.push(channel(184, 20));
	let caps = capabilities(&[radio], &[], &Backend::stack());
	let channels =
		&caps["document"]["hotspot"]["interface"]["wlx00c0caa1b2c3"]["band"]["5ghz"]["channel"];
	assert_eq!(channels, &json!([36, 40]));
}

/// Every member name the capabilities carry is one MSG admits, band keys included, so a client can
/// read the message carrying them.
#[test]
fn the_capabilities_cross_the_wire() {
	use bliti_core::channel::{
		envelope::{Reading, read, write},
		messages::Message,
	};

	let sent = Message::Configuration {
		document: object(json!({ "attachments": [] })),
		capabilities: Some(device()),
	};
	let Ok(Reading::Message(Message::Configuration { capabilities, .. })) =
		read::<Message>(&write(&sent))
	else {
		panic!("the configuration reads back");
	};
	assert_eq!(capabilities, Some(device()));
}

#[test]
fn a_hotspot_width_fits_only_where_every_channel_it_spans_can_start_an_access_point() {
	let radar = |number| Channel {
		radar: true,
		..channel(number, 80)
	};
	let radio = RadioInfo {
		bands: BTreeMap::from([(
			Band::Five,
			band(
				vec![
					channel(36, 80),
					channel(40, 80),
					channel(44, 80),
					channel(48, 80),
					radar(52),
					radar(56),
					radar(60),
					radar(64),
					channel(165, 20),
				],
				&[20, 40, 80],
			),
		)]),
		..builtin()
	};
	let fits = |channel: u32, width: u32| {
		let hotspot = Hotspot {
			enabled: true,
			ssid: "bliti".into(),
			passphrase: "read me aloud".into(),
			interface: None,
			share_upstream: None,
			isolate_clients: None,
			dhcp_range: None,
			band: Some("5ghz".into()),
			channel: Some(channel),
			channel_width: Some(width),
		};
		hotspot_fits(std::slice::from_ref(&radio), &hotspot).map_err(|invalid| {
			assert_eq!(invalid.at, "$['hotspot']['channel-width']");
			invalid.reason
		})
	};
	assert_eq!(fits(36, 80), Ok(()));
	assert_eq!(fits(48, 80), Ok(()));
	assert_eq!(fits(44, 40), Ok(()));
	assert!(fits(64, 80).unwrap_err().contains("52, 56, 60, 64"));
	assert!(fits(165, 40).unwrap_err().contains("cannot be widened"));
	assert_eq!(fits(165, 20), Ok(()));
}
