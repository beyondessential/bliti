//! The application messages carried over the channel, as JSON.
//!
//! Behaviour is specified in BLI-MSG. The envelope they ride in, and the three outcomes of reading
//! one, live in [`super::envelope`]; this module carries the message types themselves.
//!
//! This card's set is the part every later feature inherits: each end names itself, and a client
//! subscribes to what a device sends continuously. A feature adds its own types and its own topics.
//!
//! No message type skips serialising a member it holds. Unknown members are found by round-tripping
//! through these types, so a member that serialises away would be read as one this build has never
//! heard of.

use serde::{Deserialize, Serialize};

use super::{
	envelope::{Criticality, Message},
	readings,
};

/// A message from the client to the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
	/// The client naming itself, first on the control stream it opens.
	#[serde(rename = "client-hello")]
	Hello {
		/// What the client software calls itself. Opaque to the device, which logs it.
		name: String,
		/// The version the client is at. Opaque to the device, which logs it.
		version: String,
	},

	/// Subscribe to what a device sends continuously, first on a stream opened for the purpose.
	/// Closing that stream is the unsubscribe.
	#[serde(rename = "subscribe")]
	Subscribe {
		/// What is being subscribed to. Topics are defined by the feature that owns them.
		topic: String,
	},
}

/// A message from the device to the client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeviceMessage {
	/// The device naming itself, first on the reporting stream it opens.
	#[serde(rename = "device-hello")]
	Hello {
		/// What the device software calls itself. Opaque to the client, which displays it.
		name: String,
		/// The version the device is at. Opaque to the client, which displays it.
		version: String,
	},

	/// What the device is: readings that do not change while it runs, or change rarely. Sent on the
	/// reporting stream and again whenever they change (BLI-SYS).
	#[serde(rename = "system-identity")]
	SystemIdentity {
		/// The static readings.
		readings: Vec<readings::Reading>,
	},

	/// One sample of the device's live readings, sent on a `system` subscription (BLI-SYS).
	#[serde(rename = "system-sample")]
	SystemSample {
		/// Milliseconds since the device booted, when the sample was taken.
		at: u64,
		/// The readings taken. Need not carry every reading.
		readings: Vec<readings::Reading>,
	},

	/// The buffered window, sent first on a `system` subscription so a graph is populated the moment
	/// it appears rather than filling from empty (BLI-SYS).
	#[serde(rename = "system-history")]
	SystemHistory {
		/// Earlier samples, oldest first.
		samples: Vec<readings::Sample>,
	},
}

impl Message for ClientMessage {
	fn knows(type_name: &str) -> bool {
		matches!(type_name, "client-hello" | "subscribe")
	}

	/// `client-hello` carries no critical member; `subscribe` carries exactly one, its selector, so
	/// that a device cannot act on a request to be sent something it has not read (BLI-MSG).
	fn criticality(type_name: &str) -> Criticality {
		match type_name {
			"client-hello" => Criticality::Exactly(&[]),
			"subscribe" => Criticality::Exactly(&["topic"]),
			_ => Criticality::Unconstrained,
		}
	}

	fn critical_members(type_name: &str) -> &'static [&'static str] {
		match type_name {
			"subscribe" => &["topic"],
			_ => &[],
		}
	}
}

impl Message for DeviceMessage {
	fn knows(type_name: &str) -> bool {
		matches!(
			type_name,
			"device-hello" | "system-identity" | "system-sample" | "system-history"
		)
	}

	/// `device-hello` carries no critical member. The system types belong to BLI-SYS, which pins
	/// nothing.
	fn criticality(type_name: &str) -> Criticality {
		match type_name {
			"device-hello" => Criticality::Exactly(&[]),
			_ => Criticality::Unconstrained,
		}
	}
}

impl ClientMessage {
	/// Serialise to JSON bytes.
	pub fn to_json(&self) -> Vec<u8> {
		super::envelope::write(self)
	}
}

impl DeviceMessage {
	/// Serialise to JSON bytes.
	pub fn to_json(&self) -> Vec<u8> {
		super::envelope::write(self)
	}
}

