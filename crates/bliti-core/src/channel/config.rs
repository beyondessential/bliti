//! The network configuration document and the rules it obeys.
//!
//! Behaviour is specified in NET, with LINK, WLAN and HOT for the parts of the document each owns.
//! A [`Document`] is the whole-device declarative configuration a client sends over a session (CFG):
//! the ordered [`Attachment`] candidates, the optional [`Hotspot`], and the regulatory domain.
//!
//! The document rides the wire as raw JSON on [`super::messages::Message::Configuration`], not as
//! this type, for the same reason [`super::readings::Entry`] keeps its `traits` raw: a member a newer
//! peer added has to survive the envelope's round trip and be echoed back unchanged, and a device
//! that read, edited and wrote back through a typed struct would drop it. So a device holds the raw
//! map as its recorded configuration and parses it into a [`Document`] only to act on it, and a
//! client builds a [`Document`] and serialises it to raw to propose one.
//!
//! [`Document::parse`] carries the structural rules the specs pin without reference to a device: a
//! required member absent, a security kind that is not one of the four, a wired-static candidate with
//! no gateway. The rules that turn on what a device supports (a hotspot beside a wireless client on a
//! radio that cannot run both, a setting outside the device's capabilities) belong to the layer that
//! owns the capabilities shape, and are not here.

use serde_json::{Map, Value as Json};

use self::Segment::{Index, Name};

/// Why a document, or a part of it, cannot be accepted.
///
/// The shape of the `invalid` answer of CFG. `at` names the part at fault so a client can put an
/// operator's cursor on it; `reason` is the device's own words; `reached` is the verification stage
/// of LINK a candidate's verification got to, absent for any other failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid {
	/// Which part of the document is at fault, as an RFC 9535 Normalized Path (e.g.
	/// `$['attachments'][0]['gateway']`).
	pub at: String,
	/// What was wrong, in words an operator can act on.
	pub reason: String,
	/// The verification stage of LINK a candidate's verification stopped at; every stage before it
	/// passed. Absent where nothing was applied, or where what failed is not a candidate's
	/// verification, as a hotspot that does not start (CFG).
	pub reached: Option<String>,
}

impl Invalid {
	/// A fault found before anything was applied: an `at` and a `reason`, no stage reached.
	fn at(at: impl Into<String>, reason: impl Into<String>) -> Self {
		Self {
			at: at.into(),
			reason: reason.into(),
			reached: None,
		}
	}
}

/// A device's whole network configuration, as one declarative document (NET).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
	/// The ordered candidates a device works down to attach to a network (LINK).
	pub attachments: Vec<Attachment>,
	/// The hotspot the device runs, or none (HOT).
	pub hotspot: Option<Hotspot>,
	/// The domain the radio operates under, as an ISO 3166-1 alpha-2 code. Unset restricts the radio
	/// to what every domain permits (NET).
	pub regulatory_domain: Option<String>,
}

/// One way a device might attach to a network: the shared members of LINK, and a `kind` that carries
/// the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
	/// What the operator calls this candidate.
	pub label: String,
	/// The resolvers of this link, in the order they are queried; a device queries these before any
	/// the link supplies (LINK).
	pub nameservers: Vec<String>,
	/// What kind of attachment this is, and the members that kind carries.
	pub kind: AttachmentKind,
}

/// The three kinds of candidate LINK admits, each carrying its own members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentKind {
	/// A wireless network to join (WLAN).
	Wireless(Wireless),
	/// A wired port taking its addressing from DHCP or stateless autoconfiguration.
	WiredDynamic {
		/// The interface it applies to.
		interface: String,
	},
	/// A wired port with static addressing.
	WiredStatic {
		/// The interface it applies to.
		interface: String,
		/// The addresses held, each with its prefix length (e.g. `192.168.1.10/24`).
		addresses: Vec<String>,
		/// The gateway. A static candidate without one is invalid (LINK), so this is never absent in a
		/// parsed candidate.
		gateway: String,
	},
}

impl AttachmentKind {
	/// The `kind` string as it appears in the document.
	fn tag(&self) -> &'static str {
		match self {
			Self::Wireless(_) => "wireless",
			Self::WiredDynamic { .. } => "wired-dynamic",
			Self::WiredStatic { .. } => "wired-static",
		}
	}
}

