//! One shape for everything a device reports about itself: a fact or a reading.
//!
//! Behaviour is specified in NFO. An [`Entry`] names what it is against a catalogue, says what it is
//! about in its `traits`, and carries a value of some `kind`. A reader renders it from that alone, so
//! a device that gains an entry appears in a reader that has never heard of it with no release in
//! between. A `fact` and a `reading` are the same shape; which catalogue names an entry is the only
//! difference, and it is carried by the message type ([`super::messages`]).
//!
//! `traits` and `value` are kept as raw JSON rather than parsed into fields. That is deliberate:
//! identity is the whole `traits` object, including traits this build cannot read, so a sender that
//! adds a trait splitting one series into several does not have an older reader merge them into one
//! wrong graph. Keeping them raw is also what lets them survive the round trip the envelope's
//! unknown-member detection depends on (see [`super::envelope`]).

use serde_json::{Map, Value as Json};

/// The value kinds this catalogue uses. The vocabulary is open: a reader that meets a kind not here
/// renders the value stringified (NFO, VIEW).
pub mod kind {
	/// A string with no numeric meaning.
	pub const TEXT: &str = "text";
	/// A number from 0 to 1 inclusive, a proportion of a whole.
	pub const FRACTION: &str = "fraction";
	/// A measurement, carrying a unit.
	pub const QUANTITY: &str = "quantity";
	/// An elapsed time, in seconds.
	pub const DURATION: &str = "duration";
	/// An instant, as RFC 3339.
	pub const DATETIME: &str = "datetime";
	/// An IPv4 address.
	pub const IPV4: &str = "ipv4";
	/// An IPv6 address.
	pub const IPV6: &str = "ipv6";
}

/// The `status` trait: how the datum stands. Every entry carries one (NFO).
pub const STATUS: &str = "status";
/// The `limits` trait: marks on a reading's scale.
pub const LIMITS: &str = "limits";

/// The entries NFO's catalogue lists as one entry per value held, which are told apart by their value
/// as well as by their name, kind and distinguishing traits.
pub const TOLD_APART_BY_VALUE: &[&str] = &["network-address"];

/// One fact or reading. One shape; the message type says which catalogue names it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
	/// Milliseconds since the sender booted, when it was taken. Meaningful only against other `at`
	/// values from the same sender.
	pub at: u64,
	/// What it is, named against the catalogue.
	pub name: String,
	/// What it is about: distinguishing traits and descriptive ones, including `status` and any
	/// `limits`. Kept raw so an unrecognised trait survives and still tells two series apart.
	pub traits: Map<String, Json>,
	/// What the value is.
	pub kind: String,
	/// The unit the value is in, named in full, where it has one.
	pub unit: Option<String>,
	/// The value, absent where the status is `skipped` or `broken`.
	pub value: Option<Json>,
}

/// How a datum stands, as the `status` trait's `is` member (NFO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
	/// The measurement was taken and what it measures is well.
	Passed,
	/// What it measures is degraded, but not gravely.
	Warning,
	/// What it measures is unwell.
	Failed,
	/// A precondition was not met, so nothing was measured.
	Skipped,
	/// The measurement was attempted and errored.
	Broken,
	/// The entry no longer applies, and a reader drops it.
	Ended,
}

impl Status {
	/// The wire string, matching the vocabulary BES software reports checks in.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Passed => "passed",
			Self::Warning => "warning",
			Self::Failed => "failed",
			Self::Skipped => "skipped",
			Self::Broken => "broken",
			Self::Ended => "ended",
		}
	}

	/// Whether a value accompanies this status.
	fn carries_value(self) -> bool {
		matches!(self, Self::Passed | Self::Warning | Self::Failed)
	}
}

impl Entry {
	/// A passed entry carrying a value. The starting point for everything a device reports; downgrade
	/// it with [`Entry::warning`] and the rest.
	pub fn new(at: u64, name: impl Into<String>, kind: impl Into<String>, value: Json) -> Self {
		let mut entry = Self {
			at,
			name: name.into(),
			traits: Map::new(),
			kind: kind.into(),
			unit: None,
			value: Some(value),
		};
		entry.set_status(Status::Passed, None);
		entry
	}

