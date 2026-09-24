use super::*;

fn document(json: Json) -> Result<Document, Invalid> {
	let Json::Object(map) = json else {
		panic!("a document is an object")
	};
	Document::parse(&map)
}

/// A full document parses, and serialising it and parsing it back is the same document.
#[test]
fn a_full_document_round_trips() {
	let parsed = document(serde_json::json!({
		"attachments": [
			{
				"kind": "wireless",
				"label": "clinic wifi",
				"enabled": true,
				"verify": true,
				"nameservers": ["10.0.0.1"],
				"ssid": "Clinic",
				"security": { "kind": "sae", "passphrase": "a good long passphrase" },
				"hidden": true,
				"bands": ["5ghz", "6ghz"]
			},
			{
				"kind": "wired-static",
				"label": "wall port",
				"enabled": true,
				"verify": true,
				"interface": "eth0",
				"addresses": ["192.168.1.10/24"],
				"gateway": "192.168.1.1"
			},
			{ "kind": "wired-dynamic", "label": "spare port", "enabled": true, "verify": true, "interface": "eth1" }
		],
		"hotspot": {
			"enabled": true,
			"ssid": "bliti-setup",
			"passphrase": "read this aloud",
			"share-upstream": false,
			"channel": 6
		},
		"regulatory-domain": "NZ"
	}))
	.unwrap();

	assert_eq!(parsed.attachments.len(), 3);
	assert_eq!(parsed.regulatory_domain.as_deref(), Some("NZ"));
	let round = Document::parse(&parsed.to_json()).unwrap();
	assert_eq!(parsed, round);
}

/// An empty attachment list is a valid document: a device out of the box attaches to nothing and
/// is reached over the channel (NET, HOT).
#[test]
fn no_attachments_and_no_hotspot_is_valid() {
	let parsed = document(serde_json::json!({ "attachments": [] })).unwrap();
	assert!(parsed.attachments.is_empty());
	assert!(parsed.hotspot.is_none());
	assert!(parsed.regulatory_domain.is_none());
}

/// A document without `attachments` at all is malformed rather than empty.
#[test]
fn a_document_without_attachments_is_invalid() {
	let err = document(serde_json::json!({})).unwrap_err();
	assert_eq!(err.at, "$['attachments']");
}