#[cfg(test)]
mod tests {
	use super::{
		super::{
			envelope::{Fault, Reading, Refusal, Skip, read, round_trip_omissions},
			readings::{Sample, Value},
		},
		*,
	};

	/// A reading standing in for whatever a device reports, for exercising the envelope around it.
	fn cpu() -> readings::Reading {
		readings::Reading::new("cpu", "CPU", Value::Fraction(0.12))
	}

	fn client(json: &str) -> Result<Reading<ClientMessage>, Fault> {
		read(json.as_bytes())
	}

	fn device(json: &str) -> Result<Reading<DeviceMessage>, Fault> {
		read(json.as_bytes())
	}

	#[test]
	fn client_hello_round_trips() {
		let message = ClientMessage::Hello {
			name: "bliti-web".to_owned(),
			version: "0.1.0".to_owned(),
		};
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message));
	}

	/// Member order is not significant in JSON and nothing reads it. These pin the names and values a
	/// peer will see; the order is whatever the writer's map yields, which is sorted.
	#[test]
	fn client_hello_json_shape_is_stable() {
		let json = ClientMessage::Hello {
			name: "bliti-web".to_owned(),
			version: "0.1.0".to_owned(),
		}
		.to_json();
		assert_eq!(
			String::from_utf8(json).unwrap(),
			r#"{"name":"bliti-web","type":"client-hello","version":"0.1.0"}"#
		);
	}

	#[test]
	fn subscribe_json_shape_is_stable() {
		let json = ClientMessage::Subscribe {
			topic: "system".to_owned(),
		}
		.to_json();
		assert_eq!(
			String::from_utf8(json).unwrap(),
			r#"{"TOPIC":"system","type":"subscribe"}"#
		);
	}

	#[test]
	fn device_hello_json_shape_is_stable() {
		let json = DeviceMessage::Hello {
			name: "bliti".to_owned(),
			version: "0.1.0".to_owned(),
		}
		.to_json();
		assert_eq!(
			String::from_utf8(json).unwrap(),
			r#"{"name":"bliti","type":"device-hello","version":"0.1.0"}"#
		);
	}

	#[test]
	fn the_system_types_round_trip() {
		for message in [
			DeviceMessage::SystemIdentity {
				readings: vec![readings::Reading::new(
					"hostname",
					"Hostname",
					Value::text("tamanu-iti"),
				)],
			},
			DeviceMessage::SystemSample {
				at: 20_308_140,
				readings: vec![cpu()],
			},
			DeviceMessage::SystemHistory {
				samples: vec![Sample {
					at: 20_306_140,
					readings: vec![cpu()],
				}],
			},
		] {
			let json = message.to_json();
			assert_eq!(read(&json).unwrap(), Reading::Message(message));
		}
	}

	#[test]
	fn a_mixed_case_name_is_a_fault() {
		assert_eq!(
			client(r#"{"type":"subscribe","Topic":"system"}"#).unwrap_err(),
			Fault::MalformedName("Topic".to_owned())
		);
	}

	#[test]
	fn the_same_name_twice_is_a_fault() {
		// Differing only in case.
		assert_eq!(
			client(r#"{"type":"subscribe","topic":"a","TOPIC":"b"}"#).unwrap_err(),
			Fault::DuplicateName("topic".to_owned())
		);
		// Exactly repeated, which a JSON parser would otherwise collapse to the last.
		assert_eq!(
			client(r#"{"type":"subscribe","topic":"a","topic":"b"}"#).unwrap_err(),
			Fault::DuplicateName("topic".to_owned())
		);
	}

	/// An ignorable member this build does not know is passed over, and the rest is read.
	#[test]
	fn an_unknown_ignorable_member_is_skipped() {
		assert_eq!(
			client(r#"{"type":"subscribe","TOPIC":"system","cadence":"fast"}"#).unwrap(),
			Reading::Message(ClientMessage::Subscribe {
				topic: "system".to_owned()
			})
		);
	}

	/// A type this build does not know is passed over whole.
	#[test]
	fn an_unknown_type_is_skipped() {
		assert_eq!(
			client(r#"{"type":"reboot","when":"now"}"#).unwrap(),
			Reading::Skipped(Skip::UnknownType("reboot".to_owned()))
		);
	}

	/// Unknown members are found by round-tripping through these types, so a member that serialises
	/// away would be read as one this build has never heard of. Checked rather than remembered.
	#[test]
	fn every_message_type_survives_the_round_trip() {
		let clients = [
			ClientMessage::Hello {
				name: "a".to_owned(),
				version: "1".to_owned(),
			},
			ClientMessage::Subscribe {
				topic: "system".to_owned(),
			},
		];
		for message in &clients {
			assert_eq!(
				round_trip_omissions(message),
				Vec::<String>::new(),
				"{message:?}"
			);
		}

		let devices = [
			DeviceMessage::Hello {
				name: "a".to_owned(),
				version: "1".to_owned(),
			},
			DeviceMessage::SystemIdentity {
				readings: vec![
					readings::Reading::new("hostname", "Hostname", Value::text("iti")),
					// Every optional member set, so one skipping its serialisation would show up.
					readings::Reading::new(
						"temperature",
						"Temperature",
						Value::scaled(48.5, "C", 110.0),
					)
					.with_detail("Disk", Value::quantity(37.8, "C"))
					.with_note("The processor core, not the case.")
					.with_state(readings::State::Warn)
					.with_limit(75.0, "Cooling")
					.in_group("thermal")
					.flowing(readings::Direction::Out),
					readings::Reading::failed("battery", "Battery", "no answer from the gauge"),
				],
			},
			DeviceMessage::SystemSample {
				at: 1,
				readings: vec![cpu()],
			},
			DeviceMessage::SystemHistory {
				samples: vec![Sample {
					at: 1,
					readings: vec![cpu()],
				}],
			},
		];
		for message in &devices {
			assert_eq!(
				round_trip_omissions(message),
				Vec::<String>::new(),
				"{message:?}"
			);
		}
	}

	/// A pinned type's members are pinned at every depth, not only at the top: BLI-MSG says those
	/// types carry no critical member beyond what it names, and says nothing about depth. A nested one
	/// is therefore a peer breaking the pin rather than a peer newer than this build.
	#[test]
	fn a_nested_critical_member_on_a_pinned_type_is_a_fault() {
		let json = r#"{"type":"client-hello","name":"a","version":"1","extra":{"NESTED":true}}"#;
		assert!(matches!(
			client(json).unwrap_err(),
			Fault::CriticalNotAllowed { .. }
		));
	}

	/// The hellos carry no critical member, so one arriving is a peer breaking the protocol rather
	/// than a peer newer than this build (BLI-MSG).
	#[test]
	fn a_critical_member_on_a_hello_is_a_fault() {
		assert!(matches!(
			client(r#"{"type":"client-hello","name":"a","version":"1","MODE":"strict"}"#)
				.unwrap_err(),
			Fault::CriticalNotAllowed { .. }
		));
		assert!(matches!(
			device(r#"{"type":"device-hello","name":"a","version":"1","CAPABILITY":"x"}"#)
				.unwrap_err(),
			Fault::CriticalNotAllowed { .. }
		));
	}

	/// `subscribe` pins its selector critical: a device must not act on a request to be sent something
	/// it has not read. Arriving plain is a fault, as is any other critical member alongside it.
	#[test]
	fn subscribe_pins_its_selector_critical() {
		assert!(matches!(
			client(r#"{"type":"subscribe","topic":"system"}"#).unwrap_err(),
			Fault::CriticalRequired { .. }
		));
		assert!(matches!(
			client(r#"{"type":"subscribe","TOPIC":"system","REDACT":["cpu"]}"#).unwrap_err(),
			Fault::CriticalNotAllowed { .. }
		));
		assert!(matches!(
			client(r#"{"TYPE":"subscribe","TOPIC":"system"}"#).unwrap_err(),
			Fault::CriticalNotAllowed { .. }
		));

		// And the shape a conforming client sends is read.
		assert_eq!(
			client(r#"{"type":"subscribe","TOPIC":"system"}"#).unwrap(),
			Reading::Message(ClientMessage::Subscribe {
				topic: "system".to_owned()
			})
		);
	}

	/// The prohibition is per type, not per build: a type a feature owns still takes the general rule.
	#[test]
	fn a_feature_type_still_refuses_rather_than_faults() {
		let json = r#"{"type":"system-identity","readings":[],"REDACT":["cpu"]}"#;
		assert_eq!(
			device(json).unwrap(),
			Reading::Refused(Refusal::CriticalMembers(vec!["redact".to_owned()]))
		);
	}

	/// An unknown type named by a critical `TYPE` is reported rather than passed over in silence.
	#[test]
	fn an_unknown_critical_type_is_refused() {
		assert_eq!(
			client(r#"{"TYPE":"wipe","confirm":true}"#).unwrap(),
			Reading::Refused(Refusal::CriticalType("wipe".to_owned()))
		);
	}

	/// Criticality holds at any depth, and refusing one nested object does not cost the rest of the
	/// message: only the path that carried it is named.
	#[test]
	fn a_nested_critical_member_is_found() {
		let json = r#"{"type":"system-identity","readings":[
			{"name":"cpu","label":"CPU","value":{"kind":"fraction","number":0.1}},
			{"name":"disk","label":"Disk","value":{"kind":"fraction","number":0.5},"SCOPE":"site"}
		]}"#;
		assert_eq!(
			device(json).unwrap(),
			Reading::Refused(Refusal::CriticalMembers(vec![
				"readings.1.scope".to_owned()
			]))
		);
	}

	/// A nested ignorable member is passed over, and the message is read.
	#[test]
	fn a_nested_ignorable_member_is_skipped() {
		let json = r#"{"type":"system-identity","readings":[
			{"name":"cpu","label":"CPU","value":{"kind":"fraction","number":0.1},"cores":4}
		]}"#;
		let Reading::Message(DeviceMessage::SystemIdentity { readings }) = device(json).unwrap()
		else {
			panic!("expected a system identity")
		};
		assert_eq!(readings.len(), 1);
	}

	#[test]
	fn a_known_type_missing_what_it_requires_is_a_fault() {
		let err = client(r#"{"type":"subscribe"}"#).unwrap_err();
		assert!(matches!(err, Fault::Malformed { .. }), "got {err:?}");
	}

	#[test]
	fn a_member_of_the_wrong_json_type_is_a_fault() {
		let err = client(r#"{"type":"subscribe","TOPIC":42}"#).unwrap_err();
		assert!(matches!(err, Fault::Malformed { .. }), "got {err:?}");
	}

	#[test]
	fn malformed_json_is_a_fault() {
		assert!(matches!(
			client("this is not json").unwrap_err(),
			Fault::NotJson(_)
		));
		assert_eq!(client("[]").unwrap_err(), Fault::NotObject);
		assert_eq!(client(r#""a string""#).unwrap_err(), Fault::NotObject);
	}

	#[test]
	fn a_message_without_a_string_type_is_a_fault() {
		assert_eq!(client(r#"{"topic":"system"}"#).unwrap_err(), Fault::NoType);
		assert_eq!(
			client(r#"{"type":42,"topic":"system"}"#).unwrap_err(),
			Fault::TypeNotString
		);
	}

	#[test]
	fn bytes_that_are_not_utf8_are_a_fault() {
		assert_eq!(
			read::<ClientMessage>(&[0xff, 0xfe]).unwrap_err(),
			Fault::NotUtf8
		);
	}

	#[test]
	fn a_message_beyond_the_ceiling_is_a_fault() {
		let huge = vec![b'a'; super::super::envelope::MAX_MESSAGE + 1];
		assert!(matches!(
			read::<ClientMessage>(&huge).unwrap_err(),
			Fault::TooLarge { .. }
		));
	}
}
