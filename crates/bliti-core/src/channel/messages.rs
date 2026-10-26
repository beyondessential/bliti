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
	config::{Failure, Outcome},
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

	/// A device's report of the configuration it holds, sent on opening a session and on every change,
	/// and a client's proposal of another: the network configuration document and, from a device,
	/// the capabilities it supports where they are new or changed. Its `document` is critical, so an
	/// end that cannot read the document does not act on the message carrying it (CFG).
	Configuration {
		/// The configuration document (NET), kept raw so a member a newer peer added survives the
		/// envelope round trip and is echoed back unchanged, exactly as [`Entry`] keeps its traits.
		document: Map<String, Json>,
		/// What the device supports, where it is new or changed. Kept raw for the same reason, and because
		/// the layer that probes capabilities owns their shape.
		capabilities: Option<Map<String, Json>>,
	},

	/// How the device is faring on the configuration it holds: whether another session holds it, the
	/// outcome of verifying it, and the state of each candidate, position for position (CFG).
	State {
		/// Whether another session holds the configuration, so this one only watches.
		busy: bool,
		/// The outcome of the configuration the device holds.
		outcome: Outcome,
		/// Why the outcome is `failed`; absent otherwise.
		failure: Option<Failure>,
		/// One entry per attachment, kept raw as the document is.
		attachments: Vec<Json>,
		/// The device's capabilities, where they changed.
		capabilities: Option<Map<String, Json>>,
	},

	/// An end's answer that it cannot carry out the act asked for (MSG).
	CannotAct {
		/// The act asked for, as the feature spec that defines it names it.
		act: String,
		/// Which part of the request is at fault, as an RFC 9535 Normalized Path rooted at the message
		/// asking for the act.
		at: String,
		/// Why, in the answering end's own words.
		reason: String,
	},

	/// An end's answer that it cannot take the document offered (MSG).
	CannotConfigure {
		/// Which part of the document is at fault, as an RFC 9535 Normalized Path rooted at the document.
		at: String,
		/// Why, in the answering end's own words.
		reason: String,
	},

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

	/// A client opening a Tailscale stream, first on the stream it opens for it (TSC).
	Tailscale,

	/// A device's answer to `tailscale`, and what it sends on every Tailscale stream when the acts it
	/// can carry out change (TSC). Named as strings, as [`Message::Acts`] is.
	TailscaleActs {
		/// The acts, by wire name.
		acts: Vec<String>,
	},

	/// A client asking a device to carry out a Tailscale act (TSC). Every member is critical, so a
	/// device does not act on a request it has not wholly read.
	TailscaleAct {
		/// The act, by wire name.
		act: String,
		/// How a `sign-in` is to be done, by wire name.
		method: Option<String>,
		/// The auth key a `sign-in` by `auth-key` uses.
		key: Option<Secret>,
		/// The name a `sign-in` registers the device under.
		name: Option<String>,
		/// The machine a `ping` tests, by its name on the tailnet.
		peer: Option<String>,
	},

	/// A device's answer that it will carry out the Tailscale act asked for (TSC).
	TailscaleAccepted,

	/// A device handing a client the login link of a `sign-in` by `login-link` (TSC).
	TailscaleLoginLink {
		/// The login link.
		url: Secret,
	},

	/// A device's report of what a `netcheck` or a `ping` found (TSC). Its shape is the act's, so it
	/// rides as raw JSON, and a member a newer device adds survives.
	TailscaleResult {
		/// What the test found.
		result: Map<String, Json>,
	},

	/// A device's report that it accepted a Tailscale act and did not carry it out (TSC).
	TailscaleFailed {
		/// Why, in the device's own words.
		reason: String,
	},
}

/// Something a message carries that must not reach a log: an auth key or a login link (TSC).
///
/// Its [`fmt::Debug`] says nothing of what it holds, so a message that is logged with `{:?}` does not
/// spill it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
	/// Hold `value`.
	pub fn new(value: impl Into<String>) -> Self {
		Self(value.into())
	}

	/// The secret itself, for whatever it is for.
	pub fn expose(&self) -> &str {
		&self.0
	}
}