	/// A fraction, from 0 to 1, rounded to four places.
	pub fn fraction(at: u64, name: impl Into<String>, number: f64) -> Self {
		Self::new(at, name, kind::FRACTION, json_number(round4(number)))
	}

	/// A quantity in a named unit, rounded to four places. The unit is spelled out in full (NFO).
	pub fn quantity(
		at: u64,
		name: impl Into<String>,
		unit: impl Into<String>,
		number: f64,
	) -> Self {
		let mut entry = Self::new(at, name, kind::QUANTITY, json_number(round4(number)));
		entry.unit = Some(unit.into());
		entry
	}

	/// An elapsed time, in seconds.
	pub fn duration(at: u64, name: impl Into<String>, seconds: f64) -> Self {
		Self::new(at, name, kind::DURATION, json_number(round4(seconds)))
	}

	/// Text with no numeric meaning.
	pub fn text(at: u64, name: impl Into<String>, text: impl Into<String>) -> Self {
		Self::new(at, name, kind::TEXT, Json::String(text.into()))
	}

	/// An instant, as RFC 3339.
	pub fn datetime(at: u64, name: impl Into<String>, rfc3339: impl Into<String>) -> Self {
		Self::new(at, name, kind::DATETIME, Json::String(rfc3339.into()))
	}

	/// An internet address, of the kind its family names.
	pub fn address(
		at: u64,
		name: impl Into<String>,
		kind: impl Into<String>,
		ip: impl Into<String>,
	) -> Self {
		Self::new(at, name, kind, Json::String(ip.into()))
	}

	/// Give a trait a value. A trait that only qualifies another belongs inside it, not beside it, so
	/// pass an object where a trait has more than one thing to say (NFO).
	#[must_use]
	pub fn with_trait(mut self, name: impl Into<String>, value: Json) -> Self {
		self.traits.insert(name.into(), value);
		self
	}

	/// Add a mark to the reading's scale. Kept as the `limits` trait, a list of `{at, label}`.
	#[must_use]
	pub fn with_limit(mut self, at: f64, label: impl Into<String>) -> Self {
		let mark = serde_json::json!({ "at": round4(at), "label": label.into() });
		match self.traits.get_mut(LIMITS) {
			Some(Json::Array(marks)) => marks.push(mark),
			_ => {
				self.traits
					.insert(LIMITS.to_owned(), Json::Array(vec![mark]));
			}
		}
		self
	}

	/// Report the entry degraded, keeping its value. `reason` is the sender's own words (NFO).
	#[must_use]
	pub fn warning(mut self, reason: impl Into<String>) -> Self {
		self.set_status(Status::Warning, Some(reason.into()));
		self
	}

	/// Report what the entry measures as unwell, keeping its value.
	#[must_use]
	pub fn failed(mut self, reason: impl Into<String>) -> Self {
		self.set_status(Status::Failed, Some(reason.into()));
		self
	}

	/// An entry whose precondition was not met: no value, with a reason.
	pub fn skipped(
		at: u64,
		name: impl Into<String>,
		kind: impl Into<String>,
		reason: impl Into<String>,
	) -> Self {
		let mut entry = Self {
			at,
			name: name.into(),
			traits: Map::new(),
			kind: kind.into(),
			unit: None,
			value: None,
		};
		entry.set_status(Status::Skipped, Some(reason.into()));
		entry
	}

	/// An entry the device declared but could not take: no value, with a reason. Hardware that is not
	/// fitted is left out entirely; this is for hardware that is there and did not answer (NFO).
	pub fn broken(
		at: u64,
		name: impl Into<String>,
		kind: impl Into<String>,
		reason: impl Into<String>,
	) -> Self {
		let mut entry = Self {
			at,
			name: name.into(),
			traits: Map::new(),
			kind: kind.into(),
			unit: None,
			value: None,
		};
		entry.set_status(Status::Broken, Some(reason.into()));
		entry
	}

	/// This entry, sent once more to say it no longer applies: everything that told it apart, so a
	/// reader knows which entry it drops, and the reason it ended. Only an entry told apart by its
	/// value keeps the value, as the one that no longer holds (NFO).
	pub fn ended(&self, at: u64, reason: impl Into<String>) -> Self {
		let mut entry = self.clone();
		entry.at = at;
		entry.set_status(Status::Ended, Some(reason.into()));
		if self.told_apart_by_value() {
			entry.value = self.value.clone();
		}
		entry
	}

