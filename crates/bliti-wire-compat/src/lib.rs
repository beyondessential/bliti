//! Wire compatibility checking: the protocol baseline against the current build.
//!
//! The technique is the oracle `cargo-semver-checks` applies to a Rust API, which never describes
//! the old surface but has the compiler adjudicate the old and the new together. There is nothing
//! to compile for a wire protocol, so the oracle is executed instead: this crate links the baseline
//! build of `bliti-core` alongside the current one and has each read what the other writes.
//!
//! Nothing here describes the wire shape, and that is the point. The message types are hand-written
//! serde, and the Rust struct does not match the wire in any case: `traits` is a raw JSON map, an
//! entry is named by `fact` or `measurement` depending on its message type, and critical casing is
//! applied by the envelope rather than by the type. Any extracted or hand-maintained schema would
//! be a second artefact free to drift from the first. This has none.
//!
//! # What the two directions require
//!
//! They are not symmetric, and the asymmetry is the definition of backward compatibility.
//!
//! - **baseline to current**: nothing the baseline wrote may be missing. The current build is never
//!   the older peer, so it must understand everything the baseline could say.
//! - **current to baseline**: must not be a [`Fault`](bliti_core::channel::envelope::Fault). The
//!   baseline is entitled to skip or refuse, which is forward compatibility working rather than a
//!   break.
//!
//! The first is containment and not equality: the current build re-serialising a message may add a
//! member of its own, which MSG permits, and demanding equality would fail on a legal addition.
//!
//! # What it does not catch
//!
//! Repurposing. A `unit` of `celsius` becoming one of `kelvin` parses identically in both builds,
//! faults nothing, and loses nothing. MSG forbids it and no mechanism here can see it; it stays a
//! matter for review.

use serde_json::Value as Json;

pub mod ledger;

/// What reading one message in one direction proved about the two builds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
	/// Read, and everything the sender wrote survived.
	Understood,
	/// Read, but the reader did not preserve these member paths.
	Lossy {
		/// What did not survive, by path within the message.
		lost: Vec<String>,
		/// The message as it was sent, for the failure to name.
		sent: String,
	},
	/// Passed over: a message type the reader does not know.
	Skipped,
	/// Not acted on: something critical the reader does not know.
	Refused,
	/// The reader says the sender is not speaking the protocol.
	Fault(String),
}

impl Verdict {
	/// Whether this verdict is a peer that cannot be spoken to.
	pub fn is_fault(&self) -> bool {
		matches!(self, Self::Fault(_))
	}
}

/// Member paths in `sent` that the round trip through the other build did not preserve.
///
/// Containment rather than equality: a reader adding members of its own is a legal addition under
/// MSG, and only what it *lost* says it failed to understand the sender.
///
/// This walk deliberately does not live in `bliti-core`, and should not be moved there. It is not
/// the same function as the envelope's own `collect_unknown`, which ignores scalar differences
/// because it answers only which members a build failed to understand, for the criticality refusal.
/// More importantly, an oracle's adjudicator must not be one of the things under test: were this in
/// `bliti-core`, it would have to be taken from the baseline or from the current build, and a change
/// to it would alter the comparison along with the thing compared.
pub fn omissions(sent: &[u8], back: &[u8]) -> Vec<String> {
	// Case-blind. A member name's case marks criticality, which the envelope lowercases on the way
	// in, so `A` returning as `a` is the same member and not a loss. Whether criticality itself
	// changed is the ledger's question.
	let sent = fold_case(serde_json::from_slice(sent).expect("a written message is JSON"));
	let back = fold_case(serde_json::from_slice(back).expect("a written message is JSON"));
	let mut lost = Vec::new();
	walk(&sent, &back, &mut String::new(), &mut lost);
	lost
}

fn fold_case(value: Json) -> Json {
	match value {
		Json::Object(members) => Json::Object(
			members
				.into_iter()
				.map(|(name, value)| (name.to_ascii_lowercase(), fold_case(value)))
				.collect(),
		),
		Json::Array(items) => Json::Array(items.into_iter().map(fold_case).collect()),
		other => other,
	}
}

fn walk(sent: &Json, back: &Json, path: &mut String, out: &mut Vec<String>) {
	match (sent, back) {
		(Json::Object(sent), Json::Object(back)) => {
			for (name, value) in sent {
				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(name);
				match back.get(name) {
					None => out.push(path.clone()),
					Some(back) => walk(value, back, path, out),
				}
				path.truncate(restore);
			}
		}
		(Json::Array(sent), Json::Array(back)) => {
			for (index, value) in sent.iter().enumerate() {
				let restore = path.len();
				if !path.is_empty() {
					path.push('.');
				}
				path.push_str(&index.to_string());
				match back.get(index) {
					None => out.push(path.clone()),
					Some(back) => walk(value, back, path, out),
				}
				path.truncate(restore);
			}
		}
		(sent, back) if sent != back => {
			out.push(format!("{path} (value changed: {sent} to {back})"));
		}
		_ => {}
	}
}

/// Read `sent` with the current build and report what it made of it.
///
/// `sent` is one message as the baseline wrote it, whether by the baseline build itself or as
/// recorded in the corpus.
pub fn current_reads(sent: &[u8]) -> Verdict {
	use bliti_core::channel::{
		envelope::{Reading, read},
		messages::Message,
	};
	match read::<Message>(sent) {
		Err(fault) => Verdict::Fault(fault.to_string()),
		Ok(Reading::Skipped(_)) => Verdict::Skipped,
		Ok(Reading::Refused(_)) => Verdict::Refused,
		Ok(Reading::Message(message)) => verdict_of(sent, &message.to_json()),
	}
}

/// Read `sent` with the baseline build and report what it made of it.
pub fn baseline_reads(sent: &[u8]) -> Verdict {
	use bliti_core_baseline::channel::{
		envelope::{Reading, read},
		messages::Message,
	};
	match read::<Message>(sent) {
		Err(fault) => Verdict::Fault(fault.to_string()),
		Ok(Reading::Skipped(_)) => Verdict::Skipped,
		Ok(Reading::Refused(_)) => Verdict::Refused,
		Ok(Reading::Message(message)) => verdict_of(sent, &message.to_json()),
	}
}

fn verdict_of(sent: &[u8], back: &[u8]) -> Verdict {
	let lost = omissions(sent, back);
	if lost.is_empty() {
		Verdict::Understood
	} else {
		Verdict::Lossy {
			lost,
			sent: String::from_utf8_lossy(sent).into_owned(),
		}
	}
}

/// Whether the two builds are at the same version marker.
///
/// Where they are not, no client would derive a handle for the other and the two ends never speak,
/// so every question this crate asks is vacuous (VER). The checks report themselves skipped rather
/// than passing quietly.
pub fn markers_agree() -> bool {
	bliti_core::key_schedule::VERSION == bliti_core_baseline::key_schedule::VERSION
}

/// The recorded corpus: messages the baseline emits, as it wrote them.
///
/// Raw JSON rather than a generator seed, so that a case replays verbatim through any build and
/// outlives both the generator that found it and the baseline that produced it.
pub fn corpus() -> Vec<(String, Vec<u8>)> {
	let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/corpus");
	let mut entries: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
		.expect("the corpus directory is present")
		.filter_map(|entry| {
			let path = entry.expect("a corpus entry is readable").path();
			if path.extension()? != "json" {
				return None;
			}
			let name = path.file_name()?.to_string_lossy().into_owned();
			Some((
				name,
				std::fs::read(&path).expect("a corpus entry is readable"),
			))
		})
		.collect();
	entries.sort();
	entries
}
