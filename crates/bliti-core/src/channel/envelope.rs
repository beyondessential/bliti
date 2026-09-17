//! The application message envelope: how a message is shaped, named, and read.
//!
//! Behaviour is specified in MSG. A message is a JSON object carrying a `type` member, and the
//! case of a member's name says what a receiver that does not recognise it must do: an upper case
//! name is critical and the object carrying it must not be acted on, a lower case name is ignorable.
//!
//! Reading a message yields one of three outcomes, and keeping them apart is the point of this
//! module. A [`Fault`] is a peer that is not speaking the protocol and closes the stream it arrived
//! on. A [`Reading::Refused`] is a peer newer than this one saying something that must not be half
//! read, which leaves the stream alone. A [`Reading::Skipped`] is a peer newer than this one saying
//! something safe to pass over. Collapsing any two of these loses the property the envelope exists
//! for.
//!
//! Unknown members are found by round-tripping: the normalised input is deserialised into the
//! message type and serialised back, and whatever the input had that the round trip does not is a
//! member this build has never heard of. That works at any depth with no per-type bookkeeping, and
//! it is why message types must not skip serialising their own members.

use std::{collections::BTreeSet, fmt};

use serde::{
	Deserialize, Serialize,
	de::{self, DeserializeOwned, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value};

/// The largest application message, as a count of JSON bytes.
///
/// Sized for the link rather than for JSON. A mebibyte is nothing to a socket and far too much for
/// BLE: a message that size is thousands of notifications, and one was enough to drown a connection
/// before anything else could be said. This leaves room for anything a feature has reason to send in
/// one piece while keeping the worst case to a second or two on the air.
pub const MAX_MESSAGE: usize = 128 * 1024;

/// A set of message types that can be read from the wire.
pub trait Message: DeserializeOwned + Serialize {
	/// Whether this build knows the named message type.
	///
	/// Read before deserialising, so that a type this build has never heard of is told apart from one
	/// it knows but cannot parse: the first is a newer peer, the second a broken one.
	fn knows(type_name: &str) -> bool;

	/// What MSG requires of criticality on the named message type.
	///
	/// The types that spec defines are pinned, so that the messages opening a conversation never put a
	/// receiver in the position of weighing criticality, and so that a request to be sent something
	/// cannot be half read. A message breaking the pin is a fault rather than a refusal: a conforming
	/// peer cannot produce one. A type a feature defines is unconstrained unless its own spec says
	/// otherwise.
	fn criticality(_type_name: &str) -> Criticality {
		Criticality::Unconstrained
	}

	/// The members this build writes as critical on the named type.
	///
	/// Separate from [`Message::criticality`], which is what MSG pins for the types it defines.
	/// This is the growth rule's sending half: a feature adding a member its message does not mean
	/// anything without names it here, and an older peer refuses the message rather than acting on a
	/// reading the sender has said is incomplete. A pinned type's two must agree.
	fn critical_members(_type_name: &str) -> &'static [&'static str] {
		&[]
	}
}

/// Whether a message survives the round trip the unknown-member detection in [`read`] depends on.
///
/// That detection works by serialising a parsed message back and treating whatever the input had
/// that the round trip does not as a member this build has never heard of. A type that skips
/// serialising one of its own members therefore makes that member look unknown, and if it arrives
/// critical the message is refused. The failure is silent and hard to trace to the serde attribute
/// that caused it, so a feature adding a message type should assert this over an example of each.
///
/// Returns the paths of members that did not survive, empty where the type is sound.
pub fn round_trip_omissions<T: Message>(message: &T) -> Vec<String> {
	let Ok(written) = serde_json::to_value(message) else {
		return vec!["<does not serialise>".to_owned()];
	};
	let Ok(parsed) = serde_json::from_value::<T>(written.clone()) else {
		return vec!["<does not round trip>".to_owned()];
	};
	let Ok(again) = serde_json::to_value(&parsed) else {
		return vec!["<does not serialise>".to_owned()];
	};
	let mut lost = BTreeSet::new();
	collect_unknown(&written, &again, &mut String::new(), &mut lost);
	lost.into_iter().collect()
}

/// What MSG requires of criticality on a message type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criticality {
	/// Nothing beyond the general rules, as for a type a feature owns.
	Unconstrained,
	/// Exactly these members are critical: each must arrive critical where it arrives at all, and no
	/// other member of the message may be.
	Exactly(&'static [&'static str]),
}

/// What reading one message yielded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading<T> {
	/// Understood, and to be acted on.
	Message(T),
	/// Not recognised, and safe to pass over. The stream carries on and nothing is reported.
	Skipped(Skip),
	/// Carries something critical this build does not know, so it is not acted on. The stream carries
	/// on: this is a peer newer than us, not a broken one.
	Refused(Refusal),
}