	/// Whether this entry is told apart by its value, being one of several held at once (NFO).
	pub fn told_apart_by_value(&self) -> bool {
		TOLD_APART_BY_VALUE.contains(&self.name.as_str())
	}

	/// Set the `status` trait, tying value presence to it: present for `passed`, `warning` and
	/// `failed`, absent for `skipped`, `broken` and `ended` (NFO).
	fn set_status(&mut self, status: Status, reason: Option<String>) {
		let mut object = Map::new();
		object.insert("is".to_owned(), Json::String(status.as_str().to_owned()));
		if let Some(reason) = reason {
			object.insert("reason".to_owned(), Json::String(reason));
		}
		self.traits.insert(STATUS.to_owned(), Json::Object(object));
		if !status.carries_value() {
			self.value = None;
		}
	}

	/// This entry's status, as its `is` string. A status this build does not know is kept as it
	/// arrived, so an older reader colours it as if passed rather than crying wolf.
	pub fn status(&self) -> Option<&str> {
		self.traits.get(STATUS)?.get("is")?.as_str()
	}

	/// The reason on the status, where there is one.
	pub fn reason(&self) -> Option<&str> {
		self.traits.get(STATUS)?.get("reason")?.as_str()
	}

	/// Write this entry into a message map, naming it against the given member (`fact` or
	/// `measurement`). No member it holds is skipped, so the round trip the envelope depends on stays
	/// lossless.
	pub(super) fn write_into(&self, name_member: &str, map: &mut Map<String, Json>) {
		map.insert("at".to_owned(), Json::Number(self.at.into()));
		map.insert(name_member.to_owned(), Json::String(self.name.clone()));
		map.insert("traits".to_owned(), Json::Object(self.traits.clone()));
		map.insert("kind".to_owned(), Json::String(self.kind.clone()));
		if let Some(unit) = &self.unit {
			map.insert("unit".to_owned(), Json::String(unit.clone()));
		}
		if let Some(value) = &self.value {
			map.insert("value".to_owned(), value.clone());
		}
	}

	/// Read an entry from a message map, taking its name from the given member. The members a fact and
	/// a reading required when they were defined are checked here; a missing one is a malformed
	/// message rather than a newer peer (MSG).
	pub(super) fn read_from(name_member: &str, map: &Map<String, Json>) -> Result<Self, String> {
		let at = map
			.get("at")
			.and_then(Json::as_u64)
			.ok_or("an entry carries a number `at`")?;
		let name = map
			.get(name_member)
			.and_then(Json::as_str)
			.ok_or_else(|| format!("an entry carries a string `{name_member}`"))?
			.to_owned();
		let traits = match map.get("traits") {
			Some(Json::Object(traits)) => traits.clone(),
			_ => return Err("an entry carries an object `traits`".to_owned()),
		};
		let kind = map
			.get("kind")
			.and_then(Json::as_str)
			.ok_or("an entry carries a string `kind`")?
			.to_owned();
		let unit = match map.get("unit") {
			None | Some(Json::Null) => None,
			Some(Json::String(unit)) => Some(unit.clone()),
			Some(_) => return Err("`unit` is a string".to_owned()),
		};
		let value = map.get("value").cloned();
		Ok(Self {
			at,
			name,
			traits,
			kind,
			unit,
			value,
		})
	}
}

/// A JSON number, falling back to null for a value JSON cannot carry.
fn json_number(value: f64) -> Json {
	serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number)
}

