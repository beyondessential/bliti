//! The baseline build and the current one, each reading what the other writes.

use bliti_core::channel::generate::message;
use bliti_wire_compat::{
	Verdict, baseline_reads, baseline_snapshot, corpus, current_reads, markers_agree,
};
use proptest::prelude::*;

/// Where the two builds sit at different version markers, no client would derive a handle for the
/// other and the two ends never speak (VER), so every question below is vacuous. Said out loud
/// rather than passed over quietly, so that a marker bump does not look like a green check.
fn vacuous() -> bool {
	if markers_agree() {
		return false;
	}
	eprintln!(
		"skipped: the baseline is at version marker {} and this build at {}, so the two never speak",
		bliti_core_baseline::key_schedule::VERSION,
		bliti_core::key_schedule::VERSION,
	);
	true
}

/// Nothing the current build says may fault the baseline.
///
/// A skip or a refusal is not a break: it is an older peer meeting something newer and doing what
/// MSG tells it to. Only a fault says the sender has stopped speaking the protocol.
#[test]
fn nothing_the_current_build_says_faults_the_baseline() {
	if vacuous() {
		return;
	}
	proptest!(|(message in message())| {
		let sent = message.to_json();
		let verdict = baseline_reads(&sent);
		prop_assert!(
			!verdict.is_fault(),
			"the baseline cannot read this build's {}: {verdict:?}",
			String::from_utf8_lossy(&sent),
		);
	});
}

/// Everything the baseline says, the current build must understand losslessly.
///
/// The current build is never the older peer, so a skip, a refusal or a lost member is a break.
/// Driven by the recorded snapshot: the pinned baseline predates `bliti-core`'s generator, so it
/// cannot be asked to produce messages live. Moving the baseline to a revision built with
/// `generate` replaces this with generation.
#[test]
fn the_current_build_understands_everything_the_baseline_says() {
	if vacuous() {
		return;
	}
	let snapshot = baseline_snapshot();
	assert!(
		snapshot.len() > 100,
		"the snapshot is the only record of what the baseline emits, and it is nearly empty: {} messages",
		snapshot.len()
	);
	for (name, sent) in snapshot {
		assert_eq!(
			current_reads(&sent),
			Verdict::Understood,
			"this build does not fully understand the baseline's {name}"
		);
	}
}

/// Every regression the corpus records still reads losslessly.
///
/// Empty until a failure is worth keeping, and it stays checked so that adding one is enough.
#[test]
fn the_current_build_understands_the_regression_corpus() {
	if vacuous() {
		return;
	}
	for (name, sent) in corpus() {
		assert_eq!(
			current_reads(&sent),
			Verdict::Understood,
			"this build does not fully understand the recorded {name}"
		);
	}
}