/// Why a message was passed over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
	/// The `type` named a message type this build does not know, and was not marked critical.
	UnknownType(String),
}

impl fmt::Display for Skip {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UnknownType(name) => {
				write!(f, "message type {name:?} is not known to this build")
			}
		}
	}
}

/// Why a message was not acted on, despite the peer speaking the protocol correctly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
	/// A critical `TYPE` naming a message type this build does not know.
	CriticalType(String),
	/// Members marked critical that this build does not know, named by their path within the message.
	CriticalMembers(Vec<String>),
}

impl fmt::Display for Refusal {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::CriticalType(name) => write!(
				f,
				"message type {name:?} is marked critical and is not known to this build"
			),
			Self::CriticalMembers(names) => write!(
				f,
				"critical members not known to this build: {}",
				names.join(", ")
			),
		}
	}
}

/// A peer that is not speaking the protocol.
///
/// None of these is a version difference, because a version difference cannot produce one. Each is
/// reported and closes the stream it arrived on, leaving the connection and every other stream alive.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fault {
	/// Beyond the size ceiling, refused rather than buffered.
	#[error("message of {size} bytes exceeds the {max}-byte maximum")]
	TooLarge {
		/// The size the message claimed.
		size: usize,
		/// The largest message that will be read.
		max: usize,
	},

	/// Not valid UTF-8.
	#[error("message is not valid UTF-8")]
	NotUtf8,

	/// Not valid JSON.
	#[error("message is not valid JSON: {0}")]
	NotJson(String),

	/// Valid JSON, but not an object.
	#[error("message is not a JSON object")]
	NotObject,

	/// A member name neither wholly lower case nor wholly upper case, or carrying something other
	/// than letters, digits and hyphens.
	#[error("member name {0:?} is malformed")]
	MalformedName(String),

	/// The same member named twice, whatever the case of each.
	#[error("member {0:?} appears more than once")]
	DuplicateName(String),

	/// No `type` member at all.
	#[error("message has no `type` member")]
	NoType,

	/// A `type` member that is not a string.
	#[error("`type` is not a string")]
	TypeNotString,

	/// A critical member on a message type that MSG forbids carrying one.
	#[error("{type_name} may not carry a critical member, but carried {names}")]
	CriticalNotAllowed {
		/// The message type that carried it.
		type_name: String,
		/// The critical members it carried, by their path within the message.
		names: String,
	},

	/// A member that must be critical on this message type arrived without being marked so.
	#[error("{type_name} requires {names} to be critical, but it was not")]
	CriticalRequired {
		/// The message type that carried it.
		type_name: String,
		/// The members that had to be critical and were not.
		names: String,
	},

	/// A known message type that does not carry what it requires.
	#[error("{type_name} message is malformed: {detail}")]
	Malformed {
		/// The message type that failed to parse.
		type_name: String,
		/// What was wrong with it.
		detail: String,
	},
}

/// Read one application message.
///
/// `bytes` is one length-delimited message off a stream, without its length prefix.
pub fn read<T: Message>(bytes: &[u8]) -> Result<Reading<T>, Fault> {
	if bytes.len() > MAX_MESSAGE {
		return Err(Fault::TooLarge {
			size: bytes.len(),
			max: MAX_MESSAGE,
		});
	}

	let text = std::str::from_utf8(bytes).map_err(|_| Fault::NotUtf8)?;
	let raw: Raw = serde_json::from_str(text).map_err(|err| Fault::NotJson(err.to_string()))?;

	let mut critical = BTreeSet::new();
	let value = normalise(raw, &mut String::new(), &mut critical)?;
	let Value::Object(object) = value else {
		return Err(Fault::NotObject);
	};

	let type_name = match object.get("type") {
		None => return Err(Fault::NoType),
		Some(Value::String(name)) => name.clone(),
		Some(_) => return Err(Fault::TypeNotString),
	};

	if !T::knows(&type_name) {
		// An unknown type marked critical is one the sender has said must not be passed over.
		return Ok(if critical.contains("type") {
			Reading::Refused(Refusal::CriticalType(type_name))
		} else {
			Reading::Skipped(Skip::UnknownType(type_name))
		});
	}

	// A type whose criticality MSG pins, carrying something other than what it pins, is a peer
	// breaking the protocol rather than a peer newer than this build.
	if let Criticality::Exactly(required) = T::criticality(&type_name) {
		let surplus: Vec<&str> = critical
			.iter()
			.map(String::as_str)
			.filter(|name| !required.contains(name))
			.collect();
		if !surplus.is_empty() {
			return Err(Fault::CriticalNotAllowed {
				type_name,
				names: surplus.join(", "),
			});
		}

		// A member absent altogether is left to the parse below, which says it is missing rather than
		// that it is miscased.
		let plain: Vec<&str> = required
			.iter()
			.copied()
			.filter(|name| object.contains_key(*name) && !critical.contains(*name))
			.collect();
		if !plain.is_empty() {
			return Err(Fault::CriticalRequired {
				type_name,
				names: plain.join(", "),
			});
		}
	}

	let normalised = Value::Object(object);
	let message: T =
		serde_json::from_value(normalised.clone()).map_err(|err| Fault::Malformed {
			type_name: type_name.clone(),
			detail: err.to_string(),
		})?;

	// Whatever the input carried that the round trip does not is a member this build has never heard
	// of. Only the critical ones stop the message being acted on.
	let mut unknown = BTreeSet::new();
	let round = serde_json::to_value(&message).map_err(|err| Fault::Malformed {
		type_name,
		detail: err.to_string(),
	})?;
	collect_unknown(&normalised, &round, &mut String::new(), &mut unknown);

	let refused: Vec<String> = critical.intersection(&unknown).cloned().collect();
	if refused.is_empty() {
		Ok(Reading::Message(message))
	} else {
		Ok(Reading::Refused(Refusal::CriticalMembers(refused)))
	}
}

