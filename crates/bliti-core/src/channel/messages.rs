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

	/// A client opening a configuration session, first on the stream it opens for it (CFG).
	Configure,

	/// A device's answer to `configure` and to `confirm`, and a client's proposal in between: the
	/// network configuration document, and — on the first a device sends in a session — the
	/// capabilities it supports. Its `document` is critical, so an end that cannot read the document
	/// does not act on the message carrying it (CFG).
	Configuration {
		/// The configuration document (NET), kept raw so a member a newer peer added survives the
		/// envelope round trip and is echoed back unchanged, exactly as [`Entry`] keeps its traits.
		document: Map<String, Json>,
		/// What the device supports, on the first `configuration` a device sends. Kept raw for the same
		/// reason, and because the layer that probes capabilities owns their shape.
		capabilities: Option<Map<String, Json>>,
	},

	/// A device's answer that a proposal was applied to the running system (CFG).
	Applied {
		/// The device's capabilities, where applying the proposal changed them.
		capabilities: Option<Map<String, Json>>,
	},

	/// The state of each candidate of the configuration running, position for position (CFG).
	State {
		/// One entry per attachment, kept raw as the document is.
		attachments: Vec<Json>,
		/// The device's capabilities, where returning to the recorded configuration changed them.
		capabilities: Option<Map<String, Json>>,
	},

	/// The PIN a device joining by WPS generated, for the operator to enter at the access point (CFG).
	Pin {
		/// The PIN's digits.
		pin: String,
	},

	/// A device's answer that a proposal cannot be accepted, naming the part at fault, the device's own
	/// reason, and the verification stage an apply-time failure reached (CFG).
	Invalid {
		/// Which part is at fault, as an RFC 9535 Normalized Path: into the document for a proposal,
		/// into the act's own message for an act.
		at: String,
		/// What happened, in the device's own words.
		reason: String,
		/// The verification stage of LINK a proposal applied and then failed stopped at.
		reached: Option<String>,
	},

	/// A client making its proposal durable (CFG).
	Confirm,

	/// A client abandoning its proposal (CFG).
	Discard,

	/// A device's answer that a configuration session is already open (CFG).
	Busy,

	/// A client asking a device to report the wireless networks it can see (CFG).
	Scan {
		/// The one wireless interface to scan on; every one able to where absent.
		interface: Option<String>,
	},

	/// A client asking a device to report what its radios can see of the spectrum (CFG).
	Survey {
		/// The one wireless interface to survey on; every one able to where absent.
		interface: Option<String>,
	},

	/// A client asking a device to join a wireless network by WPS (CFG, WLAN).
	Wps {
		/// The WPS method: `push-button` or `pin`.
		method: String,
		/// The wireless interface to join on; the device chooses where absent.
		interface: Option<String>,
		/// The one network whose credentials the device may accept; any where absent.
		ssid: Option<String>,
	},

	/// A device's answer to `scan`: one entry per access point each radio scanned heard (CFG), kept
	/// raw so a member a newer device adds survives.
	Networks {
		/// The access points heard, as the `access-points` member.
		access_points: Vec<Json>,
	},

	/// A device's answer to `survey`. Its shape is the device-scanning feature's to define, so it rides
	/// as raw JSON.
	Spectrum {
		/// What the radio can see of the spectrum.
		spectrum: Map<String, Json>,
	},

	/// A client opening a power stream, first on the stream it opens for it (CTL).
	Power,

	/// A device's answer to `power`: the acts it can carry out (CTL). Named as strings rather than
	/// as a closed set, so a client passes over an act a newer device lists rather than failing to
	/// read the message.
	Acts {
		/// The acts, by wire name.
		acts: Vec<String>,
	},

	/// A client asking a device to carry out an act (CTL). Its selector is critical, so a device does
	/// not act on a request whose selector it has not read.
	Act {
		/// The act, by wire name.
		act: String,
	},

	/// A device's answer that it will carry out the act asked for (CTL), or has done the `load` or
	/// `reset` asked for (CRV).
	Accepted,

	/// A device's answer that it will not carry out the act asked for (CTL), or has not done the
	/// `load` or `reset` asked for (CRV).
	Refused {
		/// Why, in the device's own words.
		reason: String,
	},

	/// A device telling a client on its `default` feed that it is about to carry out an act (CTL).
	GoingAway {
		/// The act accepted, by wire name.
		act: String,
		/// Why the device is going away, by wire name. Open, as `act` is: a client reads a cause it
		/// does not know by the act alone.
		cause: String,
	},

	/// A client opening a curve stream, first on the stream it opens for it (CRV).
	Curve,

	/// A device's answer to `curve`, and what it sends on every curve stream when its curve document
	/// changes (CRV).
	Curves {
		/// The curve document in force, kept raw: the device validates a document, not the envelope.
		/// Absent where the device manages no backup supply.
		document: Option<Json>,
		/// How long a full charge lasts, where the device manages a backup supply.
		lasts: Option<Span>,
		/// How long a full recharge takes, where the device also holds a charging curve.
		recharge: Option<Span>,
	},

	/// A client asking a device to load a curve document (CRV). Kept raw, so a document the device
	/// finds wrong is refused with a reason rather than faulting the stream.
	Load {
		/// The curve document.
		document: Json,
	},

	/// A client asking a device to return to the curve its build carries (CRV).
	Reset,
}

