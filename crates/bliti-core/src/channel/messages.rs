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

use super::envelope::Message;

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

	/// The device's hostname and network addresses, sent on connect and again whenever they change.
	#[serde(rename = "identity")]
	Identity {
		/// The device's hostname.
		hostname: String,
		/// Every global address the device has, each with the interface it belongs to. Loopback and
		/// link-local addresses are left out; the client decides what is worth showing.
		addresses: Vec<Address>,
	},
}

impl Message for ClientMessage {
	fn knows(type_name: &str) -> bool {
		matches!(type_name, "client-hello" | "subscribe")
	}
}

impl Message for DeviceMessage {
	fn knows(type_name: &str) -> bool {
		matches!(type_name, "device-hello" | "identity")
	}
}

/// One network address the device holds, with the interface it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
	/// The address in its textual form.
	pub address: String,
	/// The interface the address belongs to.
	pub interface: String,
	/// The address family.
	pub family: AddressFamily,
}

/// The family of a network address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AddressFamily {
	/// An IPv4 address.
	Ipv4,
	/// An IPv6 address.
	Ipv6,
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
		super::envelope::{Fault, Reading, Refusal, Skip, read},
		*,
	};

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

	#[test]
	fn client_hello_json_shape_is_stable() {
		let json = ClientMessage::Hello {
			name: "bliti-web".to_owned(),
			version: "0.1.0".to_owned(),
		}
		.to_json();
		assert_eq!(
			String::from_utf8(json).unwrap(),
			r#"{"type":"client-hello","name":"bliti-web","version":"0.1.0"}"#
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
			r#"{"type":"subscribe","topic":"system"}"#
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
			r#"{"type":"device-hello","name":"bliti","version":"0.1.0"}"#
		);
	}

	#[test]
	fn device_identity_round_trips() {
		let message = DeviceMessage::Identity {
			hostname: "tamanu-iti".to_owned(),
			addresses: vec![Address {
				address: "192.0.2.10".to_owned(),
				interface: "eth0".to_owned(),
				family: AddressFamily::Ipv4,
			}],
		};
		let json = message.to_json();
		assert_eq!(read(&json).unwrap(), Reading::Message(message));
	}

	/// Names are matched without regard to case, so a member arriving upper case reaches the same
	/// member and is handled identically (BLI-MSG, "Member names").
	#[test]
	fn a_recognised_member_is_read_whichever_case_it_arrives_in() {
		let lower = client(r#"{"type":"subscribe","topic":"system"}"#).unwrap();
		let upper = client(r#"{"TYPE":"subscribe","TOPIC":"system"}"#).unwrap();
		assert_eq!(lower, upper);
		assert_eq!(
			lower,
			Reading::Message(ClientMessage::Subscribe {
				topic: "system".to_owned()
			})
		);
	}

	/// Casing marks names, never values.
	#[test]
	fn casing_does_not_reach_values() {
		let Reading::Message(ClientMessage::Subscribe { topic }) =
			client(r#"{"type":"subscribe","topic":"SYSTEM"}"#).unwrap()
		else {
			panic!("expected a subscribe")
		};
		assert_eq!(topic, "SYSTEM", "a topic is a value and keeps its case");
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
			client(r#"{"type":"subscribe","topic":"system","cadence":"fast"}"#).unwrap(),
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

	/// A critical member this build does not know means the message is not acted on. This is a peer
	/// newer than us, not a broken one, so it is refused rather than faulted.
	#[test]
	fn an_unknown_critical_member_is_refused() {
		assert_eq!(
			client(r#"{"type":"subscribe","topic":"system","REDACT":["cpu"]}"#).unwrap(),
			Reading::Refused(Refusal::CriticalMembers(vec!["redact".to_owned()]))
		);
	}

	/// A critical member the build does know is acted on like any other: criticality bites only where
	/// a member is not recognised.
	#[test]
	fn a_known_member_marked_critical_is_not_refused() {
		assert_eq!(
			client(r#"{"TOPIC":"system","type":"subscribe"}"#).unwrap(),
			Reading::Message(ClientMessage::Subscribe {
				topic: "system".to_owned()
			})
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
		let json = r#"{"type":"identity","hostname":"iti","addresses":[
			{"address":"192.0.2.10","interface":"eth0","family":"ipv4"},
			{"address":"192.0.2.11","interface":"eth1","family":"ipv4","SCOPE":"site"}
		]}"#;
		assert_eq!(
			device(json).unwrap(),
			Reading::Refused(Refusal::CriticalMembers(vec![
				"addresses.1.scope".to_owned()
			]))
		);
	}

	/// A nested ignorable member is passed over, and the message is read.
	#[test]
	fn a_nested_ignorable_member_is_skipped() {
		let json = r#"{"type":"identity","hostname":"iti","addresses":[
			{"address":"192.0.2.10","interface":"eth0","family":"ipv4","scope":"site"}
		]}"#;
		let Reading::Message(DeviceMessage::Identity { addresses, .. }) = device(json).unwrap()
		else {
			panic!("expected an identity")
		};
		assert_eq!(addresses.len(), 1);
	}

	#[test]
	fn a_known_type_missing_what_it_requires_is_a_fault() {
		let err = client(r#"{"type":"subscribe"}"#).unwrap_err();
		assert!(matches!(err, Fault::Malformed { .. }), "got {err:?}");
	}

	#[test]
	fn a_member_of_the_wrong_json_type_is_a_fault() {
		let err = client(r#"{"type":"subscribe","topic":42}"#).unwrap_err();
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
