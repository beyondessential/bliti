use super::{
	super::envelope::{Fault, Reading, Refusal, Skip, read, round_trip_omissions},
	*,
};

fn parse(json: &str) -> Result<Reading<Message>, Fault> {
	read(json.as_bytes())
}

/// A reading standing in for whatever a device reports, for exercising the envelope around it.
fn cpu() -> Entry {
	Entry::fraction(20_308_140, "cpu-usage", 0.12)
}

/// One `hello`, and each end reads its peer's without the type being distinguished by name (MSG).
#[test]
fn one_hello_round_trips() {
	let message = Message::Hello {
		name: "bliti-web".to_owned(),
		version: "0.1.0".to_owned(),
	};
	let json = message.to_json();
	assert_eq!(read(&json).unwrap(), Reading::Message(message));
}

#[test]
fn the_hello_json_shape_is_stable() {
	let json = Message::Hello {
		name: "bliti".to_owned(),
		version: "0.1.0".to_owned(),
	}
	.to_json();
	assert_eq!(
		String::from_utf8(json).unwrap(),
		r#"{"name":"bliti","type":"hello","version":"0.1.0"}"#
	);
}

#[test]
fn subscribe_marks_its_selector_critical_on_the_wire() {
	let json = Message::Subscribe {
		topic: "default".to_owned(),
	}
	.to_json();
	assert_eq!(
		String::from_utf8(json).unwrap(),
		r#"{"TOPIC":"default","type":"subscribe"}"#
	);
}

#[test]
fn a_fact_and_a_reading_round_trip() {
	for message in [
		Message::Fact(Entry::text(20_308_140, "hostname", "tamanu-iti")),
		Message::Reading(cpu()),
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message));
	}
}

/// A `fact` and a `reading` of the same catalogue name are different entries: the message type
/// keeps them apart (NFO).
#[test]
fn a_fact_and_a_reading_of_one_name_do_not_collide() {
	let fact = Message::Fact(Entry::quantity(1, "memory-bytes", "bytes", 8_000_000.0));
	let reading = Message::Reading(Entry::quantity(1, "memory-bytes", "bytes", 5_000_000.0));
	assert_ne!(fact, reading);
	assert_eq!(read(&fact.to_json()).unwrap(), Reading::Message(fact));
	assert_eq!(read(&reading.to_json()).unwrap(), Reading::Message(reading));
}