/// Write a message as the JSON bytes that go on a stream.
///
/// Casing is applied here rather than in each message type's own declaration, because the convention
/// belongs to the envelope: a type says what it carries, and this says how a member is named on the
/// wire. Members the type names in [`Message::critical_members`] go out in upper case.
pub fn write<T: Message>(message: &T) -> Vec<u8> {
	let mut value = serde_json::to_value(message).expect("a message serialises");
	let type_name = value
		.get("type")
		.and_then(Value::as_str)
		.map(str::to_owned)
		.unwrap_or_default();
	if let Value::Object(object) = &mut value {
		for name in T::critical_members(&type_name) {
			if let Some(member) = object.remove(*name) {
				object.insert(name.to_ascii_uppercase(), member);
			}
		}
	}
	serde_json::to_vec(&value).expect("a message serialises")
}

/// Whether a member name is well formed, and whether it is critical.
///
/// A name is letters, digits and hyphens, wholly lower case or wholly upper case. Digits and hyphens
/// carry no case, so a name of only those is not critical.
fn classify(name: &str) -> Option<bool> {
	if name.is_empty() {
		return None;
	}
	let ok = |c: char| c.is_ascii_alphanumeric() || c == '-';
	if !name.chars().all(ok) {
		return None;
	}
	let has_lower = name.chars().any(|c| c.is_ascii_lowercase());
	let has_upper = name.chars().any(|c| c.is_ascii_uppercase());
	match (has_lower, has_upper) {
		(true, true) => None,
		(_, upper) => Some(upper),
	}
}

/// Validate every member name, lower case it, and record the paths of those marked critical.
fn normalise(raw: Raw, path: &mut String, critical: &mut BTreeSet<String>) -> Result<Value, Fault> {
	match raw {
		Raw::Object(entries) => {
			let mut seen = BTreeSet::new();
			let mut object = Map::new();
			for (name, value) in entries {
				let Some(is_critical) = classify(&name) else {
					return Err(Fault::MalformedName(name));
				};
				let lowered = name.to_ascii_lowercase();
				if !seen.insert(lowered.clone()) {
					return Err(Fault::DuplicateName(lowered));
				}

				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(&lowered);
				if is_critical {
					critical.insert(path.clone());
				}
				let value = normalise(value, path, critical)?;
				path.truncate(restore);

				object.insert(lowered, value);
			}
			Ok(Value::Object(object))
		}
		Raw::Array(items) => {
			let mut out = Vec::with_capacity(items.len());
			for (index, item) in items.into_iter().enumerate() {
				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(&index.to_string());
				out.push(normalise(item, path, critical)?);
				path.truncate(restore);
			}
			Ok(Value::Array(out))
		}
		Raw::Other(value) => Ok(value),
	}
}

/// Collect the paths of members the input carried that the round trip does not.
fn collect_unknown(input: &Value, round: &Value, path: &mut String, out: &mut BTreeSet<String>) {
	match (input, round) {
		(Value::Object(input), Value::Object(round)) => {
			for (name, value) in input {
				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(name);
				match round.get(name) {
					None => {
						out.insert(path.clone());
					}
					Some(round) => collect_unknown(value, round, path, out),
				}
				path.truncate(restore);
			}
		}
		(Value::Array(input), Value::Array(round)) => {
			for (index, value) in input.iter().enumerate() {
				let Some(round) = round.get(index) else {
					continue;
				};
				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(&index.to_string());
				collect_unknown(value, round, path, out);
				path.truncate(restore);
			}
		}
		_ => {}
	}
}

