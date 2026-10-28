//! The case's facts on the feed (NFO, "Case").

use serde_json::Value as Json;

use super::*;
use crate::facts::case;

fn fact<'a>(batch: &'a [Message], name: &str) -> Option<&'a Entry> {
	batch.iter().find_map(|message| match message {
		Message::Fact(entry) if entry.name == name => Some(entry),
		_ => None,
	})
}

// spec: NFO
#[tokio::test]
async fn an_opening_goes_out_at_once_with_its_latest_opening_in_one_batch() {
	let dir = tempfile::tempdir().unwrap();
	let (case, mover) = case::moved_by_hand(&dir.path().join("case-history.jsonl"), true);
	while case.entries(0).is_empty() {
		tokio::time::sleep(Duration::from_millis(10)).await;
	}
	let mut streams = paired_in(&keys(0x42), Controller::none(), Supply::default(), case).await;
	let mut feed = feed_of(&mut streams).await;

	mover.send(false).unwrap();

	// Well inside the facts poll: the case wakes the feed rather than waiting for it.
	let batches = batches_on(&mut feed, FACTS_POLL / 2).await;
	let batch = batches
		.iter()
		.find(|batch| fact(batch, "case-closed").is_some())
		.expect("the opening goes out before the next poll");
	assert_eq!(
		fact(batch, "case-closed").unwrap().value,
		Some(Json::Bool(false))
	);
	let opened = fact(batch, "case-last-opened").expect("in the same batch as the opening");
	assert_eq!(opened.traits.get("found"), Some(&Json::Bool(false)));
}
