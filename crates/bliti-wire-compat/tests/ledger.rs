//! The ledger records every member this build marks critical, and nothing else.

use bliti_wire_compat::ledger::{FILE, built, keys, recorded};

/// A member marked critical that the ledger does not record. Adding one breaks every older peer,
/// which is permitted and must be said out loud.
#[test]
fn every_critical_member_is_recorded() {
	let missing: Vec<_> = built().difference(&keys(&recorded())).cloned().collect();
	assert!(
		missing.is_empty(),
		"not recorded in {FILE}: {}\n\
		 Peers older than this build will refuse these message types outright. If that is \
		 intended, add each to {FILE} with a reason.",
		missing
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join("; ")
	);
}

/// A recorded member that is no longer critical. The ledger is a statement about this build, so a
/// stale entry is as wrong as a missing one.
#[test]
fn every_recorded_member_is_still_critical() {
	let stale: Vec<_> = keys(&recorded()).difference(&built()).cloned().collect();
	assert!(
		stale.is_empty(),
		"recorded in {FILE} but no longer critical: {}\n\
		 Remove each from {FILE}.",
		stale
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join("; ")
	);
}

/// A reason is the whole value of the entry over a bare list.
#[test]
fn every_entry_gives_a_reason() {
	for entry in recorded() {
		assert!(
			entry.reason.trim().len() > 20,
			"{} carries no real reason in {FILE}",
			entry.member
		);
	}
}
