//! The application messages carried over the channel, as JSON.
//!
//! Behaviour is specified in MSG. The envelope they ride in, and the three outcomes of reading
//! one, live in [`super::envelope`]; this module carries the message types themselves.
//!
//! There is one message set and both ends send and receive from it: a message's direction comes from
//! which end opened the stream it arrived on, never from the type. An end that receives a type it
//! knows but has nothing to do about treats it as a no-op (MSG).
//!
//! No message type skips serialising a member it holds. Unknown members are found by round-tripping
//! through these types, so a member that serialises away would be read as one this build has never
//! heard of.

use serde::{
	Deserialize, Deserializer, Serialize, Serializer,
	de::{self, MapAccess, Visitor},
};
use serde_json::{Map, Value as Json};
use std::fmt;

use super::{
	envelope::{Criticality, MessageSet},
	readings::Entry,
};

/// One application message. Both ends speak this set.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
	/// An end naming itself, first on its hello stream. Direction is established by which end sent it,
	/// so there is one `hello` rather than a client one and a device one.
	Hello {
		/// What the software calls itself. Opaque to the peer, which displays or logs it.
		name: String,
		/// The version it is at. Opaque to the peer.
		version: String,
	},

	/// A request for a topic, first on a stream opened for the purpose. Closing that stream is the
	/// unsubscribe. Its selector is critical, so a peer cannot act on a request to be sent something
	/// it has not read (MSG).
	Subscribe {
		/// The topic being subscribed to. Topics are defined by the feature that owns them.
		topic: String,
	},

	/// Something true about the device, named against the fact catalogue (NFO).
	Fact(Entry),

	/// A measurement whose history is worth keeping, named against the reading catalogue (NFO).
	Reading(Entry),
}

impl Message {
	/// Serialise to JSON bytes, applying the member casing the envelope owns.
	pub fn to_json(&self) -> Vec<u8> {
		super::envelope::write(self)
	}

	/// This message as a JSON map, with its `type` member. The casing of a critical member is applied
	/// by the envelope on write, not here.
	fn to_map(&self) -> Map<String, Json> {
		let mut map = Map::new();
		match self {
			Self::Hello { name, version } => {
				map.insert("type".to_owned(), "hello".into());
				map.insert("name".to_owned(), name.clone().into());
				map.insert("version".to_owned(), version.clone().into());
			}
			Self::Subscribe { topic } => {
				map.insert("type".to_owned(), "subscribe".into());
				map.insert("topic".to_owned(), topic.clone().into());
			}
			Self::Fact(entry) => {
				map.insert("type".to_owned(), "fact".into());
				entry.write_into("fact", &mut map);
			}
			Self::Reading(entry) => {
				map.insert("type".to_owned(), "reading".into());
				entry.write_into("measurement", &mut map);
			}
		}
		map
	}
}

impl Serialize for Message {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		self.to_map().serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for Message {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer.deserialize_map(MessageVisitor)
	}
}

struct MessageVisitor;

impl<'de> Visitor<'de> for MessageVisitor {
	type Value = Message;

	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("a bliti application message")
	}

	fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Message, A::Error> {
		let mut map = Map::new();
		while let Some((name, value)) = access.next_entry::<String, Json>()? {
			map.insert(name, value);
		}
		let type_name = map
			.get("type")
			.and_then(Json::as_str)
			.ok_or_else(|| de::Error::custom("a message carries a string `type`"))?;

		match type_name {
			"hello" => Ok(Message::Hello {
				name: string(&map, "name")?,
				version: string(&map, "version")?,
			}),
			"subscribe" => Ok(Message::Subscribe {
				topic: string(&map, "topic")?,
			}),
			"fact" => Entry::read_from("fact", &map)
				.map(Message::Fact)
				.map_err(de::Error::custom),
			"reading" => Entry::read_from("measurement", &map)
				.map(Message::Reading)
				.map_err(de::Error::custom),
			other => Err(de::Error::custom(format!("unknown message type {other:?}"))),
		}
	}
}

fn string<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<String, E> {
	map.get(member)
		.and_then(Json::as_str)
		.map(ToOwned::to_owned)
		.ok_or_else(|| de::Error::custom(format!("this message carries a string `{member}`")))
}

impl MessageSet for Message {
	fn known_types() -> &'static [&'static str] {
		&["hello", "subscribe", "fact", "reading"]
	}

	/// `hello` carries no critical member; `subscribe` carries exactly one, its selector. The feature
	/// types NFO owns are unconstrained.
	fn criticality(type_name: &str) -> Criticality {
		match type_name {
			"hello" => Criticality::Exactly(&[]),
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

#[cfg(test)]
mod tests {
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
		];
		for message in &messages {
			assert_eq!(
				round_trip_omissions(message),
				Vec::<String>::new(),
				"{message:?}"
			);
		}
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
}
