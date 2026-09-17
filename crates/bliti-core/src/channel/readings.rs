//! Self-describing readings: what a device reports about itself, carrying its own meaning.
//!
//! Behaviour is specified in BLI-SYS. A reading names itself, says what it measures and in what
//! unit, and a client renders it from that alone. The point is that a device which gains a reading
//! appears in a client that has never heard of it, with no client release in between.
//!
//! Every shape here preserves what it does not recognise rather than dropping it: an unknown value
//! kind keeps its members, and an unknown state or direction keeps its name. The envelope finds
//! unknown members by round-tripping a parsed message back to JSON (see [`super::envelope`]), so a
//! type that dropped what it did not understand would make a peer's ordinary newer member look like
//! something to refuse.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::{Map, Value as Json};

/// One thing a device reports about itself.
///
/// Carries either a [`Reading::value`] or an [`Reading::error`], never both and never neither: a
/// reading is a measurement or an account of why there is none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
	/// Stable identifier, lower case with hyphens. Never reused for a different quantity.
	pub name: String,

	/// What to call the reading in an interface. Prose, and free to change between versions.
	pub label: String,

	/// The headline value.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub value: Option<Value>,

	/// Further values, revealed behind the headline.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub detail: Vec<Detail>,

	/// Plain prose about what the reading means, for a reading that is routinely misread.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub note: Option<String>,

	/// Whether the reading is in difficulty. Absent means [`State::Ok`].
	#[serde(default, skip_serializing_if = "State::is_ok")]
	pub state: State,

	/// Marks on the reading's scale, such as a board's declared thresholds.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub limits: Vec<Limit>,

	/// Ties this reading to others for display. A client that ignores it shows them separately and is
	/// still correct.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub group: Option<String>,

	/// Which way a flow runs, for a reading that measures one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub direction: Option<Direction>,

	/// Why the reading could not be taken. Present only where there is no value.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

impl Reading {
	/// A reading carrying a value.
	pub fn new(name: impl Into<String>, label: impl Into<String>, value: Value) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			value: Some(value),
			detail: Vec::new(),
			note: None,
			state: State::Ok,
			limits: Vec::new(),
			group: None,
			direction: None,
			error: None,
		}
	}

	/// A reading the device declares but could not take. Hardware that is not fitted is left out
	/// entirely instead; this is for hardware that is there and did not answer.
	pub fn failed(
		name: impl Into<String>,
		label: impl Into<String>,
		why: impl fmt::Display,
	) -> Self {
		Self {
			name: name.into(),
			label: label.into(),
			value: None,
			detail: Vec::new(),
			note: None,
			state: State::Fault,
			limits: Vec::new(),
			group: None,
			direction: None,
			error: Some(why.to_string()),
		}
	}

	/// Add a detail entry, revealed behind the headline.
	#[must_use]
	pub fn with_detail(mut self, label: impl Into<String>, value: Value) -> Self {
		self.detail.push(Detail {
			label: label.into(),
			value,
		});
		self
	}

	/// Add prose about what the reading means.
	#[must_use]
	pub fn with_note(mut self, note: impl Into<String>) -> Self {
		self.note = Some(note.into());
		self
	}

	/// Set whether the reading is in difficulty.
	#[must_use]
	pub fn with_state(mut self, state: State) -> Self {
		self.state = state;
		self
	}

	/// Add a mark on the reading's scale.
	#[must_use]
	pub fn with_limit(mut self, at: f64, label: impl Into<String>) -> Self {
		self.limits.push(Limit {
			at,
			label: label.into(),
		});
		self
	}

	/// Tie this reading to others for display.
	#[must_use]
	pub fn in_group(mut self, group: impl Into<String>) -> Self {
		self.group = Some(group.into());
		self
	}

	/// Say which way this reading's flow runs.
	#[must_use]
	pub fn flowing(mut self, direction: Direction) -> Self {
		self.direction = Some(direction);
		self
	}

	/// Whether the reading holds together: a measurement or an account of why there is none, and a
	/// failed reading marked as one.
	pub fn is_coherent(&self) -> bool {
		match (&self.value, &self.error) {
			(Some(_), None) => true,
			(None, Some(_)) => self.state == State::Fault,
			_ => false,
		}
	}
}

/// One value behind a reading's headline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detail {
	/// What to call it.
	pub label: String,
	/// What it is.
	pub value: Value,
}

/// A mark on a reading's scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Limit {
	/// Where on the scale the mark sits.
	pub at: f64,
	/// What the mark means.
	pub label: String,
}