/// A static candidate with no gateway cannot be told from any other, so LINK rejects it, and the
/// fault names the missing member.
#[test]
fn a_static_candidate_without_a_gateway_is_invalid() {
	let err = document(serde_json::json!({
		"attachments": [{
			"kind": "wired-static",
			"label": "wall port",
			"enabled": true,
			"verify": true,
			"interface": "eth0",
			"addresses": ["192.168.1.10/24"]
		}]
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['gateway']");
	assert!(err.reached.is_none());
}

/// Several statics on one interface are the site-switching case and stay legal (LINK).
#[test]
fn several_statics_on_one_interface_are_allowed() {
	let parsed = document(serde_json::json!({
		"attachments": [
			{ "kind": "wired-static", "label": "site a", "enabled": true, "verify": true, "interface": "eth0",
			  "addresses": ["10.1.0.5/24"], "gateway": "10.1.0.1" },
			{ "kind": "wired-static", "label": "site b", "enabled": true, "verify": true, "interface": "eth0",
			  "addresses": ["10.2.0.5/24"], "gateway": "10.2.0.1" }
		]
	}))
	.unwrap();
	assert_eq!(parsed.attachments.len(), 2);
}

/// Two dynamic candidates on one interface are not (LINK).
#[test]
fn two_dynamic_candidates_on_one_interface_are_invalid() {
	let err = document(serde_json::json!({
		"attachments": [
			{ "kind": "wired-dynamic", "label": "a", "enabled": true, "verify": true, "interface": "eth0" },
			{ "kind": "wired-dynamic", "label": "b", "enabled": true, "verify": true, "interface": "eth0" }
		]
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][1]['interface']");
}

/// Each key-based security kind requires a passphrase, and the fault names it.
#[test]
fn a_key_network_without_a_passphrase_is_invalid() {
	for kind in ["psk", "sae", "psk-sae"] {
		let err = document(serde_json::json!({
			"attachments": [{
				"kind": "wireless", "label": "w", "enabled": true, "verify": true, "ssid": "S",
				"security": { "kind": kind }
			}]
		}))
		.unwrap_err();
		assert_eq!(
			err.at, "$['attachments'][0]['security']['passphrase']",
			"{kind}"
		);
	}
}

/// A set of bands names at least one, and none twice (WLAN).
#[test]
fn bands_name_at_least_one_and_none_twice() {
	for bands in [serde_json::json!([]), serde_json::json!(["5ghz", "5ghz"])] {
		let err = document(serde_json::json!({
			"attachments": [{
				"kind": "wireless", "label": "w", "enabled": true, "verify": true, "ssid": "S",
				"security": { "kind": "psk", "passphrase": "a good long passphrase" },
				"bands": bands
			}]
		}))
		.unwrap_err();
		assert_eq!(err.at, "$['attachments'][0]['bands']", "{bands}");
	}
}

/// An unknown security kind is rejected.
#[test]
fn an_unknown_security_kind_is_invalid() {
	let err = document(serde_json::json!({
		"attachments": [{
			"kind": "wireless", "label": "w", "enabled": true, "verify": true, "ssid": "S",
			"security": { "kind": "wep", "passphrase": "x" }
		}]
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['security']['kind']");
}

/// Enterprise keeps its method and credentials as raw members, and they survive the round trip.
#[test]
fn enterprise_credentials_survive_the_round_trip() {
	let parsed = document(serde_json::json!({
		"attachments": [{
			"kind": "wireless", "label": "eduroam", "enabled": true, "verify": true, "ssid": "eduroam",
			"security": {
				"kind": "enterprise", "eap": "peap",
				"identity": "user@site", "password": "secret"
			}
		}]
	}))
	.unwrap();
	let AttachmentKind::Wireless(wireless) = &parsed.attachments[0].kind else {
		panic!("expected a wireless candidate")
	};
	let Security::Enterprise { members } = &wireless.security else {
		panic!("expected enterprise security")
	};
	assert_eq!(members.get("eap").and_then(Json::as_str), Some("peap"));
	assert_eq!(Document::parse(&parsed.to_json()).unwrap(), parsed);
}

/// Every candidate says whether it is verified, whatever its kind (LINK).
#[test]
fn a_candidate_without_verify_is_invalid() {
	for verify in [None, Some(serde_json::json!("yes"))] {
		let mut candidate = serde_json::json!({ "kind": "wired-dynamic", "label": "a", "enabled": true, "interface": "eth0" });
		if let Some(verify) = verify {
			candidate["verify"] = verify;
		}
		let err = document(serde_json::json!({ "attachments": [candidate] })).unwrap_err();
		assert_eq!(err.at, "$['attachments'][0]['verify']");
	}
}

/// `verify` is written whichever way it is set.
#[test]
fn verify_is_written_either_way() {
	for verify in [true, false] {
		let parsed = document(serde_json::json!({
			"attachments": [{ "kind": "wired-dynamic", "label": "a", "enabled": true, "verify": verify, "interface": "eth0" }]
		}))
		.unwrap();
		assert_eq!(parsed.attachments[0].verify, verify);
		assert_eq!(parsed.to_json()["attachments"][0]["verify"], verify);
	}
}

/// Every candidate says whether it is turned on, whatever its kind (LINK).
#[test]
fn a_candidate_without_enabled_is_invalid() {
	for enabled in [None, Some(serde_json::json!(1))] {
		let mut candidate = serde_json::json!({ "kind": "wireless", "label": "w", "verify": true, "ssid": "S",
			"security": { "kind": "psk", "passphrase": "a good long passphrase" } });
		if let Some(enabled) = enabled {
			candidate["enabled"] = enabled;
		}
		let err = document(serde_json::json!({ "attachments": [candidate] })).unwrap_err();
		assert_eq!(err.at, "$['attachments'][0]['enabled']");
	}
}

/// A candidate turned off parses as one, and `enabled` is written whichever way it is set.
#[test]
fn enabled_is_written_either_way() {
	for enabled in [true, false] {
		let parsed = document(serde_json::json!({
			"attachments": [{ "kind": "wired-dynamic", "label": "a", "enabled": enabled, "verify": true, "interface": "eth0" }]
		}))
		.unwrap();
		assert_eq!(parsed.attachments[0].enabled, enabled);
		assert_eq!(parsed.to_json()["attachments"][0]["enabled"], enabled);
	}
}

/// An unknown attachment kind is rejected, naming the kind member.
#[test]
fn an_unknown_attachment_kind_is_invalid() {
	let err = document(serde_json::json!({
		"attachments": [{ "kind": "cellular", "label": "modem", "enabled": true, "verify": true }]
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['kind']");
}

/// A hotspot carries an SSID and a passphrase, both required, and the fault names the missing one.
#[test]
fn a_hotspot_without_a_passphrase_is_invalid() {
	let err = document(serde_json::json!({
		"attachments": [],
		"hotspot": { "enabled": true, "ssid": "setup" }
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['passphrase']");
}

/// A hotspot says whether it runs, and one turned off keeps what it carries through a round trip
/// (HOT).
#[test]
fn a_hotspot_carries_enabled_and_keeps_its_settings_when_off() {
	let err = document(serde_json::json!({
		"attachments": [],
		"hotspot": { "ssid": "setup", "passphrase": "read this aloud" }
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['enabled']");

	let parsed = document(serde_json::json!({
		"attachments": [],
		"hotspot": { "enabled": false, "ssid": "setup", "passphrase": "read this aloud", "channel": 6 }
	}))
	.unwrap();
	let hotspot = parsed.hotspot.as_ref().unwrap();
	assert!(!hotspot.enabled);
	assert_eq!(hotspot.channel, Some(6));
	assert_eq!(parsed.enabled_hotspot(), None);
	assert_eq!(parsed.to_json()["hotspot"]["enabled"], Json::Bool(false));
	assert_eq!(Document::parse(&parsed.to_json()).unwrap(), parsed);
}

/// An unset hotspot boolean parses to None, which HOT reads as its enabled default, and a set one
/// carries through.
#[test]
fn unset_hotspot_switches_are_none() {
	let parsed = document(serde_json::json!({
		"attachments": [],
		"hotspot": { "enabled": true, "ssid": "s", "passphrase": "p", "isolate-clients": false }
	}))
	.unwrap();
	let hotspot = parsed.hotspot.unwrap();
	assert_eq!(hotspot.share_upstream, None);
	assert_eq!(hotspot.isolate_clients, Some(false));
	assert_eq!(hotspot.dhcp_range, None);
}

/// An absent optional member is left out of the serialisation, which is how a device reads unset.
#[test]
fn absent_optionals_are_omitted_on_write() {
	let doc = Document {
		attachments: vec![],
		hotspot: None,
		regulatory_domain: None,
	};
	let json = doc.to_json();
	assert!(!json.contains_key("hotspot"));
	assert!(!json.contains_key("regulatory-domain"));
	assert_eq!(json.get("attachments"), Some(&Json::Array(vec![])));
}

/// Paths are RFC 9535 Normalized Paths: bracketed, single-quoted, with its escapes, so a hyphenated
/// name the dot shorthand cannot carry is spelled one way only.
#[test]
fn paths_are_normalized_jsonpath() {
	assert_eq!(path(&[]), "$");
	assert_eq!(
		path(&[Name("hotspot"), Name("share-upstream")]),
		"$['hotspot']['share-upstream']"
	);
	assert_eq!(
		path(&[Name("attachments"), Index(2), Name("gateway")]),
		"$['attachments'][2]['gateway']"
	);
	assert_eq!(path(&[Name("it's\\\n\u{1}")]), r"$['it\'s\\\n\u0001']");
}

/// A wireless candidate and the hotspot may name the interface that carries them, and naming none
/// leaves the choice to the device (LINK, HOT).
#[test]
fn wireless_and_hotspot_name_an_interface_or_leave_it_to_the_device() {
	let parsed = document(serde_json::json!({
		"attachments": [
			{ "kind": "wireless", "label": "uplink", "enabled": true, "verify": true, "ssid": "Clinic", "interface": "wlx00c0caa1b2c3",
			  "security": { "kind": "sae", "passphrase": "a good long passphrase" } },
			{ "kind": "wireless", "label": "either", "enabled": true, "verify": true, "ssid": "Office",
			  "security": { "kind": "psk", "passphrase": "another passphrase" } }
		],
		"hotspot": { "enabled": true, "ssid": "setup", "passphrase": "read this aloud", "interface": "wlan0" }
	}))
	.unwrap();
	let interfaces: Vec<_> = parsed
		.attachments
		.iter()
		.map(|a| match &a.kind {
			AttachmentKind::Wireless(w) => w.interface.as_deref(),
			_ => panic!("expected wireless candidates"),
		})
		.collect();
	assert_eq!(interfaces, [Some("wlx00c0caa1b2c3"), None]);
	assert_eq!(
		parsed.hotspot.as_ref().unwrap().interface.as_deref(),
		Some("wlan0")
	);
	assert_eq!(Document::parse(&parsed.to_json()).unwrap(), parsed);

	let err = document(serde_json::json!({
		"attachments": [],
		"hotspot": { "enabled": true, "ssid": "s", "passphrase": "p", "interface": 0 }
	}))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['interface']");
}
