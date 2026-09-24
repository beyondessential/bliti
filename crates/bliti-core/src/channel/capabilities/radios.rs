//! The rules of HOT that turn on the device's radios, which the mirror of `capabilities.document`
//! has no way to express: they relate one part of a document to another.
//!
//! Both read `capabilities.radios` and the hotspot's `interface` keys, and the document as JSON, so
//! the device and the client share them as they share [`super::check`].

use serde_json::{Map, Value as Json};

use super::super::config::{Invalid, Segment, path};

/// How a radio runs an access point beside a wireless client, as `alongside` names it (HOT).
const ONE_AT_A_TIME: &str = "one-at-a-time";

/// Refuse a document whose hotspot and a wireless candidate could be carried only by one radio
/// running one at a time (HOT).
///
/// A candidate may use the radio it names, or any radio; the hotspot the radio it names, or any able
/// to run one. The fault is at the hotspot's `interface` where it names one, else at the hotspot.
pub fn placement(
	document: &Map<String, Json>,
	capabilities: &Map<String, Json>,
) -> Result<(), Invalid> {
	let Some(hotspot) = document.get("hotspot").and_then(Json::as_object) else {
		return Ok(());
	};
	let radios = Radios::of(capabilities);
	let access_points = radios.hotspot(hotspot);
	if radios.all.is_empty() || access_points.is_empty() {
		return Ok(());
	}
	for candidate in wireless(document) {
		let apart = radios.for_candidate(candidate.interface).any(|station| {
			access_points
				.iter()
				.any(|ap| *ap != station || radios.alongside(ap) != Some(ONE_AT_A_TIME))
		});
		if !apart {
			let at: &[Segment<'_>] = if named(hotspot).is_some() {
				&[Segment::Name("hotspot"), Segment::Name("interface")]
			} else {
				&[Segment::Name("hotspot")]
			};
			return Err(Invalid {
				at: path(at),
				reason: format!(
					"the hotspot and {:?} could only run on {}, which cannot run both at once",
					candidate.label, access_points[0]
				),
				reached: None,
			});
		}
	}
	Ok(())
}

/// A wireless candidate, as far as the radios it may use go.
struct Candidate<'a> {
	label: &'a str,
	interface: Option<&'a str>,
}

/// The document's wireless candidates.
fn wireless(document: &Map<String, Json>) -> impl Iterator<Item = Candidate<'_>> {
	document
		.get("attachments")
		.and_then(Json::as_array)
		.into_iter()
		.flatten()
		.filter_map(Json::as_object)
		.filter(|attachment| attachment.get("kind").and_then(Json::as_str) == Some("wireless"))
		.map(|attachment| Candidate {
			label: attachment
				.get("label")
				.and_then(Json::as_str)
				.unwrap_or_default(),
			interface: named(attachment),
		})
}

/// The interface an object names, where it names one.
fn named(object: &Map<String, Json>) -> Option<&str> {
	object.get("interface").and_then(Json::as_str)
}

/// The device's radios, as `capabilities.radios` and the hotspot's `interface` keys describe them.
struct Radios<'a> {
	/// Each radio's interface, with its `alongside` where it has one.
	all: Vec<(&'a str, Option<&'a str>)>,
	/// The interfaces `capabilities.document.hotspot` is keyed by, where it is keyed by interface.
	hotspot_keys: Option<Vec<&'a str>>,
}

impl<'a> Radios<'a> {
	fn of(capabilities: &'a Map<String, Json>) -> Self {
		let all = capabilities
			.get("radios")
			.and_then(Json::as_object)
			.into_iter()
			.flatten()
			.map(|(name, radio)| (name.as_str(), radio.get("alongside").and_then(Json::as_str)))
			.collect();
		let hotspot_keys = capabilities
			.get("document")
			.and_then(|document| document.get("hotspot"))
			.and_then(|hotspot| hotspot.get("interface"))
			.and_then(Json::as_object)
			.map(|keyed| keyed.keys().map(String::as_str).collect());
		Self { all, hotspot_keys }
	}

	fn alongside(&self, radio: &str) -> Option<&'a str> {
		self.all
			.iter()
			.find(|(name, _)| *name == radio)
			.and_then(|(_, alongside)| *alongside)
	}

	/// The radios a wireless candidate may use: the one it names, else every one.
	fn for_candidate<'b>(
		&'b self,
		interface: Option<&'b str>,
	) -> impl Iterator<Item = &'a str> + 'b {
		self.all
			.iter()
			.map(|(name, _)| *name)
			.filter(move |name| interface.is_none_or(|named| named == *name))
	}

	/// The radios the hotspot may run on: the one it names, else every one able to run it, which are
	/// those the hotspot's `interface` keys name, or where it is not keyed by interface, those
	/// carrying `alongside` (NET).
	fn hotspot<'b>(&self, hotspot: &'b Map<String, Json>) -> Vec<&'b str>
	where
		'a: 'b,
	{
		if let Some(name) = named(hotspot) {
			return vec![name];
		}
		match &self.hotspot_keys {
			Some(keys) => keys.clone(),
			None => self
				.all
				.iter()
				.filter(|(_, alongside)| alongside.is_some())
				.map(|(name, _)| *name)
				.collect(),
		}
	}
}