/// A measurement, carrying enough to be rendered by a client that knows nothing about it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
	/// A proportion of a whole, from 0 to 1 inclusive.
	Fraction(f64),

	/// A measurement in a unit, optionally with the top of its scale.
	Quantity {
		/// The measurement.
		number: f64,
		/// What it is measured in.
		unit: String,
		/// The top of its scale, where it has one. A quantity without one is never drawn against a
		/// scale.
		max: Option<f64>,
	},

	/// An elapsed time, in seconds.
	Duration(f64),

	/// A string with no numeric meaning.
	Text(String),

	/// A kind this build does not know, kept as it arrived.
	///
	/// A client renders the reading's label alone. The members are held so the value survives the
	/// round trip the envelope's unknown-member detection depends on.
	Unknown(Map<String, Json>),
}

impl Value {
	/// A quantity with no ceiling.
	pub fn quantity(number: f64, unit: impl Into<String>) -> Self {
		Self::Quantity {
			number,
			unit: unit.into(),
			max: None,
		}
	}

	/// A quantity with the top of its scale, which a client may draw against.
	pub fn scaled(number: f64, unit: impl Into<String>, max: f64) -> Self {
		Self::Quantity {
			number,
			unit: unit.into(),
			max: Some(max),
		}
	}

	/// Text with no numeric meaning.
	pub fn text(text: impl Into<String>) -> Self {
		Self::Text(text.into())
	}

	/// Whether this value has a scale a client may draw against.
	pub fn has_scale(&self) -> bool {
		match self {
			Self::Fraction(_) => true,
			Self::Quantity { max, .. } => max.is_some(),
			_ => false,
		}
	}
}

impl Serialize for Value {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		let mut map = Map::new();
		match self {
			Self::Fraction(number) => {
				map.insert("kind".into(), "fraction".into());
				map.insert("number".into(), json_number(*number));
			}
			Self::Quantity { number, unit, max } => {
				map.insert("kind".into(), "quantity".into());
				map.insert("number".into(), json_number(*number));
				map.insert("unit".into(), unit.clone().into());
				if let Some(max) = max {
					map.insert("max".into(), json_number(*max));
				}
			}
			Self::Duration(seconds) => {
				map.insert("kind".into(), "duration".into());
				map.insert("seconds".into(), json_number(*seconds));
			}
			Self::Text(text) => {
				map.insert("kind".into(), "text".into());
				map.insert("text".into(), text.clone().into());
			}
			// Written back exactly as it arrived, including its `kind`.
			Self::Unknown(raw) => map = raw.clone(),
		}
		map.serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for Value {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let raw = Map::deserialize(deserializer)?;
		let Some(Json::String(kind)) = raw.get("kind") else {
			return Err(de::Error::custom("a value carries a string `kind`"));
		};

		// A kind this build does not know is not an error: it is a device newer than this client, and
		// BLI-SYS says the reading renders as its label alone rather than the message failing.
		match kind.as_str() {
			"fraction" => Ok(Self::Fraction(number(&raw, "number")?)),
			"quantity" => Ok(Self::Quantity {
				number: number(&raw, "number")?,
				unit: string(&raw, "unit")?,
				max: match raw.get("max") {
					None | Some(Json::Null) => None,
					Some(_) => Some(number(&raw, "max")?),
				},
			}),
			"duration" => Ok(Self::Duration(number(&raw, "seconds")?)),
			"text" => Ok(Self::Text(string(&raw, "text")?)),
			_ => Ok(Self::Unknown(raw)),
		}
	}
}

/// A JSON number, falling back to null for a value JSON cannot carry.
fn json_number(value: f64) -> Json {
	serde_json::Number::from_f64(value).map_or(Json::Null, Json::Number)
}

fn number<E: de::Error>(raw: &Map<String, Json>, member: &str) -> Result<f64, E> {
	raw.get(member).and_then(Json::as_f64).ok_or_else(|| {
		de::Error::custom(format!("a value of this kind carries a number `{member}`"))
	})
}

fn string<E: de::Error>(raw: &Map<String, Json>, member: &str) -> Result<String, E> {
	raw.get(member)
		.and_then(Json::as_str)
		.map(ToOwned::to_owned)
		.ok_or_else(|| {
			de::Error::custom(format!("a value of this kind carries a string `{member}`"))
		})
}

/// Whether a reading is in difficulty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum State {
	/// Nothing the matter.
	#[default]
	Ok,
	/// Worth an operator's attention.
	Warn,
	/// Broken, or unreadable.
	Fault,
	/// A state this build does not know, kept as it arrived. Treated as [`State::Ok`].
	Other(String),
}

impl State {
	/// Whether this is the absent-means-ok case, which is not written to the wire.
	pub fn is_ok(&self) -> bool {
		matches!(self, Self::Ok)
	}

