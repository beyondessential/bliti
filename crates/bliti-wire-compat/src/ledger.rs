//! The ledger of members older peers will refuse.
//!
//! A member added in upper case is critical, and MSG permits adding one without moving the version
//! marker. It is a break all the same: every peer older than the change refuses that message type
//! outright rather than acting on a reading the sender has said is incomplete. That should be
//! possible, and impossible by accident.
//!
//! The oracle cannot ask this question. A refusal is legal, so there is nothing for it to fail on.
//! The ledger is what turns "permitted" into "permitted, but say so", which is the bargain
//! `#[expect(..., reason = "...")]` already strikes in this workspace.
//!
//! It is keyed on the code rather than on a run. Collecting the refusals the oracle happened to
//! observe would make it a function of random generation: flaky, and able to pass by luck. It is
//! keyed instead on [`MessageSet::critical_members`], a `&'static` list and a property of the
//! build.
//!
//! Only top-level members appear. The envelope uppercases nothing below the top of a message, so
//! this build cannot emit a nested critical member; were that ever wanted, entries would need
//! member paths rather than names.

use std::{collections::BTreeSet, fmt};

use bliti_core::channel::{envelope::MessageSet, messages::Message};
use serde::Deserialize;

/// Where the ledger lives, relative to the workspace root.
pub const FILE: &str = "wire-breaks.toml";

/// One acknowledged break: a member this build marks critical.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Entry {
	/// The message type carrying it.
	pub message: String,
	/// The member, named in lower case as the type declares it.
	pub member: String,
	/// Why it is critical, in the author's own words.
	pub reason: String,
}

/// A member this build marks critical, as the ledger keys them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key(pub String, pub String);

impl fmt::Display for Key {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{} carries critical `{}`", self.0, self.1)
	}
}

#[derive(Debug, Deserialize)]
struct File {
	#[serde(default)]
	critical: Vec<Entry>,
}

/// Read the ledger from the workspace root.
pub fn recorded() -> Vec<Entry> {
	let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../", "wire-breaks.toml");
	let text = std::fs::read_to_string(path).expect("the ledger is present at the workspace root");
	let file: File = toml::from_str(&text).expect("the ledger is well-formed TOML");
	file.critical
}

/// What the current build actually marks critical, across every type it knows.
pub fn built() -> BTreeSet<Key> {
	Message::known_types()
		.iter()
		.flat_map(|type_name| {
			Message::critical_members(type_name)
				.iter()
				.map(move |member| Key((*type_name).to_owned(), (*member).to_owned()))
		})
		.collect()
}

/// The keys the ledger records.
pub fn keys(entries: &[Entry]) -> BTreeSet<Key> {
	entries
		.iter()
		.map(|entry| Key(entry.message.clone(), entry.member.clone()))
		.collect()
}