/// JSON as it arrived, keeping every member in the order and the casing it was written in.
///
/// [`serde_json::Value`] cannot be used for this: its map collapses a name repeated in one object,
/// taking the last, so a message carrying the same name twice would be read as valid.
#[derive(Debug)]
enum Raw {
	Object(Vec<(String, Raw)>),
	Array(Vec<Raw>),
	Other(Value),
}

impl<'de> Deserialize<'de> for Raw {
	fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		deserializer.deserialize_any(RawVisitor)
	}
}

struct RawVisitor;

impl<'de> Visitor<'de> for RawVisitor {
	type Value = Raw;

	fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("any JSON value")
	}

	fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<Raw, A::Error> {
		let mut entries = Vec::new();
		while let Some((name, value)) = access.next_entry::<String, Raw>()? {
			entries.push((name, value));
		}
		Ok(Raw::Object(entries))
	}

	fn visit_seq<A: SeqAccess<'de>>(self, mut access: A) -> Result<Raw, A::Error> {
		let mut items = Vec::new();
		while let Some(item) = access.next_element::<Raw>()? {
			items.push(item);
		}
		Ok(Raw::Array(items))
	}

	fn visit_bool<E: de::Error>(self, v: bool) -> Result<Raw, E> {
		Ok(Raw::Other(Value::Bool(v)))
	}

	fn visit_i64<E: de::Error>(self, v: i64) -> Result<Raw, E> {
		Ok(Raw::Other(Value::from(v)))
	}

	fn visit_u64<E: de::Error>(self, v: u64) -> Result<Raw, E> {
		Ok(Raw::Other(Value::from(v)))
	}

	fn visit_f64<E: de::Error>(self, v: f64) -> Result<Raw, E> {
		Ok(Raw::Other(Value::from(v)))
	}

	fn visit_str<E: de::Error>(self, v: &str) -> Result<Raw, E> {
		Ok(Raw::Other(Value::String(v.to_owned())))
	}

	fn visit_unit<E: de::Error>(self) -> Result<Raw, E> {
		Ok(Raw::Other(Value::Null))
	}

	fn visit_none<E: de::Error>(self) -> Result<Raw, E> {
		Ok(Raw::Other(Value::Null))
	}
}

#[cfg(test)]
mod tests {
	use serde::{Deserialize, Serialize};

	use super::*;

	/// A message type standing in for one a feature defines later.
	///
	/// It is needed because every type MSG itself defines forbids a critical member, so none of
	/// them can carry one legally and none can serve as the vehicle for the general rules below.
	#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
	#[serde(tag = "type")]
	enum Sample {
		#[serde(rename = "sample")]
		Sample { topic: String },
	}

	impl Message for Sample {
		fn knows(type_name: &str) -> bool {
			type_name == "sample"
		}
	}

	fn sample(json: &str) -> Result<Reading<Sample>, Fault> {
		read(json.as_bytes())
	}

	/// Names are matched without regard to case, so a member arriving upper case reaches the same
	/// member and is handled identically.
	#[test]
	fn a_recognised_member_is_read_whichever_case_it_arrives_in() {
		let lower = sample(r#"{"type":"sample","topic":"system"}"#).unwrap();
		let upper = sample(r#"{"TYPE":"sample","TOPIC":"system"}"#).unwrap();
		assert_eq!(lower, upper);
		assert_eq!(
			lower,
			Reading::Message(Sample::Sample {
				topic: "system".to_owned()
			})
		);
	}

	/// Criticality bites only where a member is not recognised.
	#[test]
	fn a_known_member_marked_critical_is_not_refused() {
		assert_eq!(
			sample(r#"{"TOPIC":"system","type":"sample"}"#).unwrap(),
			Reading::Message(Sample::Sample {
				topic: "system".to_owned()
			})
		);
	}

	/// The case that separates a newer peer from a broken one.
	#[test]
	fn an_unknown_critical_member_is_refused() {
		assert_eq!(
			sample(r#"{"type":"sample","topic":"system","REDACT":["cpu"]}"#).unwrap(),
			Reading::Refused(Refusal::CriticalMembers(vec!["redact".to_owned()]))
		);
	}

	/// Casing marks names, never values.
	#[test]
	fn casing_does_not_reach_values() {
		let Reading::Message(Sample::Sample { topic }) =
			sample(r#"{"type":"sample","topic":"SYSTEM"}"#).unwrap()
		else {
			panic!("expected a sample")
		};
		assert_eq!(topic, "SYSTEM", "a topic is a value and keeps its case");
	}

	/// A digit or a hyphen carries no case, so a name of only those is not critical.
	#[test]
	fn a_caseless_name_is_not_critical() {
		assert_eq!(
			sample(r#"{"type":"sample","topic":"system","0-1":true}"#).unwrap(),
			Reading::Message(Sample::Sample {
				topic: "system".to_owned()
			})
		);
	}
}