impl fmt::Debug for Secret {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("Secret(..)")
	}
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
			Self::State {
				busy,
				outcome,
				failure,
				attachments,
				capabilities,
			} => {
				map.insert("type".to_owned(), "state".into());
				map.insert("busy".to_owned(), (*busy).into());
				map.insert("outcome".to_owned(), outcome.as_str().into());
				if let Some(failure) = failure {
					map.insert("failure".to_owned(), Json::Object(failure.to_map()));
				}
				map.insert("attachments".to_owned(), Json::Array(attachments.clone()));
				if let Some(capabilities) = capabilities {
					map.insert(
						"capabilities".to_owned(),
						Json::Object(capabilities.clone()),
					);
				}
			}
			Self::CannotAct { act, at, reason } => {
				map.insert("type".to_owned(), "cannot-act".into());
				map.insert("act".to_owned(), act.clone().into());
				map.insert("at".to_owned(), at.clone().into());
				map.insert("reason".to_owned(), reason.clone().into());
			}
			Self::CannotConfigure { at, reason } => {
				map.insert("type".to_owned(), "cannot-configure".into());
				map.insert("at".to_owned(), at.clone().into());
				map.insert("reason".to_owned(), reason.clone().into());
			}
			Self::Scan { interface } => {
				map.insert("type".to_owned(), "scan".into());
				insert_interface(&mut map, interface);
			}
			Self::Survey { interface } => {
				map.insert("type".to_owned(), "survey".into());
				insert_interface(&mut map, interface);
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
			Self::Tailscale => {
				map.insert("type".to_owned(), "tailscale".into());
			}
			Self::TailscaleActs { acts } => {
				map.insert("type".to_owned(), "tailscale-acts".into());
				map.insert(
					"acts".to_owned(),
					Json::Array(acts.iter().cloned().map(Json::String).collect()),
				);
			}
			Self::TailscaleAct {
				act,
				method,
				key,
				name,
				peer,
			} => {
				map.insert("type".to_owned(), "tailscale-act".into());
				map.insert("act".to_owned(), act.clone().into());
				for (member, value) in [
					("method", method.as_deref()),
					("key", key.as_ref().map(Secret::expose)),
					("name", name.as_deref()),
					("peer", peer.as_deref()),
				] {
					if let Some(value) = value {
						map.insert(member.to_owned(), value.into());
					}
				}
			}
			Self::TailscaleAccepted => {
				map.insert("type".to_owned(), "tailscale-accepted".into());
			}
			Self::TailscaleLoginLink { url } => {
				map.insert("type".to_owned(), "tailscale-login-link".into());
				map.insert("url".to_owned(), url.expose().into());
			}
			Self::TailscaleResult { result } => {
				map.insert("type".to_owned(), "tailscale-result".into());
				map.insert("result".to_owned(), Json::Object(result.clone()));
			}
			Self::TailscaleFailed { reason } => {
				map.insert("type".to_owned(), "tailscale-failed".into());
				map.insert("reason".to_owned(), reason.clone().into());
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
			"state" => Ok(Message::State {
				busy: boolean(&map, "busy")?,
				outcome: Outcome::read(&string::<A::Error>(&map, "outcome")?)
					.map_err(de::Error::custom)?,
				failure: optional_failure(&map, "failure")?,
				attachments: array(&map, "attachments")?,
				capabilities: optional_object(&map, "capabilities")?,
			}),
			"cannot-act" => Ok(Message::CannotAct {
				act: string(&map, "act")?,
				at: string(&map, "at")?,
				reason: string(&map, "reason")?,
			}),
			"cannot-configure" => Ok(Message::CannotConfigure {
				at: string(&map, "at")?,
				reason: string(&map, "reason")?,
			}),
			"scan" => Ok(Message::Scan {
				interface: optional_string(&map, "interface")?,
			}),
			"survey" => Ok(Message::Survey {
				interface: optional_string(&map, "interface")?,
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
			"tailscale" => Ok(Message::Tailscale),
			"tailscale-acts" => Ok(Message::TailscaleActs {
				acts: strings(&map, "acts")?,
			}),
			"tailscale-act" => Ok(Message::TailscaleAct {
				act: string(&map, "act")?,
				method: optional_string(&map, "method")?,
				key: optional_string(&map, "key")?.map(Secret::new),
				name: optional_string(&map, "name")?,
				peer: optional_string(&map, "peer")?,
			}),
			"tailscale-accepted" => Ok(Message::TailscaleAccepted),
			"tailscale-login-link" => Ok(Message::TailscaleLoginLink {
				url: Secret::new(string::<A::Error>(&map, "url")?),
			}),
			"tailscale-result" => Ok(Message::TailscaleResult {
				result: object(&map, "result")?,
			}),
			"tailscale-failed" => Ok(Message::TailscaleFailed {
				reason: string(&map, "reason")?,
			}),
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

fn boolean<E: de::Error>(map: &Map<String, Json>, member: &str) -> Result<bool, E> {
	map.get(member)
		.and_then(Json::as_bool)
		.ok_or_else(|| de::Error::custom(format!("this message carries a boolean `{member}`")))
}

fn optional_failure<E: de::Error>(
	map: &Map<String, Json>,
	member: &str,
) -> Result<Option<Failure>, E> {
	optional_object::<E>(map, member)?
		.map(|failure| Failure::read(&failure).map_err(de::Error::custom))
		.transpose()
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
			"state",
			"cannot-act",
			"cannot-configure",
			"scan",
			"survey",
			"networks",
			"spectrum",
			"power",
			"acts",
			"act",
			"accepted",
			"going-away",
			"curve",
			"curves",
			"load",
			"reset",
			"tailscale",
			"tailscale-acts",
			"tailscale-act",
			"tailscale-accepted",
			"tailscale-login-link",
			"tailscale-result",
			"tailscale-failed",
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
			// act on the message carrying it (CFG).
			"configuration" => &["document"],
			// `act` pins its selector critical, so a device does not carry out an act it has not read
			// (CTL).
			"act" => &["act"],
			// `tailscale-act` pins every member critical, since an act carried out without a parameter it
			// did not read would be a different act (TSC).
			"tailscale-act" => &["act", "method", "key", "name", "peer"],
			_ => &[],
		}
	}
}

#[cfg(test)]
mod tests;
