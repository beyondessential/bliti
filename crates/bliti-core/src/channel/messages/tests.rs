use super::{
	super::config::{Failure, Outcome},
	super::envelope::{Fault, Reading, Refusal, Skip, read, round_trip_omissions},
	*,
};
use crate::provisioning::report::{DoneState, PromptKind, StepState};

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
		Message::State {
			busy: false,
			outcome: Outcome::Failed,
			failure: Some(Failure {
				at: "$['attachments'][0]['gateway']".to_owned(),
				reason: "no answer".to_owned(),
				reached: Some("addressing".to_owned()),
			}),
			attachments: vec![serde_json::json!({"is": "unavailable"})],
			capabilities: None,
		},
		Message::CannotAct {
			act: "scan".to_owned(),
			at: "$".to_owned(),
			reason: "another session holds the configuration".to_owned(),
		},
		Message::CannotConfigure {
			at: "$".to_owned(),
			reason: "another session holds the configuration".to_owned(),
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
		Message::CannotAct {
			act: "reboot".to_owned(),
			at: "$".to_owned(),
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

/// `state` always says whether the session is busy and what the outcome is, and carries a failure
/// only where it failed (CFG).
#[test]
fn state_carries_its_outcome_and_failure() {
	let verifying = Message::State {
		busy: true,
		outcome: Outcome::Verifying,
		failure: None,
		attachments: Vec::new(),
		capabilities: None,
	};
	assert_eq!(
		serde_json::from_slice::<Json>(&verifying.to_json()).unwrap(),
		serde_json::json!({"type": "state", "busy": true, "outcome": "verifying", "attachments": []})
	);
	assert_eq!(
		parse(r#"{"type":"state","busy":true,"outcome":"verifying","attachments":[]}"#).unwrap(),
		Reading::Message(verifying)
	);
	assert!(parse(r#"{"type":"state","outcome":"applied","attachments":[]}"#).is_err());
	assert!(
		parse(r#"{"type":"state","busy":false,"outcome":"unheard-of","attachments":[]}"#).is_err()
	);
	assert!(
		parse(
			r#"{"type":"state","busy":false,"outcome":"failed","failure":{"at":"$"},"attachments":[]}"#
		)
		.is_err()
	);
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
		Reading::Refused(Refusal::CriticalMembers {
			type_name: "reading".to_owned(),
			members: vec!["scope".to_owned()],
		})
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

/// The Tailscale stream's messages round trip (TSC).
#[test]
fn the_tailscale_messages_round_trip() {
	let result = serde_json::json!({"udp": true, "relays": []});
	for message in [
		Message::Tailscale,
		Message::TailscaleActs {
			acts: vec!["turn-off".to_owned(), "netcheck".to_owned()],
		},
		Message::TailscaleAct {
			act: "sign-in".to_owned(),
			method: Some("auth-key".to_owned()),
			key: Some(Secret::new("tskey-auth-abc")),
			name: Some("iti-7".to_owned()),
			peer: None,
		},
		Message::TailscaleAct {
			act: "netcheck".to_owned(),
			method: None,
			key: None,
			name: None,
			peer: None,
		},
		Message::TailscaleAccepted,
		Message::TailscaleLoginLink {
			url: Secret::new("https://login.example/a1b2"),
		},
		Message::TailscaleResult {
			result: result.as_object().unwrap().clone(),
		},
		Message::TailscaleFailed {
			reason: "the control server is out of reach".to_owned(),
		},
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message.clone()));
		assert_eq!(round_trip_omissions(&message), Vec::<String>::new());
	}
}

/// `tailscale-act` writes every member it carries in upper case, and an absent one not at all (TSC).
#[test]
fn tailscale_act_writes_its_members_critical() {
	let act = Message::TailscaleAct {
		act: "ping".to_owned(),
		method: None,
		key: None,
		name: None,
		peer: Some("canopy".to_owned()),
	};
	assert_eq!(
		String::from_utf8(act.to_json()).unwrap(),
		r#"{"ACT":"ping","PEER":"canopy","type":"tailscale-act"}"#
	);
	let sign_in = Message::TailscaleAct {
		act: "sign-in".to_owned(),
		method: Some("auth-key".to_owned()),
		key: Some(Secret::new("k")),
		name: Some("iti".to_owned()),
		peer: None,
	};
	assert_eq!(
		String::from_utf8(sign_in.to_json()).unwrap(),
		r#"{"ACT":"sign-in","KEY":"k","METHOD":"auth-key","NAME":"iti","type":"tailscale-act"}"#
	);
}

/// A `tailscale-act` carrying a critical member this build does not know is not acted on (TSC).
#[test]
fn tailscale_act_refuses_a_critical_member_it_does_not_know() {
	assert!(matches!(
		parse(r#"{"type":"tailscale-act","ACT":"ping","TIMEOUT":5}"#).unwrap(),
		Reading::Refused(_)
	));
	assert!(matches!(
		parse(r#"{"type":"tailscale-act","ACT":"ping","timeout":5}"#).unwrap(),
		Reading::Message(Message::TailscaleAct { .. })
	));
}

/// An auth key and a login link stay out of a message's `Debug` output (TSC).
#[test]
fn secrets_do_not_debug() {
	let act = Message::TailscaleAct {
		act: "sign-in".to_owned(),
		method: Some("auth-key".to_owned()),
		key: Some(Secret::new("tskey-auth-hunter2")),
		name: None,
		peer: None,
	};
	let link = Message::TailscaleLoginLink {
		url: Secret::new("https://login.example/hunter2"),
	};
	for message in [act, link] {
		assert!(!format!("{message:?}").contains("hunter2"));
	}
}

/// `end-of-batch` carries nothing beyond its type, and round trips (NFO).
#[test]
fn end_of_batch_round_trips() {
	let json = Message::EndOfBatch.to_json();
	assert_eq!(
		String::from_utf8(json.clone()).unwrap(),
		r#"{"type":"end-of-batch"}"#
	);
	assert_eq!(read(&json).unwrap(), Reading::Message(Message::EndOfBatch));
	assert_eq!(
		round_trip_omissions(&Message::EndOfBatch),
		Vec::<String>::new()
	);
}

/// The Tamanu stream's messages round trip (TMU).
#[test]
fn the_tamanu_messages_round_trip() {
	for message in [
		Message::Tamanu,
		Message::TamanuActs {
			acts: vec!["restart".to_owned()],
		},
		Message::TamanuActs { acts: Vec::new() },
		Message::TamanuAct {
			act: "start".to_owned(),
		},
		Message::TamanuAccepted,
		Message::TamanuFailed {
			reason: "tamanu-facility-api is not running".to_owned(),
		},
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message.clone()));
		assert_eq!(round_trip_omissions(&message), Vec::<String>::new());
	}
}

/// `tamanu-act` writes its act in upper case (TMU).
#[test]
fn tamanu_act_writes_its_act_critical() {
	let act = Message::TamanuAct {
		act: "restart".to_owned(),
	};
	assert_eq!(
		String::from_utf8(act.to_json()).unwrap(),
		r#"{"ACT":"restart","type":"tamanu-act"}"#
	);
}

/// A `tamanu-act` carrying a critical member this build does not know is not acted on (TMU).
#[test]
fn tamanu_act_refuses_a_critical_member_it_does_not_know() {
	assert!(matches!(
		parse(r#"{"type":"tamanu-act","ACT":"restart","FORCE":true}"#).unwrap(),
		Reading::Refused(_)
	));
	assert!(matches!(
		parse(r#"{"type":"tamanu-act","ACT":"restart","force":true}"#).unwrap(),
		Reading::Message(Message::TamanuAct { .. })
	));
}

/// `tamanu-failed` always says why.
#[test]
fn tamanu_failed_requires_its_reason() {
	assert!(parse(r#"{"type":"tamanu-failed"}"#).is_err());
}

/// `tailscale-failed` always says why.
#[test]
fn tailscale_failed_requires_its_reason() {
	assert!(parse(r#"{"type":"tailscale-failed"}"#).is_err());
}

fn step(id: &str, state: StepState) -> StepReport {
	StepReport {
		id: id.to_owned(),
		title: "Write the disk".to_owned(),
		done: DoneState::NotDone,
		why: None,
		state,
		progress: None,
		reason: None,
		result: None,
	}
}

/// The provisioning stream's messages round trip, each optional member both present and absent
/// (PRS).
#[test]
fn the_provision_messages_round_trip() {
	let mut running = step("disk", StepState::Running);
	running.progress = Some(0.5);
	running.why = Some("Up to date".to_owned());
	let mut failed = step("firmware", StepState::Failed);
	failed.reason = Some("the stick was pulled".to_owned());
	let mut passed = step("check", StepState::Passed);
	passed.result = Some(object(r#"{"serial":"abc"}"#));
	for message in [
		Message::Provision,
		Message::ProvisionAccepted,
		Message::ProvisionState {
			run: RunState::Idle,
			steps: Vec::new(),
			prompts: Vec::new(),
		},
		Message::ProvisionState {
			run: RunState::Waiting,
			steps: vec![running, failed, passed, step("net", StepState::Unchosen)],
			prompts: vec![
				PromptReport {
					id: "p1".to_owned(),
					step: "disk".to_owned(),
					kind: PromptKind::Secret,
					text: "Type the password".to_owned(),
					blocking: true,
				},
				PromptReport {
					id: "p2".to_owned(),
					step: "disk".to_owned(),
					kind: PromptKind::Other("photo".to_owned()),
					text: "Take a picture".to_owned(),
					blocking: false,
				},
			],
		},
		Message::ProvisionAct {
			act: "start".to_owned(),
			steps: Some(vec!["disk".to_owned(), "net".to_owned()]),
			prompt: None,
			value: None,
		},
		Message::ProvisionAct {
			act: "answer".to_owned(),
			steps: None,
			prompt: Some("p1".to_owned()),
			value: Some(Answer::boolean(true)),
		},
		Message::ProvisionAct {
			act: "answer".to_owned(),
			steps: None,
			prompt: Some("p2".to_owned()),
			value: Some(Answer::text("hunter2")),
		},
		Message::ProvisionAct {
			act: "retry".to_owned(),
			steps: None,
			prompt: None,
			value: None,
		},
	] {
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message.clone()));
		assert_eq!(round_trip_omissions(&message), Vec::<String>::new());
	}
}

fn object(json: &str) -> Map<String, Json> {
	serde_json::from_str(json).unwrap()
}

/// `provision-act` writes every member it carries in upper case (PRS).
#[test]
fn provision_act_writes_its_members_critical() {
	let answer = Message::ProvisionAct {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some("p1".to_owned()),
		value: Some(Answer::boolean(false)),
	};
	assert_eq!(
		String::from_utf8(answer.to_json()).unwrap(),
		r#"{"ACT":"answer","PROMPT":"p1","VALUE":false,"type":"provision-act"}"#
	);
	let start = Message::ProvisionAct {
		act: "start".to_owned(),
		steps: Some(vec!["a".to_owned()]),
		prompt: None,
		value: None,
	};
	assert_eq!(
		String::from_utf8(start.to_json()).unwrap(),
		r#"{"ACT":"start","STEPS":["a"],"type":"provision-act"}"#
	);
}

/// A `provision-act` carrying a critical member this build does not know is not acted on, and one
/// carrying an ignorable member is (PRS).
#[test]
fn provision_act_refuses_a_critical_member_it_does_not_know() {
	assert!(matches!(
		parse(r#"{"type":"provision-act","ACT":"stop","FORCE":true}"#).unwrap(),
		Reading::Refused(_)
	));
	assert!(matches!(
		parse(r#"{"type":"provision-act","ACT":"stop","force":true}"#).unwrap(),
		Reading::Message(Message::ProvisionAct { .. })
	));
}

/// A `provision-act` whose members are not written critical is not read as the act it names (PRS).
#[test]
fn provision_act_read_without_its_members_critical_is_a_member_it_lacks() {
	let Reading::Message(Message::ProvisionAct { act, .. }) =
		parse(r#"{"type":"provision-act","act":"stop"}"#).unwrap()
	else {
		panic!("an act written in lower case is still the act");
	};
	assert_eq!(act, "stop");
}

/// A `provision-state` states the whole run, so every member of it is required (PRS).
#[test]
fn provision_state_requires_its_members() {
	assert!(parse(r#"{"type":"provision-state","steps":[],"prompts":[]}"#).is_err());
	assert!(parse(r#"{"type":"provision-state","run":"idle","prompts":[]}"#).is_err());
	assert!(parse(r#"{"type":"provision-state","run":"idle","steps":[]}"#).is_err());
	assert!(parse(r#"{"type":"provision-state","run":"paused","steps":[],"prompts":[]}"#).is_err());
}

/// A step or a prompt carrying a member this build does not know is read, ignoring it (PRS).
#[test]
fn a_step_and_a_prompt_may_carry_further_members() {
	let read = parse(
		r#"{"type":"provision-state","run":"running","steps":[{"id":"a","title":"A","done":"unknown","state":"running","eta":9}],"prompts":[{"id":"p","step":"a","kind":"photo","text":"Look","blocking":false,"hint":"x"}]}"#,
	)
	.unwrap();
	let Reading::Message(Message::ProvisionState { steps, prompts, .. }) = read else {
		panic!("a state with further members is still a state");
	};
	assert_eq!(steps[0].id, "a");
	assert_eq!(prompts[0].kind, PromptKind::Other("photo".to_owned()));
}

/// The answer to a prompt does not reach a log through a message's debug output (PRS).
#[test]
fn an_answer_does_not_show_in_a_messages_debug_output() {
	let answer = Message::ProvisionAct {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some("p1".to_owned()),
		value: Some(Answer::text("hunter2")),
	};
	assert!(!format!("{answer:?}").contains("hunter2"));
}