/// A time, and how far either way it may be off, both in seconds (CRV, "The curve stream").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
	/// The time.
	pub duration: f64,
	/// How far either way the time may be off.
	pub margin: f64,
}

impl Span {
	fn to_json(self) -> Json {
		let mut map = Map::new();
		map.insert("duration".to_owned(), number(self.duration));
		map.insert("margin".to_owned(), number(self.margin));
		Json::Object(map)
	}
}

/// A JSON number, falling back to null for a value JSON cannot carry.
fn number(value: f64) -> Json {
	serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number)
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
			Self::Configure => {
				map.insert("type".to_owned(), "configure".into());
			}
			Self::Configuration {
				document,
				capabilities,
			} => {
				map.insert("type".to_owned(), "configuration".into());
				map.insert("document".to_owned(), Json::Object(document.clone()));
				if let Some(capabilities) = capabilities {
					map.insert(
						"capabilities".to_owned(),
						Json::Object(capabilities.clone()),
					);
				}
			}
			Self::Applied { capabilities } => {
				map.insert("type".to_owned(), "applied".into());
				if let Some(capabilities) = capabilities {
					map.insert(
						"capabilities".to_owned(),
						Json::Object(capabilities.clone()),
					);
				}
			}
			Self::State {
				attachments,
				capabilities,
			} => {
				map.insert("type".to_owned(), "state".into());
				map.insert("attachments".to_owned(), Json::Array(attachments.clone()));
				if let Some(capabilities) = capabilities {
					map.insert(
						"capabilities".to_owned(),
						Json::Object(capabilities.clone()),
					);
				}
			}
			Self::Pin { pin } => {
				map.insert("type".to_owned(), "pin".into());
				map.insert("pin".to_owned(), pin.clone().into());
			}
			Self::Invalid {
				at,
				reason,
				reached,
			} => {
				map.insert("type".to_owned(), "invalid".into());
				map.insert("at".to_owned(), at.clone().into());
				map.insert("reason".to_owned(), reason.clone().into());
				if let Some(reached) = reached {
					map.insert("reached".to_owned(), reached.clone().into());
				}
			}
			Self::Confirm => {
				map.insert("type".to_owned(), "confirm".into());
			}
			Self::Discard => {
				map.insert("type".to_owned(), "discard".into());
			}
			Self::Busy => {
				map.insert("type".to_owned(), "busy".into());
			}
			Self::Scan { interface } => {
				map.insert("type".to_owned(), "scan".into());
				insert_interface(&mut map, interface);
			}
			Self::Survey { interface } => {
				map.insert("type".to_owned(), "survey".into());
				insert_interface(&mut map, interface);
			}
			Self::Wps {
				method,
				interface,
				ssid,
			} => {
				map.insert("type".to_owned(), "wps".into());
				map.insert("method".to_owned(), method.clone().into());
				insert_interface(&mut map, interface);
				if let Some(ssid) = ssid {
					map.insert("ssid".to_owned(), ssid.clone().into());
				}
			}
			Self::Networks { access_points } => {
				map.insert("type".to_owned(), "networks".into());
				map.insert(
					"access-points".to_owned(),
					Json::Array(access_points.clone()),
				);
			}
			Self::Spectrum { spectrum } => {
				map.insert("type".to_owned(), "spectrum".into());
				map.insert("spectrum".to_owned(), Json::Object(spectrum.clone()));
			}
			Self::Power => {
				map.insert("type".to_owned(), "power".into());
			}
			Self::Acts { acts } => {
				map.insert("type".to_owned(), "acts".into());
				map.insert(
					"acts".to_owned(),
					Json::Array(acts.iter().cloned().map(Json::String).collect()),
				);
			}
			Self::Act { act } => {
				map.insert("type".to_owned(), "act".into());
				map.insert("act".to_owned(), act.clone().into());
			}
			Self::Accepted => {
				map.insert("type".to_owned(), "accepted".into());
			}
			Self::Refused { reason } => {
				map.insert("type".to_owned(), "refused".into());
				map.insert("reason".to_owned(), reason.clone().into());
			}
			Self::GoingAway { act, cause } => {
				map.insert("type".to_owned(), "going-away".into());
				map.insert("act".to_owned(), act.clone().into());
				map.insert("cause".to_owned(), cause.clone().into());
			}
			Self::Curve => {
				map.insert("type".to_owned(), "curve".into());
			}
			Self::Curves {
				document,
				lasts,
				recharge,
			} => {
				map.insert("type".to_owned(), "curves".into());
				if let Some(document) = document {
					map.insert("document".to_owned(), document.clone());
				}
				if let Some(lasts) = lasts {
					map.insert("lasts".to_owned(), lasts.to_json());
				}
				if let Some(recharge) = recharge {
					map.insert("recharge".to_owned(), recharge.to_json());
				}
			}
			Self::Load { document } => {
				map.insert("type".to_owned(), "load".into());
				map.insert("document".to_owned(), document.clone());
			}
			Self::Reset => {
				map.insert("type".to_owned(), "reset".into());
			}
		}
		map
	}
}