	/// Whether an operator should be drawn to this reading. A state this build does not know is not
	/// treated as trouble: a newer device must not make an older client cry wolf.
	pub fn is_trouble(&self) -> bool {
		matches!(self, Self::Warn | Self::Fault)
	}

	fn as_str(&self) -> &str {
		match self {
			Self::Ok => "ok",
			Self::Warn => "warn",
			Self::Fault => "fault",
			Self::Other(name) => name,
		}
	}
}

/// Which way a flow runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Direction {
	/// Into the device.
	In,
	/// Out of the device.
	Out,
	/// A direction this build does not know, kept as it arrived.
	Other(String),
}

impl Direction {
	fn as_str(&self) -> &str {
		match self {
			Self::In => "in",
			Self::Out => "out",
			Self::Other(name) => name,
		}
	}

	/// Whether these two are opposed, which is what lets a client draw them mirrored.
	pub fn opposes(&self, other: &Self) -> bool {
		matches!((self, other), (Self::In, Self::Out) | (Self::Out, Self::In))
	}
}

/// A string-backed enum that keeps a name it does not know, so the value survives the round trip.
macro_rules! string_enum {
	($type:ty, $($name:literal => $variant:expr),+ $(,)?) => {
		impl Serialize for $type {
			fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
				serializer.serialize_str(self.as_str())
			}
		}

		impl<'de> Deserialize<'de> for $type {
			fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
				let name = String::deserialize(deserializer)?;
				Ok(match name.as_str() {
					$($name => $variant,)+
					_ => Self::Other(name),
				})
			}
		}
	};
}

string_enum!(State, "ok" => Self::Ok, "warn" => Self::Warn, "fault" => Self::Fault);
string_enum!(Direction, "in" => Self::In, "out" => Self::Out);

/// One sample: every reading taken at one moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
	/// Milliseconds since the device booted, when the sample was taken.
	///
	/// Measured from boot rather than from an epoch, because a device in the field may have no set
	/// clock. Meaningful only against other times from the same device.
	pub at: u64,
	/// The readings taken. Need not carry every reading, and the set may differ between samples.
	pub readings: Vec<Reading>,
}

#[cfg(test)]
mod tests {
	use super::*;

	fn round_trip(value: &Value) -> Value {
		let json = serde_json::to_string(value).unwrap();
		serde_json::from_str(&json).unwrap()
	}

	#[test]
	fn every_value_kind_round_trips() {
		for value in [
			Value::Fraction(0.125),
			Value::quantity(4.19, "V"),
			Value::scaled(48.5, "°C", 110.0),
			Value::Duration(20308.0),
			Value::text("Mains"),
		] {
			assert_eq!(round_trip(&value), value, "{value:?}");
		}
	}

	#[test]
	fn a_quantity_without_max_omits_it_rather_than_writing_null() {
		let json = serde_json::to_string(&Value::quantity(1.5, "A")).unwrap();
		assert!(!json.contains("max"), "{json}");
		assert!(!json.contains("null"), "{json}");
	}

	/// A kind this build has never heard of leaves the reading readable as a label, and is written
	/// back exactly as it arrived so the envelope does not mistake its members for unknown ones.
	#[test]
	fn an_unknown_kind_is_kept_verbatim() {
		let json = r#"{"kind":"pressure","pascals":101325.0,"sensor":"bmp280"}"#;
		let value: Value = serde_json::from_str(json).unwrap();
		assert!(matches!(value, Value::Unknown(_)));
		assert!(!value.has_scale());

		let written: Json = serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
		let original: Json = serde_json::from_str(json).unwrap();
		assert_eq!(written, original);
	}