/// A wireless network to join, and what is needed to join it (WLAN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wireless {
	/// The network to join.
	pub ssid: String,
	/// How the device authenticates to it.
	pub security: Security,
	/// Whether the network is joined without appearing in a scan.
	pub hidden: Option<bool>,
	/// The wireless interface it is joined on. Unset, the device chooses one able to carry it (LINK).
	pub interface: Option<String>,
}

/// How a device authenticates to a wireless network (WLAN).
///
/// The three key-based kinds carry a passphrase. Enterprise carries the EAP method and whatever
/// credentials that method requires, kept raw because the set of methods and their credentials is
/// open and no part of this crate acts on them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Security {
	/// WPA2 pre-shared key.
	Psk {
		/// The pre-shared key.
		passphrase: String,
	},
	/// WPA3 SAE.
	Sae {
		/// The SAE password.
		passphrase: String,
	},
	/// The WPA2/WPA3 transitional mode.
	PskSae {
		/// The shared passphrase.
		passphrase: String,
	},
	/// 802.1X enterprise. The EAP method and its credentials, kept as raw members.
	Enterprise {
		/// The EAP method and the credentials it requires, as the document carried them.
		members: Map<String, Json>,
	},
}

impl Security {
	/// The `kind` string as it appears in the document.
	fn tag(&self) -> &'static str {
		match self {
			Self::Psk { .. } => "psk",
			Self::Sae { .. } => "sae",
			Self::PskSae { .. } => "psk-sae",
			Self::Enterprise { .. } => "enterprise",
		}
	}
}

/// The wireless access point a device runs for clients to join directly (HOT).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotspot {
	/// The network the hotspot advertises. No default; a device derives it from nothing (HOT).
	pub ssid: String,
	/// What a client joins with. No default; kept clear of every other value on the device (HOT).
	pub passphrase: String,
	/// The wireless interface whose radio runs it. Unset, the device chooses one able to (HOT).
	pub interface: Option<String>,
	/// Whether clients reach the device's own network. Enabled where unset (HOT).
	pub share_upstream: Option<bool>,
	/// Whether clients are kept from reaching each other. Enabled where unset (HOT).
	pub isolate_clients: Option<bool>,
	/// The addresses handed to clients. Unset uses the range every other bliti device uses (HOT).
	pub dhcp_range: Option<String>,
	/// The band the hotspot operates on, where the radio lets it be chosen (HOT).
	pub band: Option<String>,
	/// The channel it operates on, where the radio lets it be chosen (HOT).
	pub channel: Option<u32>,
	/// The width of that channel, where the radio lets it be chosen (HOT).
	pub channel_width: Option<u32>,
}

impl Document {
	/// Parse a document from the raw JSON a `configuration` message carried, applying the structural
	/// rules the specs pin without reference to a device.
	///
	/// The rules that turn on a device's capabilities are not applied here: a caller with a device's
	/// capabilities in hand checks those separately. What this rejects is a document no device could
	/// accept whatever it supports.
	pub fn parse(document: &Map<String, Json>) -> Result<Self, Invalid> {
		let attachments = match document.get("attachments") {
			Some(Json::Array(items)) => items,
			_ => {
				return Err(Invalid::at(
					path(&[Name("attachments")]),
					"the document carries an array `attachments`",
				));
			}
		};

		let mut parsed = Vec::with_capacity(attachments.len());
		let mut dynamic_interfaces = Vec::new();
		for (index, item) in attachments.iter().enumerate() {
			let at = |member: &str| path(&[Name("attachments"), Index(index), Name(member)]);
			let Json::Object(candidate) = item else {
				return Err(Invalid::at(
					path(&[Name("attachments"), Index(index)]),
					"a candidate is an object",
				));
			};
			let attachment = Attachment::parse(candidate, index)?;
			// At most one wired-dynamic per interface (LINK). Several statics on one interface are the
			// site-switching case and stay legal.
			if let AttachmentKind::WiredDynamic { interface } = &attachment.kind {
				if dynamic_interfaces.contains(interface) {
					return Err(Invalid::at(
						at("interface"),
						format!("interface {interface:?} already has a dynamic candidate"),
					));
				}
				dynamic_interfaces.push(interface.clone());
			}
			parsed.push(attachment);
		}

		let hotspot = match document.get("hotspot") {
			None | Some(Json::Null) => None,
			Some(Json::Object(object)) => Some(Hotspot::parse(object)?),
			Some(_) => {
				return Err(Invalid::at(
					path(&[Name("hotspot")]),
					"the hotspot is an object",
				));
			}
		};

		let regulatory_domain = match document.get("regulatory-domain") {
			None | Some(Json::Null) => None,
			Some(Json::String(code)) => Some(code.clone()),
			Some(_) => {
				return Err(Invalid::at(
					path(&[Name("regulatory-domain")]),
					"the regulatory domain is a string",
				));
			}
		};

		Ok(Self {
			attachments: parsed,
			hotspot,
			regulatory_domain,
		})
	}