/// Name the wireless interface an act is addressed to, where it names one.
fn insert_interface(map: &mut Map<String, Json>, interface: &Option<String>) {
	if let Some(interface) = interface {
		map.insert("interface".to_owned(), interface.clone().into());
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
			"configure" => Ok(Message::Configure),
			"configuration" => Ok(Message::Configuration {
				document: object(&map, "document")?,
				capabilities: optional_object(&map, "capabilities")?,
			}),
			"applied" => Ok(Message::Applied {
				capabilities: optional_object(&map, "capabilities")?,
			}),
			"state" => Ok(Message::State {
				attachments: array(&map, "attachments")?,
				capabilities: optional_object(&map, "capabilities")?,
			}),
			"pin" => Ok(Message::Pin {
				pin: string(&map, "pin")?,
			}),
			"invalid" => Ok(Message::Invalid {
				at: string(&map, "at")?,
				reason: string(&map, "reason")?,
				reached: optional_string(&map, "reached")?,
			}),
			"confirm" => Ok(Message::Confirm),
			"discard" => Ok(Message::Discard),
			"busy" => Ok(Message::Busy),
			"scan" => Ok(Message::Scan {
				interface: optional_string(&map, "interface")?,
			}),
			"survey" => Ok(Message::Survey {
				interface: optional_string(&map, "interface")?,
			}),
			"wps" => Ok(Message::Wps {
				method: string(&map, "method")?,
				interface: optional_string(&map, "interface")?,
				ssid: optional_string(&map, "ssid")?,
			}),
			"networks" => Ok(Message::Networks {
				access_points: array(&map, "access-points")?,
			}),
			"spectrum" => Ok(Message::Spectrum {
				spectrum: object(&map, "spectrum")?,
			}),
			"power" => Ok(Message::Power),
			"acts" => Ok(Message::Acts {
				acts: strings(&map, "acts")?,
			}),
			"act" => Ok(Message::Act {
				act: string(&map, "act")?,
			}),
			"accepted" => Ok(Message::Accepted),
			"refused" => Ok(Message::Refused {
				reason: string(&map, "reason")?,
			}),
			"going-away" => Ok(Message::GoingAway {
				act: string(&map, "act")?,
				cause: string(&map, "cause")?,
			}),
			"curve" => Ok(Message::Curve),
			"curves" => Ok(Message::Curves {
				document: match map.get("document") {
					None | Some(Json::Null) => None,
					Some(document) => Some(document.clone()),
				},
				lasts: optional_span(&map, "lasts")?,
				recharge: optional_span(&map, "recharge")?,
			}),
			"load" => Ok(Message::Load {
				document: map
					.get("document")
					.cloned()
					.ok_or_else(|| de::Error::custom("this message carries a `document`"))?,
			}),
			"reset" => Ok(Message::Reset),
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

fn optional_string<E: de::Error>(
	map: &Map<String, Json>,
	member: &str,
) -> Result<Option<String>, E> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(None),
		Some(Json::String(value)) => Ok(Some(value.clone())),
		Some(_) => Err(de::Error::custom(format!("`{member}` is a string"))),
	}
}

