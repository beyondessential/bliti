use bliti_core::channel::{
	envelope::{Reading, read},
	messages::Span,
};

use super::{
	streams::{loading, proposal, tailscale_act},
	*,
};

/// A proposal is a `configuration` carrying the document whole and no capabilities, with the
/// document critical on the wire so a device that cannot read it does not act on it (CFG).
#[test]
fn a_proposal_carries_the_document_critical() {
	let bytes =
		proposal(r#"{"attachments":[],"regulatory-domain":"VU","later":{"kept":1}}"#).unwrap();
	let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
	assert_eq!(wire["type"], "configuration");
	assert_eq!(wire["DOCUMENT"]["regulatory-domain"], "VU");
	assert!(wire.get("capabilities").is_none());

	let Ok(Reading::Message(Message::Configuration {
		document,
		capabilities,
	})) = read(&bytes)
	else {
		panic!("a proposal reads back as a configuration");
	};
	assert_eq!(capabilities, None);
	// A member this build does not know is sent as the operator's document carried it.
	assert_eq!(document["later"]["kept"], 1);
}

/// The power stream's answers and the feed's `going-away` reach the application with their
/// members as the wire names them, which is what it reads the act and its cause from (CTL).
#[test]
fn the_power_messages_are_described_as_the_wire_names_them() {
	for (message, expected) in [
		(
			Message::Acts {
				acts: vec!["reboot".to_owned()],
			},
			serde_json::json!({"type": "acts", "acts": ["reboot"]}),
		),
		(Message::Accepted, serde_json::json!({"type": "accepted"})),
		(
			Message::CannotAct {
				act: "reboot".to_owned(),
				at: "$['act']".to_owned(),
				reason: "no".to_owned(),
			},
			serde_json::json!({"type": "cannot-act", "act": "reboot", "at": "$['act']", "reason": "no"}),
		),
		(
			Message::GoingAway {
				act: "power-off".to_owned(),
				cause: "low-battery".to_owned(),
			},
			serde_json::json!({"type": "going-away", "act": "power-off", "cause": "low-battery"}),
		),
	] {
		let (described, fault) = describe(&message.to_json());
		assert_eq!(fault, None);
		let described: serde_json::Value = serde_json::from_str(&described).unwrap();
		assert_eq!(
			described,
			serde_json::json!({"kind": "message", "message": expected})
		);
	}
}

/// `end-of-batch` reaches the application as the wire names it, which is when it applies what the
/// feed sent since the last one (NFO).
#[test]
fn end_of_batch_is_described_as_the_wire_names_it() {
	let (described, fault) = describe(&Message::EndOfBatch.to_json());
	assert_eq!(fault, None);
	let described: serde_json::Value = serde_json::from_str(&described).unwrap();
	assert_eq!(
		described,
		serde_json::json!({"kind": "message", "message": {"type": "end-of-batch"}})
	);
}

/// A refusal names the type it refused, whether the type itself or a member of a type this build
/// knows was critical and unknown (STAT).
#[test]
fn a_refusal_names_its_type() {
	for (raw, type_name) in [
		(r#"{"TYPE":"wipe"}"#, "wipe"),
		(
			r#"{"type":"tamanu-act","ACT":"restart","FORCE":true}"#,
			"tamanu-act",
		),
	] {
		let (described, fault) = describe(raw.as_bytes());
		assert_eq!(fault, None);
		let described: serde_json::Value = serde_json::from_str(&described).unwrap();
		assert_eq!(described["kind"], "refused");
		assert_eq!(described["type"], type_name);
	}
}

/// A load carries the document as the file held it, whatever the device will make of it: the
/// device validates it and refuses with a reason (CRV).
#[test]
fn a_load_carries_the_document_as_it_stands() {
	let bytes =
		loading(r#"{"discharging":{"points":[[2.5,0],[4.2,1]],"learnt-from":3},"later":1}"#)
			.unwrap();
	let wire: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
	assert_eq!(wire["type"], "load");
	assert_eq!(wire["document"]["discharging"]["learnt-from"], 3);
	assert_eq!(wire["document"]["later"], 1);

	let Ok(Reading::Message(Message::Load { document })) = read(&bytes) else {
		panic!("a load reads back as a load");
	};
	assert_eq!(document["discharging"]["points"][1][0], 4.2);

	// Not an object is still sent, for the device to refuse.
	assert!(loading("[]").is_ok());
	assert!(loading("not json").is_err());
}

/// The curve stream's messages reach the application with their members as the wire names them,
/// the figures in seconds and the document whole (CRV).
#[test]
fn the_curve_messages_are_described_as_the_wire_names_them() {
	let document = serde_json::json!({
		"discharging": {"points": [[2.5, 0], [4.2, 1]], "learnt-from": 2, "error": 0.1, "duration": 26400},
	});
	for (message, expected) in [
		(
			Message::Curves {
				document: Some(document.clone()),
				lasts: Some(Span {
					duration: 18600.0,
					margin: 720.0,
				}),
				recharge: Some(Span {
					duration: 13200.0,
					margin: 1200.0,
				}),
			},
			serde_json::json!({
				"type": "curves",
				"document": document,
				"lasts": {"duration": 18600.0, "margin": 720.0},
				"recharge": {"duration": 13200.0, "margin": 1200.0},
			}),
		),
		(
			Message::Curves {
				document: None,
				lasts: None,
				recharge: None,
			},
			serde_json::json!({"type": "curves"}),
		),
		(Message::Accepted, serde_json::json!({"type": "accepted"})),
		(
			Message::CannotConfigure {
				at: "$['discharging']['points'][0]".to_owned(),
				reason: "the first point is above the floor".to_owned(),
			},
			serde_json::json!({
				"type": "cannot-configure",
				"at": "$['discharging']['points'][0]",
				"reason": "the first point is above the floor",
			}),
		),
	] {
		let (described, fault) = describe(&message.to_json());
		assert_eq!(fault, None);
		let described: serde_json::Value = serde_json::from_str(&described).unwrap();
		assert_eq!(
			described,
			serde_json::json!({"kind": "message", "message": expected})
		);
	}
}

/// The pre-proposal check is the core's, naming the first member capabilities do not cover.
#[test]
fn the_capability_check_names_what_is_not_covered() {
	let capabilities =
		r#"{"document":{"attachments":{"kind":{"wired-dynamic":{"interface":["eth0"]}}}}}"#;
	assert_eq!(
		capability_fault(
			r#"{"attachments":[{"kind":"wired-dynamic","label":"a","enabled":true,"verify":true,"interface":"eth0"}]}"#,
			capabilities
		),
		Ok(None)
	);
	let fault = capability_fault(
		r#"{"attachments":[{"kind":"wired-dynamic","label":"a","enabled":true,"verify":true,"interface":"eth1"}]}"#,
		capabilities,
	)
	.unwrap()
	.unwrap();
	assert_eq!(fault["at"], "$['attachments'][0]['interface']");
	assert_eq!(fault["rule"], "mirror");
	assert!(fault["reason"].is_string());
	assert!(capability_fault("[]", capabilities).is_err());
}

/// The radio rules of HOT are the core's too, named by the rule they break.
#[test]
fn the_capability_check_names_the_radio_rule_broken() {
	let capabilities = r#"{
		"document": {
			"attachments": {"kind": {"wireless": {"security": {"kind": {"psk": {}}}}}},
			"hotspot": {"interface": {"wlan0": {}}}
		},
		"radios": {"wlan0": {"model": "m", "bands": ["2ghz"], "alongside": "one-at-a-time"}}
	}"#;
	let document = r#"{
		"attachments": [{"kind": "wireless", "label": "a", "enabled": true, "verify": true, "ssid": "a",
		                 "security": {"kind": "psk", "passphrase": "12345678"}}],
		"hotspot": {"enabled": true, "ssid": "s", "passphrase": "12345678"}
	}"#;
	let fault = capability_fault(document, capabilities).unwrap().unwrap();
	assert_eq!(fault["at"], "$['hotspot']");
	assert_eq!(fault["rule"], "one-at-a-time");
	let placed = placement_fault(document, capabilities).unwrap().unwrap();
	assert_eq!(placed["at"], "$['hotspot']");
	assert_eq!(
		placement_fault(r#"{"attachments": []}"#, capabilities),
		Ok(None)
	);
}

/// Where the hotspot chooses its own channel is the core's answer too.
#[test]
fn the_channel_choice_is_the_cores() {
	let capabilities = r#"{
		"document": {"hotspot": {"interface": {"wlan0": {}}}},
		"radios": {"wlan0": {"model": "m", "bands": ["5ghz"], "alongside": "shared-channel"}}
	}"#;
	let beside = r#"{"attachments": [{"kind": "wireless", "label": "a"}], "hotspot": {}}"#;
	assert_eq!(
		channel_choice(beside, capabilities),
		Ok(Some(serde_json::json!({ "own": [], "follows": ["wlan0"] })))
	);
	assert_eq!(
		channel_choice(r#"{"attachments": []}"#, capabilities),
		Ok(Some(serde_json::json!({ "own": ["wlan0"], "follows": [] })))
	);
}

/// The names a page filters the chooser on are the ones the device may advertise, the first its
/// own, and it matches.
#[test]
fn the_expected_local_names_are_the_devices() {
	use bliti_core::key_schedule::{KeyFingerprint, PresenceToken};
	let token = PresenceToken::from_bytes([0x42; 16]);
	let fingerprint = KeyFingerprint::from_bytes([0x07; 18]);
	let code = QrCode {
		payload: QrPayload::new(token.clone(), fingerprint),
	};
	let names = code.local_names();
	assert_eq!(names, vec![Advertised::new(&token).to_local_name()]);
	let advertisement = code.read_local_name(&names[0]).unwrap();
	assert!(advertisement.supported());
	assert!(advertisement.matches());

	let other = QrCode {
		payload: QrPayload::new(PresenceToken::from_bytes([0x43; 16]), fingerprint),
	};
	let heard = other.read_local_name(&names[0]).unwrap();
	assert!(heard.supported());
	assert!(!heard.matches());

	// A marker this build does not implement is unsupported, not a different device.
	let later = Advertised {
		version: 99,
		..Advertised::new(&token)
	};
	let heard = code.read_local_name(&later.to_local_name()).unwrap();
	assert_eq!(heard.version(), 99);
	assert!(!heard.supported());
	assert!(!heard.matches());
}

#[test]
fn a_proposal_is_an_object() {
	assert!(proposal("[]").is_err());
	assert!(proposal("not json").is_err());
}

/// A sign-in by key carries its method and key critical, and its name only where one is given; a
/// ping carries its machine only where one is given (TSC).
#[test]
fn a_tailscale_act_carries_only_the_members_given() {
	let wire = |message: Message| -> serde_json::Value {
		serde_json::from_slice(&message.to_json()).unwrap()
	};
	let sign_in = wire(tailscale_act(
		"sign-in".to_owned(),
		Some("auth-key".to_owned()),
		Some("tskey-auth-k".to_owned()),
		None,
		None,
	));
	assert_eq!(
		sign_in,
		serde_json::json!({"type": "tailscale-act", "ACT": "sign-in", "METHOD": "auth-key", "KEY": "tskey-auth-k"})
	);
	let named = wire(tailscale_act(
		"sign-in".to_owned(),
		Some("auth-key".to_owned()),
		Some("k".to_owned()),
		Some("clinic-iti".to_owned()),
		None,
	));
	assert_eq!(named["NAME"], "clinic-iti");
	for peer in [None, Some(String::new())] {
		assert_eq!(
			wire(tailscale_act("ping".to_owned(), None, None, None, peer)),
			serde_json::json!({"type": "tailscale-act", "ACT": "ping"})
		);
	}
	assert_eq!(
		wire(tailscale_act(
			"ping".to_owned(),
			None,
			None,
			None,
			Some("canopy".to_owned())
		))["PEER"],
		"canopy"
	);
}

/// The Tamanu stream's act is `tamanu-act` with the act critical (TMU).
#[test]
fn a_tamanu_act_carries_its_act_critical() {
	assert_eq!(
		String::from_utf8(
			Message::TamanuAct {
				act: "restart".to_owned()
			}
			.to_json()
		)
		.unwrap(),
		r#"{"ACT":"restart","type":"tamanu-act"}"#
	);
}

/// A write fills what the link carries, less the ATT header, but never more than Web Bluetooth
/// writes in one value (CHN).
#[test]
fn a_write_fills_the_link_as_far_as_web_bluetooth_allows() {
	assert_eq!(super::write_chunk(23), 20);
	assert_eq!(super::write_chunk(247), 244);
	assert_eq!(super::write_chunk(517), 512);
}

/// The chooser is filtered on the name up to the suffix for every marker the build implements, so a
/// device is listed at any marker the client can drive (PRV).
#[test]
fn the_chooser_filters_on_each_marker_implemented() {
	let prefixes = crate::provision::name_prefixes();
	assert_eq!(prefixes, vec!["ITI_PROVISION_1_".to_owned()]);
}

#[test]
fn a_picked_name_is_read_into_its_marker_and_suffix() {
	use crate::provision::reading;

	assert_eq!(
		reading("ITI_PROVISION_1_K7QA"),
		Some((1, "K7QA".to_owned()))
	);
	assert_eq!(
		reading("ITI_PROVISION_1_FAULT"),
		Some((1, "FAULT".to_owned()))
	);
	// A marker this build does not implement is still read, so the page can say so.
	assert_eq!(
		reading("ITI_PROVISION_9_K7QA"),
		Some((9, "K7QA".to_owned()))
	);
	assert_eq!(reading("Someone's phone"), None);
	assert_eq!(reading("ITI_PROVISION_1_k7qa"), None);
}

/// An act carries only the members it was given, and a start carries the steps chosen (PRS).
#[test]
fn a_provision_act_carries_only_the_members_given() {
	use bliti_core::provisioning::report::Answer;

	use crate::streams::provision_act;

	let start = provision_act(
		"start".to_owned(),
		Some(vec!["hardware".to_owned(), "eeprom".to_owned()]),
		None,
		None,
	)
	.to_json();
	let wire: serde_json::Value = serde_json::from_slice(&start).unwrap();
	assert_eq!(wire["type"], "provision-act");
	assert_eq!(wire["ACT"], "start");
	assert_eq!(wire["STEPS"], serde_json::json!(["hardware", "eeprom"]));
	assert!(wire.get("PROMPT").is_none() && wire.get("VALUE").is_none());

	let answer = provision_act(
		"answer".to_owned(),
		None,
		Some("case".to_owned()),
		Some(Answer::boolean(true)),
	)
	.to_json();
	let wire: serde_json::Value = serde_json::from_slice(&answer).unwrap();
	assert_eq!(wire["PROMPT"], "case");
	assert_eq!(wire["VALUE"], true);
	assert!(wire.get("STEPS").is_none());

	let retry: serde_json::Value =
		serde_json::from_slice(&provision_act("retry".to_owned(), None, None, None).to_json())
			.unwrap();
	assert_eq!(retry["ACT"], "retry");
}

/// The state a device sends reaches the page with the members the wire names, the revision and the
/// image's date among them (PRS).
#[test]
fn a_provision_state_is_described_as_the_wire_names_it() {
	use bliti_core::provisioning::report::RunState;

	let (described, fault) = describe(
		&Message::ProvisionState {
			revision: 1,
			built: jiff::civil::Date::constant(2026, 10, 29),
			run: RunState::Idle,
			steps: Vec::new(),
			prompts: Vec::new(),
		}
		.to_json(),
	);
	assert_eq!(fault, None);
	let described: serde_json::Value = serde_json::from_str(&described).unwrap();
	assert_eq!(described["message"]["revision"], 1);
	assert_eq!(described["message"]["built"], "2026-10-29");
	assert_eq!(described["message"]["run"], "idle");
}