	/// Serialise this document to the raw JSON a `configuration` message carries. An absent optional
	/// member is left out, which is how NET says a device reads unset (NET).
	pub fn to_json(&self) -> Map<String, Json> {
		let mut map = Map::new();
		map.insert(
			"attachments".to_owned(),
			Json::Array(self.attachments.iter().map(Attachment::to_json).collect()),
		);
		if let Some(hotspot) = &self.hotspot {
			map.insert("hotspot".to_owned(), Json::Object(hotspot.to_json()));
		}
		if let Some(domain) = &self.regulatory_domain {
			map.insert("regulatory-domain".to_owned(), Json::String(domain.clone()));
		}
		map
	}
}

impl Attachment {
	fn parse(candidate: &Map<String, Json>, index: usize) -> Result<Self, Invalid> {
		let at = |member: &str| path(&[Name("attachments"), Index(index), Name(member)]);
		let label = string(candidate, "label")
			.map_err(|reason| Invalid::at(at("label"), reason))?
			.to_owned();
		let nameservers = string_array(candidate, "nameservers")
			.map_err(|reason| Invalid::at(at("nameservers"), reason))?;

		let kind = match candidate.get("kind").and_then(Json::as_str) {
			Some("wireless") => AttachmentKind::Wireless(Wireless::parse(candidate, index)?),
			Some("wired-dynamic") => AttachmentKind::WiredDynamic {
				interface: string(candidate, "interface")
					.map_err(|reason| Invalid::at(at("interface"), reason))?
					.to_owned(),
			},
			Some("wired-static") => {
				let interface = string(candidate, "interface")
					.map_err(|reason| Invalid::at(at("interface"), reason))?
					.to_owned();
				let addresses = string_array(candidate, "addresses")
					.map_err(|reason| Invalid::at(at("addresses"), reason))?;
				if addresses.is_empty() {
					return Err(Invalid::at(
						at("addresses"),
						"a static candidate carries at least one address",
					));
				}
				// A static candidate without a gateway cannot be told from any other, so LINK makes it
				// invalid rather than allowing it.
				let gateway = match candidate.get("gateway").and_then(Json::as_str) {
					Some(gateway) => gateway.to_owned(),
					None => {
						return Err(Invalid::at(
							at("gateway"),
							"a static candidate carries a gateway",
						));
					}
				};
				AttachmentKind::WiredStatic {
					interface,
					addresses,
					gateway,
				}
			}
			_ => {
				return Err(Invalid::at(
					at("kind"),
					"a candidate's kind is `wireless`, `wired-dynamic` or `wired-static`",
				));
			}
		};

		Ok(Self {
			label,
			nameservers,
			kind,
		})
	}

	fn to_json(&self) -> Json {
		let mut map = Map::new();
		map.insert("kind".to_owned(), Json::String(self.kind.tag().to_owned()));
		map.insert("label".to_owned(), Json::String(self.label.clone()));
		if !self.nameservers.is_empty() {
			map.insert(
				"nameservers".to_owned(),
				Json::Array(
					self.nameservers
						.iter()
						.map(|ns| Json::String(ns.clone()))
						.collect(),
				),
			);
		}
		match &self.kind {
			AttachmentKind::Wireless(wireless) => wireless.write_into(&mut map),
			AttachmentKind::WiredDynamic { interface } => {
				map.insert("interface".to_owned(), Json::String(interface.clone()));
			}
			AttachmentKind::WiredStatic {
				interface,
				addresses,
				gateway,
			} => {
				map.insert("interface".to_owned(), Json::String(interface.clone()));
				map.insert(
					"addresses".to_owned(),
					Json::Array(addresses.iter().map(|a| Json::String(a.clone())).collect()),
				);
				map.insert("gateway".to_owned(), Json::String(gateway.clone()));
			}
		}
		Json::Object(map)
	}
}