	#[test]
	fn a_value_without_a_kind_is_rejected() {
		assert!(serde_json::from_str::<Value>(r#"{"number":1}"#).is_err());
	}

	#[test]
	fn a_known_kind_missing_what_it_carries_is_rejected() {
		assert!(serde_json::from_str::<Value>(r#"{"kind":"fraction"}"#).is_err());
		assert!(serde_json::from_str::<Value>(r#"{"kind":"quantity","number":1}"#).is_err());
		assert!(serde_json::from_str::<Value>(r#"{"kind":"text","text":42}"#).is_err());
	}

	/// Only a fraction and a quantity that named its ceiling may be drawn against a scale.
	#[test]
	fn a_scale_exists_only_where_there_is_a_ceiling() {
		assert!(Value::Fraction(0.5).has_scale());
		assert!(Value::scaled(1.0, "A", 5.0).has_scale());
		assert!(!Value::quantity(1.0, "A").has_scale());
		assert!(!Value::text("Mains").has_scale());
		assert!(!Value::Duration(1.0).has_scale());
	}

	#[test]
	fn an_unknown_state_is_kept_and_is_not_treated_as_trouble() {
		let state: State = serde_json::from_str(r#""degraded""#).unwrap();
		assert_eq!(state, State::Other("degraded".to_owned()));
		assert!(!state.is_trouble());
		assert_eq!(serde_json::to_string(&state).unwrap(), r#""degraded""#);
	}

	#[test]
	fn the_states_this_build_knows_round_trip() {
		for state in [State::Ok, State::Warn, State::Fault] {
			let json = serde_json::to_string(&state).unwrap();
			assert_eq!(serde_json::from_str::<State>(&json).unwrap(), state);
		}
		assert!(State::Warn.is_trouble());
		assert!(State::Fault.is_trouble());
		assert!(!State::Ok.is_trouble());
	}

	#[test]
	fn an_unknown_direction_is_kept_and_opposes_nothing() {
		let direction: Direction = serde_json::from_str(r#""sideways""#).unwrap();
		assert_eq!(direction, Direction::Other("sideways".to_owned()));
		assert!(!direction.opposes(&Direction::In));
		assert!(!Direction::In.opposes(&direction));
	}

	#[test]
	fn only_in_and_out_oppose_each_other() {
		assert!(Direction::In.opposes(&Direction::Out));
		assert!(Direction::Out.opposes(&Direction::In));
		assert!(!Direction::In.opposes(&Direction::In));
	}

	/// An ok state is the absent case and is not written, so the common reading stays small.
	#[test]
	fn an_ok_state_is_not_written() {
		let json =
			serde_json::to_string(&Reading::new("cpu", "CPU", Value::Fraction(0.1))).unwrap();
		assert!(!json.contains("state"), "{json}");
		assert!(!json.contains("detail"), "{json}");
		assert!(!json.contains("note"), "{json}");
	}

	#[test]
	fn a_reading_round_trips_with_everything_set() {
		let reading = Reading::new(
			"temperature",
			"Temperature",
			Value::scaled(48.5, "°C", 110.0),
		)
		.with_detail("Disk", Value::quantity(37.8, "°C"))
		.with_note("This is the processor core, not the case or the room.")
		.with_state(State::Warn)
		.with_limit(75.0, "Cooling")
		.in_group("thermal")
		.flowing(Direction::Out);
		let json = serde_json::to_string(&reading).unwrap();
		assert_eq!(serde_json::from_str::<Reading>(&json).unwrap(), reading);
	}

	#[test]
	fn a_failed_reading_carries_a_reason_and_no_value() {
		let reading = Reading::failed("battery", "Battery", "no answer from the gauge at 0x36");
		assert!(reading.is_coherent());
		assert_eq!(reading.state, State::Fault);
		assert!(reading.value.is_none());
		assert!(reading.error.is_some());
	}

	/// A measurement or an account of why there is none, never both and never neither.
	#[test]
	fn a_reading_carrying_neither_or_both_is_incoherent() {
		let mut reading = Reading::new("cpu", "CPU", Value::Fraction(0.1));
		assert!(reading.is_coherent());

		reading.value = None;
		assert!(!reading.is_coherent(), "neither");

		reading.value = Some(Value::Fraction(0.1));
		reading.error = Some("also broken".to_owned());
		assert!(!reading.is_coherent(), "both");
	}

	/// An error without the fault state would leave a client colouring a broken reading as well.
	#[test]
	fn a_failed_reading_must_be_marked_as_one() {
		let mut reading = Reading::failed("battery", "Battery", "no answer");
		reading.state = State::Ok;
		assert!(!reading.is_coherent());
	}

	/// A member this build has never heard of must reach the envelope as an unknown member, which
	/// skips it. Denying it here would turn a newer device into a fault instead.
	#[test]
	fn an_unknown_member_inside_a_reading_is_tolerated() {
		let json =
			r#"{"name":"cpu","label":"CPU","value":{"kind":"fraction","number":0.1},"cores":4}"#;
		let reading: Reading = serde_json::from_str(json).unwrap();
		assert_eq!(reading.name, "cpu");

		let nested = r#"{"name":"cpu","label":"CPU","value":{"kind":"fraction","number":0.1,"precision":3}}"#;
		let reading: Reading = serde_json::from_str(nested).unwrap();
		assert_eq!(reading.value, Some(Value::Fraction(0.1)));
	}

	#[test]
	fn a_sample_carries_its_time_and_its_readings() {
		let sample = Sample {
			at: 20_308_140,
			readings: vec![Reading::new("cpu", "CPU", Value::Fraction(0.12))],
		};
		let json = serde_json::to_string(&sample).unwrap();
		assert_eq!(serde_json::from_str::<Sample>(&json).unwrap(), sample);
	}
}
