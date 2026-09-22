//! Strategies generating conforming application messages.
//!
//! These exist for the wire compatibility oracle of `bliti-wire-compat`, which links the baseline
//! build of this crate alongside the current one and has each read what the other writes. The
//! generator lives here rather than in the oracle so that each build brings its own: the baseline
//! generates what it could say, the current build what it can say, and a message type added to
//! [`Message`] is exercised without anyone remembering to extend a list elsewhere.
//!
//! Conformance is the substance of it. The envelope faults on a member name that is mixed case,
//! that carries anything but letters, digits and hyphens, or that appears twice once lowercased. A
//! generator ignoring those rules produces messages both builds reject identically, which an oracle
//! reads as a compatibility failure rather than as the malformed input it is. So the naming rules
//! belong here, at every depth, inside `traits` and inside any generated value.
//!
//! The fields of [`Entry`] are public, so these build by struct literal and reach the whole
//! representable set rather than only what the constructors compose: `unit` is set by
//! [`Entry::quantity`] alone, but any entry may carry one, and the browser client or a third
//! implementation may send one that does. Some generated shapes therefore look semantically odd.
//! That is the intent: the wire admits them.

use std::collections::BTreeSet;

use proptest::prelude::*;
use serde_json::{Map, Value as Json};

use super::{messages::Message, readings::Entry};

/// A well-formed member name: letters, digits and hyphens, wholly one case.
///
/// Both cases are generated. An upper case name is critical, and nesting one inside `traits` is a
/// shape a peer may legitimately send.
fn member_name() -> impl Strategy<Value = String> {
	prop::collection::vec(
		prop::sample::select(vec!['a', 'b', 'c', 'x', '9', '-']),
		1..8,
	)
	.prop_map(|cs| cs.into_iter().collect::<String>())
	.prop_flat_map(|name| {
		prop_oneof![
			Just(name.to_ascii_lowercase()),
			Just(name.to_ascii_uppercase())
		]
	})
}

/// An object whose member names conform and do not collide once lowercased.
///
/// The collision matters: `foo` and `FOO` are one member, and a message carrying both is malformed
/// rather than incompatible.
fn object_of(
	value: impl Strategy<Value = Json> + 'static,
) -> impl Strategy<Value = Map<String, Json>> {
	prop::collection::vec((member_name(), value), 0..4).prop_map(|entries| {
		let mut seen = BTreeSet::new();
		let mut object = Map::new();
		for (name, value) in entries {
			if seen.insert(name.to_ascii_lowercase()) {
				object.insert(name, value);
			}
		}
		object
	})
}

/// JSON whose every object member name conforms, at any depth.
fn json() -> impl Strategy<Value = Json> {
	let leaf = prop_oneof![
		Just(Json::Null),
		any::<bool>().prop_map(Json::Bool),
		any::<i32>().prop_map(|number| Json::Number(number.into())),
		// Finite and rounded as a reading's own values are, so that what is generated is what a
		// sender could have produced.
		(-1e6f64..1e6).prop_map(|number| {
			serde_json::Number::from_f64((number * 10_000.0).round() / 10_000.0)
				.map_or(Json::Null, Json::Number)
		}),
		"[a-z ]{0,12}".prop_map(Json::String),
	];
	leaf.prop_recursive(3, 12, 3, |inner| {
		prop_oneof![
			prop::collection::vec(inner.clone(), 0..3).prop_map(Json::Array),
			object_of(inner).prop_map(Json::Object),
		]
	})
}

/// One fact or reading, over the whole representable set.
pub fn entry() -> impl Strategy<Value = Entry> {
	(
		any::<u64>(),
		"[a-z][a-z-]{0,14}",
		object_of(json()),
		// A kind outside the catalogue is included: the vocabulary is open, and a reader meeting one
		// renders the value stringified (NFO).
		prop::sample::select(vec![
			"text",
			"fraction",
			"quantity",
			"duration",
			"datetime",
			"ipv4",
			"ipv6",
			"novel-kind",
		]),
		prop::option::of("[a-z/]{1,12}"),
		prop::option::of(json()),
	)
		.prop_map(|(at, name, traits, kind, unit, value)| Entry {
			at,
			name,
			traits,
			kind: kind.to_owned(),
			unit,
			value,
		})
}