impl Wireless {
	fn parse(candidate: &Map<String, Json>, index: usize) -> Result<Self, Invalid> {
		let at = |member: &str| path(&[Name("attachments"), Index(index), Name(member)]);
		let ssid = string(candidate, "ssid")
			.map_err(|reason| Invalid::at(at("ssid"), reason))?
			.to_owned();

		let security = match candidate.get("security") {
			Some(Json::Object(object)) => Security::parse(object, index)?,
			_ => {
				return Err(Invalid::at(
					at("security"),
					"a wireless candidate carries a `security` object",
				));
			}
		};

		let hidden = match candidate.get("hidden") {
			None | Some(Json::Null) => None,
			Some(Json::Bool(hidden)) => Some(*hidden),
			Some(_) => return Err(Invalid::at(at("hidden"), "`hidden` is a boolean")),
		};
		let interface = optional_string(candidate, "interface")
			.map_err(|reason| Invalid::at(at("interface"), reason))?;

		Ok(Self {
			ssid,
			security,
			hidden,
			interface,
		})
	}

	fn write_into(&self, map: &mut Map<String, Json>) {
		map.insert("ssid".to_owned(), Json::String(self.ssid.clone()));
		map.insert("security".to_owned(), Json::Object(self.security.to_json()));
		if let Some(hidden) = self.hidden {
			map.insert("hidden".to_owned(), Json::Bool(hidden));
		}
		if let Some(interface) = &self.interface {
			map.insert("interface".to_owned(), Json::String(interface.clone()));
		}
	}
}

impl Security {
	fn parse(security: &Map<String, Json>, index: usize) -> Result<Self, Invalid> {
		let at = |member: &str| {
			path(&[
				Name("attachments"),
				Index(index),
				Name("security"),
				Name(member),
			])
		};
		let passphrase = |kind: &str| {
			security
				.get("passphrase")
				.and_then(Json::as_str)
				.map(ToOwned::to_owned)
				.ok_or_else(|| {
					Invalid::at(
						at("passphrase"),
						format!("a {kind} network carries a passphrase"),
					)
				})
		};
		match security.get("kind").and_then(Json::as_str) {
			Some("psk") => Ok(Self::Psk {
				passphrase: passphrase("psk")?,
			}),
			Some("sae") => Ok(Self::Sae {
				passphrase: passphrase("sae")?,
			}),
			Some("psk-sae") => Ok(Self::PskSae {
				passphrase: passphrase("psk-sae")?,
			}),
			Some("enterprise") => Ok(Self::Enterprise {
				members: security.clone(),
			}),
			_ => Err(Invalid::at(
				at("kind"),
				"security is one of `psk`, `sae`, `psk-sae` or `enterprise`",
			)),
		}
	}

	fn to_json(&self) -> Map<String, Json> {
		match self {
			Self::Psk { passphrase } | Self::Sae { passphrase } | Self::PskSae { passphrase } => {
				let mut map = Map::new();
				map.insert("kind".to_owned(), Json::String(self.tag().to_owned()));
				map.insert("passphrase".to_owned(), Json::String(passphrase.clone()));
				map
			}
			// Enterprise keeps whatever members it arrived with, including its own `kind`.
			Self::Enterprise { members } => members.clone(),
		}
	}
}