fn object<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<Map<String, Json>, E> {
	match map.get(member) {
		Some(Json::Object(object)) => Ok(object.clone()),
		_ => Err(de::Error::custom(format!(
			"this message carries an object `{member}`"
		))),
	}
}

fn optional_object<E: de::Error>(
	map: &Map<String, Json>,
	member: &str,
) -> Result<Option<Map<String, Json>>, E> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(None),
		Some(Json::Object(object)) => Ok(Some(object.clone())),
		Some(_) => Err(de::Error::custom(format!("`{member}` is an object"))),
	}
}

fn optional_span<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<Option<Span>, E> {
	let Some(span) = optional_object::<E>(map, member)? else {
		return Ok(None);
	};
	let seconds = |name: &str| {
		span.get(name)
			.and_then(Json::as_f64)
			.ok_or_else(|| de::Error::custom(format!("`{member}` carries a number `{name}`")))
	};
	Ok(Some(Span {
		duration: seconds("duration")?,
		margin: seconds("margin")?,
	}))
}

fn strings<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<Vec<String>, E> {
	array(map, member)?
		.into_iter()
		.map(|item| match item {
			Json::String(value) => Ok(value),
			_ => Err(de::Error::custom(format!("`{member}` holds strings"))),
		})
		.collect()
}

fn array<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<Vec<Json>, E> {
	match map.get(member) {
		Some(Json::Array(items)) => Ok(items.clone()),
		_ => Err(de::Error::custom(format!(
			"this message carries an array `{member}`"
		))),
	}
}

impl MessageSet for Message {
	fn known_types() -> &'static [&'static str] {
		&[
			"hello",
			"subscribe",
			"fact",
			"reading",
			"configure",
			"configuration",
			"applied",
			"state",
			"pin",
			"invalid",
			"confirm",
			"discard",
			"busy",
			"scan",
			"survey",
			"wps",
			"networks",
			"spectrum",
			"power",
			"acts",
			"act",
			"accepted",
			"refused",
			"going-away",
			"curve",
			"curves",
			"load",
			"reset",
		]
	}

	/// `hello` carries no critical member; `subscribe` carries exactly one, its selector. The feature
	/// types NFO and CFG own are unconstrained: they name their own critical members through
	/// [`MessageSet::critical_members`] rather than being pinned here.
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
			// `configuration` pins its document critical, so a peer that cannot read the document does not
			// act on the message carrying it (CFG). Recorded in `wire-breaks.toml`.
			"configuration" => &["document"],
			// `act` pins its selector critical, so a device does not carry out an act it has not read
			// (CTL). Recorded in `wire-breaks.toml`.
			"act" => &["act"],
			_ => &[],
		}
	}
}

#[cfg(test)]
mod tests;