/// One application message, over every type this build knows.
///
/// A type added to [`Message`] belongs here too. The oracle's reach is exactly this strategy's
/// reach, and a type left out is a type never checked.
pub fn message() -> impl Strategy<Value = Message> {
	prop_oneof![
		("[a-z-]{1,10}", "[0-9.]{1,8}")
			.prop_map(|(name, version)| Message::Hello { name, version }),
		"[a-z-]{1,10}".prop_map(|topic| Message::Subscribe { topic }),
		entry().prop_map(Message::Fact),
		entry().prop_map(Message::Reading),
		// The configuration session of CFG. The document, capabilities and act-answer payloads ride as
		// raw JSON, so they are generated over the same conforming-JSON strategy as everything else.
		Just(Message::Configure),
		(object_of(json()), prop::option::of(object_of(json()))).prop_map(
			|(document, capabilities)| Message::Configuration {
				document,
				capabilities,
			}
		),
		Just(Message::Applied),
		(
			"[a-z][a-z.0-9-]{0,16}",
			"[a-z ]{1,20}",
			prop::option::of("[a-z ]{1,16}"),
		)
			.prop_map(|(at, reason, reached)| Message::Invalid {
				at,
				reason,
				reached
			}),
		Just(Message::Confirm),
		Just(Message::Discard),
		Just(Message::Busy),
		Just(Message::Scan),
		Just(Message::Survey),
		prop::sample::select(vec!["push-button", "pin"]).prop_map(|method| Message::Wps {
			method: method.to_owned()
		}),
		prop::collection::vec(json(), 0..3).prop_map(|networks| Message::Networks { networks }),
		object_of(json()).prop_map(|spectrum| Message::Spectrum { spectrum }),
	]
}

#[cfg(test)]
mod tests {
	use proptest::{strategy::ValueTree, test_runner::TestRunner};

	use super::{
		super::envelope::{MessageSet, Reading, read},
		*,
	};

	fn sample(count: usize) -> Vec<Message> {
		let mut runner = TestRunner::deterministic();
		let strategy = message();
		(0..count)
			.map(|_| strategy.new_tree(&mut runner).unwrap().current())
			.collect()
	}

	/// Every generated message is one this build reads without fault, skip or refusal. A generator
	/// producing anything else would have the oracle report a compatibility failure for a message no
	/// peer would have sent.
	///
	/// Read rather than round-tripped: the envelope lowercases member names, so a message carrying a
	/// critical trait does not return byte for byte. Whether the other build preserved what this one
	/// sent is the oracle's question, and it folds case to ask it.
	#[test]
	fn everything_generated_is_conforming() {
		for message in sample(2000) {
			let reading = read::<Message>(&message.to_json());
			assert!(
				matches!(reading, Ok(Reading::Message(_))),
				"{message:?} read as {reading:?}"
			);
		}
	}

	/// The oracle is worth its coverage of the optional members, which is where a removal hides: a
	/// member absent from every generated message is one whose loss nothing would notice.
	#[test]
	fn the_optional_members_are_reached() {
		let (mut unit, mut no_value, mut traits, mut critical_trait) = (0, 0, 0, 0);
		for message in sample(2000) {
			let (Message::Fact(entry) | Message::Reading(entry)) = &message else {
				continue;
			};
			if entry.unit.is_some() {
				unit += 1;
			}
			if entry.value.is_none() {
				no_value += 1;
			}
			if !entry.traits.is_empty() {
				traits += 1;
			}
			if entry
				.traits
				.keys()
				.any(|name| name.chars().any(|c| c.is_ascii_uppercase()))
			{
				critical_trait += 1;
			}
		}
		assert!(unit > 100, "entries carrying a unit: {unit}");
		assert!(no_value > 100, "entries without a value: {no_value}");
		assert!(traits > 100, "entries carrying traits: {traits}");
		assert!(
			critical_trait > 100,
			"entries carrying a critical trait: {critical_trait}"
		);
	}

	/// Every type this build knows is generated. A type in the set but not in the strategy is a type
	/// the oracle never exercises.
	#[test]
	fn every_known_type_is_generated() {
		let generated: BTreeSet<String> = sample(2000)
			.iter()
			.map(|message| {
				let json: Json = serde_json::from_slice(&message.to_json()).unwrap();
				json.get("type").unwrap().as_str().unwrap().to_owned()
			})
			.collect();
		for known in Message::known_types() {
			assert!(generated.contains(*known), "{known} is never generated");
		}
	}
}