impl Hotspot {
	fn parse(hotspot: &Map<String, Json>) -> Result<Self, Invalid> {
		let ssid = string(hotspot, "ssid")
			.map_err(|reason| Invalid::at(hotspot_at("ssid"), reason))?
			.to_owned();
		let passphrase = string(hotspot, "passphrase")
			.map_err(|reason| Invalid::at(hotspot_at("passphrase"), reason))?
			.to_owned();
		let interface = optional_string(hotspot, "interface")
			.map_err(|reason| Invalid::at(hotspot_at("interface"), reason))?;
		let share_upstream = bool_member(hotspot, "share-upstream")
			.map_err(|reason| Invalid::at(hotspot_at("share-upstream"), reason))?;
		let isolate_clients = bool_member(hotspot, "isolate-clients")
			.map_err(|reason| Invalid::at(hotspot_at("isolate-clients"), reason))?;
		let dhcp_range = match hotspot.get("dhcp-range") {
			None | Some(Json::Null) => None,
			Some(Json::String(range)) => Some(range.clone()),
			Some(_) => {
				return Err(Invalid::at(
					hotspot_at("dhcp-range"),
					"the DHCP range is a string",
				));
			}
		};
		let band = match hotspot.get("band") {
			None | Some(Json::Null) => None,
			Some(Json::String(band)) => Some(band.clone()),
			Some(_) => return Err(Invalid::at(hotspot_at("band"), "the band is a string")),
		};
		let channel = u32_member(hotspot, "channel")
			.map_err(|reason| Invalid::at(hotspot_at("channel"), reason))?;
		let channel_width = u32_member(hotspot, "channel-width")
			.map_err(|reason| Invalid::at(hotspot_at("channel-width"), reason))?;

		Ok(Self {
			ssid,
			passphrase,
			interface,
			share_upstream,
			isolate_clients,
			dhcp_range,
			band,
			channel,
			channel_width,
		})
	}

	fn to_json(&self) -> Map<String, Json> {
		let mut map = Map::new();
		map.insert("ssid".to_owned(), Json::String(self.ssid.clone()));
		map.insert(
			"passphrase".to_owned(),
			Json::String(self.passphrase.clone()),
		);
		if let Some(interface) = &self.interface {
			map.insert("interface".to_owned(), Json::String(interface.clone()));
		}
		if let Some(share) = self.share_upstream {
			map.insert("share-upstream".to_owned(), Json::Bool(share));
		}
		if let Some(isolate) = self.isolate_clients {
			map.insert("isolate-clients".to_owned(), Json::Bool(isolate));
		}
		if let Some(range) = &self.dhcp_range {
			map.insert("dhcp-range".to_owned(), Json::String(range.clone()));
		}
		if let Some(band) = &self.band {
			map.insert("band".to_owned(), Json::String(band.clone()));
		}
		if let Some(channel) = self.channel {
			map.insert("channel".to_owned(), Json::Number(channel.into()));
		}
		if let Some(width) = self.channel_width {
			map.insert("channel-width".to_owned(), Json::Number(width.into()));
		}
		map
	}
}

/// One step of a path into a document: a member by name, or an array element by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment<'a> {
	/// A member of an object.
	Name(&'a str),
	/// An element of an array.
	Index(usize),
}

/// The RFC 9535 Normalized Path of a node, e.g. `$['attachments'][1]['gateway']`.
///
/// The normalized form rather than the dot shorthand, because a Normalized Path is the one spelling
/// of a node, so a client can find the field an `invalid` names by comparing strings, and because the
/// shorthand cannot carry a hyphenated name such as `share-upstream`.
pub fn path(segments: &[Segment<'_>]) -> String {
	let mut out = String::from("$");
	for segment in segments {
		match segment {
			Segment::Index(index) => {
				out.push('[');
				out.push_str(&index.to_string());
				out.push(']');
			}
			Segment::Name(name) => {
				out.push_str("['");
				for c in name.chars() {
					match c {
						'\u{8}' => out.push_str("\\b"),
						'\u{c}' => out.push_str("\\f"),
						'\n' => out.push_str("\\n"),
						'\r' => out.push_str("\\r"),
						'\t' => out.push_str("\\t"),
						'\'' => out.push_str("\\'"),
						'\\' => out.push_str("\\\\"),
						c if c < ' ' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
						c => out.push(c),
					}
				}
				out.push_str("']");
			}
		}
	}
	out
}

/// The path of a member of the hotspot.
fn hotspot_at(member: &str) -> String {
	path(&[Name("hotspot"), Name(member)])
}

/// A required string member, or a message naming it.
fn string<'a>(map: &'a Map<String, Json>, member: &str) -> Result<&'a str, String> {
	map.get(member)
		.and_then(Json::as_str)
		.ok_or_else(|| format!("a string `{member}`"))
}

/// An optional string member.
fn optional_string(map: &Map<String, Json>, member: &str) -> Result<Option<String>, String> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(None),
		Some(Json::String(value)) => Ok(Some(value.clone())),
		Some(_) => Err(format!("`{member}` is a string")),
	}
}