/// Round to at most four decimal places. No reading this protocol carries is meaningful past four,
/// and rounding nearly halves what a stream of them costs compressed (NFO).
fn round4(value: f64) -> f64 {
	(value * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_passed_entry_carries_its_value_and_a_status() {
		let entry = Entry::fraction(20_308_140, "cpu-usage", 0.1234);
		assert_eq!(entry.status(), Some("passed"));
		assert!(entry.reason().is_none(), "passed carries no reason");
		assert_eq!(entry.value, Some(json_number(0.1234)));
	}

	/// An address that goes keeps its value, the only thing telling it from the interface's others,
	/// and any other entry that ends loses its value.
	#[test]
	fn an_ended_entry_keeps_its_value_only_where_the_value_tells_it_apart() {
		let address = Entry::address(1, "network-address", kind::IPV6, "fd00::1".to_owned());
		let ended = address.ended(2, "no longer applies");
		assert_eq!(ended.status(), Some("ended"));
		assert_eq!(ended.value, Some(Json::String("fd00::1".into())));

		let hotspot = Entry::text(1, "hotspot", "bliti");
		assert_eq!(hotspot.ended(2, "no longer applies").value, None);
	}

	#[test]
	fn a_numeric_value_is_rounded_to_four_places() {
		let entry = Entry::fraction(1, "cpu-usage", 0.123_456_789);
		assert_eq!(entry.value, Some(json_number(0.1235)));
		let quantity = Entry::quantity(1, "temperature", "celsius", 48.567_89);
		assert_eq!(quantity.value, Some(json_number(48.5679)));
	}

	#[test]
	fn a_warning_keeps_its_value_and_carries_a_reason() {
		let entry = Entry::quantity(1, "cpu-frequency", "hertz", 600_000_000.0)
			.warning("the platform is limiting the processor");
		assert_eq!(entry.status(), Some("warning"));
		assert_eq!(
			entry.reason(),
			Some("the platform is limiting the processor")
		);
		assert!(entry.value.is_some());
	}

	#[test]
	fn skipped_and_broken_carry_no_value_but_a_reason() {
		let skipped = Entry::skipped(
			1,
			"battery-direction",
			kind::TEXT,
			"not watched long enough yet",
		);
		assert_eq!(skipped.status(), Some("skipped"));
		assert_eq!(skipped.reason(), Some("not watched long enough yet"));
		assert!(skipped.value.is_none());

		let broken = Entry::broken(
			1,
			"temperature",
			kind::QUANTITY,
			"no answer from the sensor",
		);
		assert_eq!(broken.status(), Some("broken"));
		assert!(broken.value.is_none());
	}

	#[test]
	fn an_ended_entry_keeps_its_traits_and_drops_its_value() {
		let running = Entry::new(1, "hotspot", "text", serde_json::json!("clinic"))
			.with_trait("channel", serde_json::json!({"number": 6}));
		let ended = running.ended(2, "the hotspot stopped");
		assert_eq!(ended.status(), Some("ended"));
		assert_eq!(ended.reason(), Some("the hotspot stopped"));
		assert_eq!(ended.value, None);
		assert_eq!(ended.at, 2);
		assert_eq!(ended.traits.get("channel"), running.traits.get("channel"));
	}

	#[test]
	fn a_trait_that_qualifies_another_sits_inside_it() {
		let entry = Entry::address(1, "network-address", kind::IPV4, "192.168.1.42").with_trait(
			"interface",
			serde_json::json!({ "name": "eth0", "route": "default" }),
		);
		let interface = entry.traits.get("interface").unwrap();
		assert_eq!(
			interface.get("route").and_then(Json::as_str),
			Some("default")
		);
	}

	#[test]
	fn an_entry_round_trips_through_a_map() {
		let entry = Entry::quantity(
			20_308_140,
			"network-throughput",
			"bytes/second",
			1_200_000.0,
		)
		.with_trait("interface", serde_json::json!({ "name": "eth0" }))
		.with_trait("direction", Json::String("in".to_owned()));

		let mut map = Map::new();
		entry.write_into("measurement", &mut map);
		assert_eq!(
			map.get("measurement").and_then(Json::as_str),
			Some("network-throughput")
		);
		assert!(map.get("value").is_some());

		let read = Entry::read_from("measurement", &map).unwrap();
		assert_eq!(read, entry);
	}

	#[test]
	fn reading_an_entry_missing_what_it_requires_is_an_error() {
		let mut map = Map::new();
		map.insert(
			"measurement".to_owned(),
			Json::String("cpu-usage".to_owned()),
		);
		// No `at`, no `traits`, no `kind`.
		assert!(Entry::read_from("measurement", &map).is_err());
	}
}