#[test]
fn a_mixed_case_name_is_a_fault() {
	assert_eq!(
		parse(r#"{"type":"subscribe","Topic":"default"}"#).unwrap_err(),
		Fault::MalformedName("Topic".to_owned())
	);
}

#[test]
fn the_same_name_twice_is_a_fault() {
	assert_eq!(
		parse(r#"{"type":"subscribe","topic":"a","TOPIC":"b"}"#).unwrap_err(),
		Fault::DuplicateName("topic".to_owned())
	);
}

/// An ignorable member this build does not know is passed over, and the rest is read.
#[test]
fn an_unknown_ignorable_member_is_skipped() {
	assert_eq!(
		parse(r#"{"type":"subscribe","TOPIC":"default","cadence":"fast"}"#).unwrap(),
		Reading::Message(Message::Subscribe {
			topic: "default".to_owned()
		})
	);
}

/// A type this build does not know is passed over whole (MSG).
#[test]
fn an_unknown_type_is_skipped() {
	assert_eq!(
		parse(r#"{"type":"reboot","when":"now"}"#).unwrap(),
		Reading::Skipped(Skip::UnknownType("reboot".to_owned()))
	);
}

/// Unknown members are found by round-tripping through these types, so a member that serialises
/// away would be read as one this build has never heard of. Checked rather than remembered.
#[test]
fn every_message_type_survives_the_round_trip() {
	let entry = Entry::quantity(20_308_140, "temperature", "celsius", 48.5)
		.with_trait("sensor", Json::String("cpu".to_owned()))
		.with_limit(75.0, "Cooling")
		.warning("a sensor is warm");
	let messages = [
		Message::Hello {
			name: "a".to_owned(),
			version: "1".to_owned(),
		},
		Message::Subscribe {
			topic: "default".to_owned(),
		},
		Message::Fact(Entry::text(1, "hostname", "iti")),
		Message::Reading(entry),
		Message::Reading(Entry::broken(
			1,
			"battery-charge",
			"fraction",
			"no answer from the gauge",
		)),
		Message::Wps {
			method: "push-button".to_owned(),
			interface: Some("wlan0".to_owned()),
			ssid: Some("clinic".to_owned()),
		},
	];
	for message in &messages {
		assert_eq!(
			round_trip_omissions(message),
			Vec::<String>::new(),
			"{message:?}"
		);
	}
}

/// The power stream's messages round trip, and `act` carries its selector critical (CTL).
#[test]
fn the_power_messages_round_trip() {
	for message in [
		Message::Power,
		Message::Acts {
			acts: vec!["restart".to_owned(), "reboot".to_owned()],
		},
		Message::Act {
			act: "reboot".to_owned(),
		},
		Message::Accepted,
		Message::Refused {
			reason: "already going".to_owned(),
		},
		Message::GoingAway {
			act: "power-off".to_owned(),
			cause: "low-battery".to_owned(),
		},
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message.clone()));
		assert_eq!(round_trip_omissions(&message), Vec::<String>::new());
	}
	assert_eq!(
		String::from_utf8(
			Message::Act {
				act: "reboot".to_owned()
			}
			.to_json()
		)
		.unwrap(),
		r#"{"ACT":"reboot","type":"act"}"#
	);
	assert_eq!(
		String::from_utf8(
			Message::GoingAway {
				act: "reboot".to_owned(),
				cause: "manual-control".to_owned(),
			}
			.to_json()
		)
		.unwrap(),
		r#"{"act":"reboot","cause":"manual-control","type":"going-away"}"#
	);
	assert_eq!(
		String::from_utf8(Message::Power.to_json()).unwrap(),
		r#"{"type":"power"}"#
	);
}

/// `going-away` always says why, so one without a cause is malformed (CTL, "Going away"). A cause
/// this build does not know is still read, for the client to go by the act.
#[test]
fn going_away_requires_its_cause() {
	assert!(matches!(
		parse(r#"{"type":"going-away","act":"reboot"}"#).unwrap_err(),
		Fault::Malformed { .. }
	));
	assert!(parse(r#"{"type":"going-away","act":"reboot","cause":7}"#).is_err());
	assert_eq!(
		parse(r#"{"type":"going-away","act":"reboot","cause":"meteor"}"#).unwrap(),
		Reading::Message(Message::GoingAway {
			act: "reboot".to_owned(),
			cause: "meteor".to_owned(),
		})
	);
}

/// The old opening message of the power stream is a type this build does not know.
#[test]
fn control_is_no_longer_a_message() {
	assert_eq!(
		parse(r#"{"type":"control"}"#).unwrap(),
		Reading::Skipped(Skip::UnknownType("control".to_owned()))
	);
}

fn document() -> Json {
	serde_json::json!({
		"discharging": {
			"points": [[2.8, 0], [4.2, 1]],
			"learnt-from": 0,
			"error": 0.2,
			"duration": 36000,
		},
	})
}

/// The curve stream's messages round trip (CRV).
#[test]
fn the_curve_messages_round_trip() {
	for message in [
		Message::Curve,
		Message::Curves {
			document: Some(document()),
			lasts: Some(Span {
				duration: 36000.0,
				margin: 1800.5,
			}),
			recharge: Some(Span {
				duration: 7200.0,
				margin: 600.0,
			}),
		},
		Message::Curves {
			document: Some(document()),
			lasts: Some(Span {
				duration: 36000.0,
				margin: 1800.0,
			}),
			recharge: None,
		},
		Message::Curves {
			document: None,
			lasts: None,
			recharge: None,
		},
		Message::Load {
			document: document(),
		},
		Message::Reset,
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message.clone()));
		assert_eq!(round_trip_omissions(&message), Vec::<String>::new());
	}
}

/// `curves` carries each span as `{duration, margin}`, and leaves out what the device does not hold
/// rather than writing it empty (CRV).
#[test]
fn the_curve_json_shape_is_stable() {
	let curves = Message::Curves {
		document: Some(document()),
		lasts: Some(Span {
			duration: 36000.0,
			margin: 1800.0,
		}),
		recharge: None,
	};
	assert_eq!(
		serde_json::from_slice::<Json>(&curves.to_json()).unwrap(),
		serde_json::json!({
			"type": "curves",
			"document": document(),
			"lasts": {"duration": 36000.0, "margin": 1800.0},
		})
	);
	let none = Message::Curves {
		document: None,
		lasts: None,
		recharge: None,
	};
	assert_eq!(
		String::from_utf8(none.to_json()).unwrap(),
		r#"{"type":"curves"}"#
	);
	assert_eq!(
		String::from_utf8(Message::Curve.to_json()).unwrap(),
		r#"{"type":"curve"}"#
	);
	assert_eq!(
		String::from_utf8(Message::Reset.to_json()).unwrap(),
		r#"{"type":"reset"}"#
	);
}

/// A document is raw JSON to the envelope: one the device would refuse is still read, so the device
/// can say what is wrong with it. A `load` with no document at all is malformed (CRV).
#[test]
fn a_load_is_read_whatever_its_document() {
	assert_eq!(
		parse(r#"{"type":"load","document":[1,2]}"#).unwrap(),
		Reading::Message(Message::Load {
			document: serde_json::json!([1, 2])
		})
	);
	assert!(matches!(
		parse(r#"{"type":"load"}"#).unwrap_err(),
		Fault::Malformed { .. }
	));
}

/// A span is a pair of numbers; one missing either, or carrying something else, is malformed.
#[test]
fn a_span_carries_two_numbers() {
	assert_eq!(
		parse(r#"{"type":"curves","lasts":{"duration":3600,"margin":60}}"#).unwrap(),
		Reading::Message(Message::Curves {
			document: None,
			lasts: Some(Span {
				duration: 3600.0,
				margin: 60.0,
			}),
			recharge: None,
		})
	);
	for broken in [
		r#"{"type":"curves","lasts":{"duration":3600}}"#,
		r#"{"type":"curves","lasts":{"duration":"long","margin":60}}"#,
		r#"{"type":"curves","recharge":3600}"#,
	] {
		assert!(
			matches!(parse(broken).unwrap_err(), Fault::Malformed { .. }),
			"{broken}"
		);
	}
}

/// An act a newer device lists is still read, as a string this build does not know (CTL).
#[test]
fn acts_are_read_whatever_they_name() {
	assert_eq!(
		parse(r#"{"type":"acts","acts":["reboot","hibernate"]}"#).unwrap(),
		Reading::Message(Message::Acts {
			acts: vec!["reboot".to_owned(), "hibernate".to_owned()]
		})
	);
	assert!(parse(r#"{"type":"acts","acts":["reboot",7]}"#).is_err());
}

/// `wps` names the network it is for by `ssid`, and leaves it out where it is for any (CFG).
#[test]
fn wps_carries_its_ssid_where_it_names_one() {
	let named = Message::Wps {
		method: "pin".to_owned(),
		interface: None,
		ssid: Some("clinic".to_owned()),
	};
	assert_eq!(
		serde_json::from_slice::<Json>(&named.to_json()).unwrap(),
		serde_json::json!({"type": "wps", "method": "pin", "ssid": "clinic"})
	);
	assert!(matches!(
		parse(r#"{"type":"wps","method":"pin","ssid":"clinic"}"#).unwrap(),
		Reading::Message(message) if message == named
	));
	assert!(matches!(
		parse(r#"{"type":"wps","method":"pin"}"#).unwrap(),
		Reading::Message(Message::Wps { ssid: None, .. })
	));
	assert!(parse(r#"{"type":"wps","method":"pin","ssid":7}"#).is_err());
}

/// `subscribe` pins its selector critical: arriving plain is a fault, as is any other critical
/// member alongside it (MSG).
#[test]
fn subscribe_pins_its_selector_critical() {
	assert!(matches!(
		parse(r#"{"type":"subscribe","topic":"default"}"#).unwrap_err(),
		Fault::CriticalRequired { .. }
	));
	assert!(matches!(
		parse(r#"{"type":"subscribe","TOPIC":"default","REDACT":["cpu"]}"#).unwrap_err(),
		Fault::CriticalNotAllowed { .. }
	));
	assert_eq!(
		parse(r#"{"type":"subscribe","TOPIC":"default"}"#).unwrap(),
		Reading::Message(Message::Subscribe {
			topic: "default".to_owned()
		})
	);
}

/// The hellos carry no critical member, so one arriving is a peer breaking the protocol (MSG).
#[test]
fn a_critical_member_on_a_hello_is_a_fault() {
	assert!(matches!(
		parse(r#"{"type":"hello","name":"a","version":"1","MODE":"strict"}"#).unwrap_err(),
		Fault::CriticalNotAllowed { .. }
	));
}

/// A feature type still refuses rather than faults on an unknown critical member, and a nested one
/// costs only the object that carried it, not the rest of the message (MSG).
#[test]
fn an_unknown_critical_member_nested_in_a_reading_is_refused() {
	let json = r#"{"type":"reading","at":1,"measurement":"cpu-usage","traits":{"status":{"is":"passed"}},"kind":"fraction","value":0.1,"SCOPE":"site"}"#;
	assert_eq!(
		parse(json).unwrap(),
		Reading::Refused(Refusal::CriticalMembers(vec!["scope".to_owned()]))
	);
}

/// An unknown type named by a critical `TYPE` is reported rather than passed over in silence.
#[test]
fn an_unknown_critical_type_is_refused() {
	assert_eq!(
		parse(r#"{"TYPE":"wipe","confirm":true}"#).unwrap(),
		Reading::Refused(Refusal::CriticalType("wipe".to_owned()))
	);
}

/// A nested ignorable member is passed over, and the message is read.
#[test]
fn a_nested_ignorable_member_is_skipped() {
	let json = r#"{"type":"reading","at":1,"measurement":"cpu-usage","traits":{"status":{"is":"passed"}},"kind":"fraction","value":0.1,"cores":4}"#;
	let Reading::Message(Message::Reading(entry)) = parse(json).unwrap() else {
		panic!("expected a reading")
	};
	assert_eq!(entry.name, "cpu-usage");
}

#[test]
fn a_known_type_missing_what_it_requires_is_a_fault() {
	assert!(matches!(
		parse(r#"{"type":"subscribe"}"#).unwrap_err(),
		Fault::Malformed { .. }
	));
	assert!(matches!(
		parse(r#"{"type":"reading","measurement":"cpu-usage"}"#).unwrap_err(),
		Fault::Malformed { .. }
	));
}

#[test]
fn malformed_json_is_a_fault() {
	assert!(matches!(
		parse("this is not json").unwrap_err(),
		Fault::NotJson(_)
	));
	assert_eq!(parse("[]").unwrap_err(), Fault::NotObject);
}

#[test]
fn a_message_without_a_string_type_is_a_fault() {
	assert_eq!(parse(r#"{"topic":"default"}"#).unwrap_err(), Fault::NoType);
	assert_eq!(
		parse(r#"{"type":42,"topic":"default"}"#).unwrap_err(),
		Fault::TypeNotString
	);
}