/// An optional array of strings, empty where the member is absent.
fn string_array(map: &Map<String, Json>, member: &str) -> Result<Vec<String>, String> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(Vec::new()),
		Some(Json::Array(items)) => items
			.iter()
			.map(|item| {
				item.as_str()
					.map(ToOwned::to_owned)
					.ok_or_else(|| format!("`{member}` is an array of strings"))
			})
			.collect(),
		Some(_) => Err(format!("`{member}` is an array of strings")),
	}
}

/// An optional boolean member.
fn bool_member(map: &Map<String, Json>, member: &str) -> Result<Option<bool>, String> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(None),
		Some(Json::Bool(value)) => Ok(Some(*value)),
		Some(_) => Err(format!("`{member}` is a boolean")),
	}
}

/// An optional non-negative integer member.
fn u32_member(map: &Map<String, Json>, member: &str) -> Result<Option<u32>, String> {
	match map.get(member) {
		None | Some(Json::Null) => Ok(None),
		Some(value) => value
			.as_u64()
			.and_then(|n| u32::try_from(n).ok())
			.map(Some)
			.ok_or_else(|| format!("`{member}` is a whole number")),
	}
}

#[cfg(test)]
mod tests {
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
					"nameservers": ["10.0.0.1"],
					"ssid": "Clinic",
					"security": { "kind": "sae", "passphrase": "a good long passphrase" },
					"hidden": true
				},
				{
					"kind": "wired-static",
					"label": "wall port",
					"interface": "eth0",
					"addresses": ["192.168.1.10/24"],
					"gateway": "192.168.1.1"
				},
				{ "kind": "wired-dynamic", "label": "spare port", "interface": "eth1" }
			],
			"hotspot": {
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
				{ "kind": "wired-static", "label": "site a", "interface": "eth0",
				  "addresses": ["10.1.0.5/24"], "gateway": "10.1.0.1" },
				{ "kind": "wired-static", "label": "site b", "interface": "eth0",
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
				{ "kind": "wired-dynamic", "label": "a", "interface": "eth0" },
				{ "kind": "wired-dynamic", "label": "b", "interface": "eth0" }
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
					"kind": "wireless", "label": "w", "ssid": "S",
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

	/// An unknown security kind is rejected.
	#[test]
	fn an_unknown_security_kind_is_invalid() {
		let err = document(serde_json::json!({
			"attachments": [{
				"kind": "wireless", "label": "w", "ssid": "S",
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
				"kind": "wireless", "label": "eduroam", "ssid": "eduroam",
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

	/// An unknown attachment kind is rejected, naming the kind member.
	#[test]
	fn an_unknown_attachment_kind_is_invalid() {
		let err = document(serde_json::json!({
			"attachments": [{ "kind": "cellular", "label": "modem" }]
		}))
		.unwrap_err();
		assert_eq!(err.at, "$['attachments'][0]['kind']");
	}

	/// A hotspot carries an SSID and a passphrase, both required, and the fault names the missing one.
	#[test]
	fn a_hotspot_without_a_passphrase_is_invalid() {
		let err = document(serde_json::json!({
			"attachments": [],
			"hotspot": { "ssid": "setup" }
		}))
		.unwrap_err();
		assert_eq!(err.at, "$['hotspot']['passphrase']");
	}

	/// An unset hotspot boolean parses to None, which HOT reads as its enabled default, and a set one
	/// carries through.
	#[test]
	fn unset_hotspot_switches_are_none() {
		let parsed = document(serde_json::json!({
			"attachments": [],
			"hotspot": { "ssid": "s", "passphrase": "p", "isolate-clients": false }
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
				{ "kind": "wireless", "label": "uplink", "ssid": "Clinic", "interface": "wlx00c0caa1b2c3",
				  "security": { "kind": "sae", "passphrase": "a good long passphrase" } },
				{ "kind": "wireless", "label": "either", "ssid": "Office",
				  "security": { "kind": "psk", "passphrase": "another passphrase" } }
			],
			"hotspot": { "ssid": "setup", "passphrase": "read this aloud", "interface": "wlan0" }
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
			"hotspot": { "ssid": "s", "passphrase": "p", "interface": 0 }
		}))
		.unwrap_err();
		assert_eq!(err.at, "$['hotspot']['interface']");
	}
}
