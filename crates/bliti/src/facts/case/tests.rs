use std::sync::mpsc;

use super::*;

const BOOT_ID: &str = "0f6b5b4e-3a52-4ad6-9d1c-2c3e4f5a6b7c";

fn clock() -> (Timestamp, u64) {
	(Timestamp::from_second(1_790_000_000).unwrap(), 4_321)
}

/// A switch a test moves by hand.
struct Fake {
	closed: Result<bool, String>,
	moves: mpsc::Receiver<bool>,
}

impl Switch for Fake {
	fn closed(&mut self) -> io::Result<bool> {
		self.closed.clone().map_err(io::Error::other)
	}

	fn moved(&mut self) -> io::Result<bool> {
		match self.moves.recv() {
			Ok(closed) => Ok(closed),
			// The test is done with it; leave the thread waiting rather than reporting a fault.
			Err(_) => loop {
				thread::park();
			},
		}
	}
}

/// A case watching a switch that starts `closed`, with its history at `path`, and the handle that
/// moves it.
pub(super) fn started(path: &Path, closed: Result<bool, &str>) -> (Case, mpsc::Sender<bool>) {
	let (mover, moves) = mpsc::channel();
	let mut moves = Some(moves);
	let closed = closed.map_err(str::to_owned);
	let case = Case::start(
		move || match moves.take() {
			Some(moves) => Ok(Fake {
				closed: closed.clone(),
				moves,
			}),
			None => Err(io::Error::other("opened once already")),
		},
		path.to_owned(),
		BOOT_ID.to_owned(),
		clock,
	)
	.unwrap();
	(case, mover)
}

/// Wait until the case has published a state satisfying `ready`.
async fn settled(case: &Case, ready: impl FnMut(&Option<State>) -> bool) {
	let mut changes = case.state.subscribe();
	tokio::time::timeout(Duration::from_secs(2), changes.wait_for(ready))
		.await
		.expect("the case published in time")
		.unwrap();
}

async fn closed_as(case: &Case, closed: bool) {
	settled(case, |state| {
		state
			.as_ref()
			.is_some_and(|state| state.closed == Ok(closed))
	})
	.await;
}

fn entry<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
	entries.iter().find(|entry| entry.name == name)
}

fn lines(path: &Path) -> Vec<Json> {
	fs::read_to_string(path)
		.unwrap_or_default()
		.lines()
		.map(|line| serde_json::from_str(line).unwrap())
		.collect()
}

fn write_history(path: &Path, events: &[Event]) {
	for event in events {
		history::append(
			path,
			&history::Entry {
				event: *event,
				time: Timestamp::from_second(1_780_000_000).unwrap(),
				boot: Boot {
					id: "an-earlier-boot".to_owned(),
					offset: 99,
				},
				found: false,
			},
		)
		.unwrap();
	}
}

#[test]
fn a_case_with_no_switch_reports_nothing() {
	assert!(Case::none().entries(1).is_empty());
}

// spec: NFO
#[tokio::test]
async fn a_closed_case_is_passed_and_an_open_one_a_warning() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, mover) = started(&path, Ok(true));

	closed_as(&case, true).await;
	let entries = case.entries(7);
	let closed = entry(&entries, CLOSED).unwrap();
	assert_eq!(closed.value, Some(Json::Bool(true)));
	assert_eq!(closed.status(), Some("passed"));
	assert!(
		entry(&entries, LAST_OPENED).is_none(),
		"no opening, nothing sent"
	);

	mover.send(false).unwrap();
	closed_as(&case, false).await;
	let entries = case.entries(8);
	let closed = entry(&entries, CLOSED).unwrap();
	assert_eq!(closed.value, Some(Json::Bool(false)));
	assert_eq!(closed.status(), Some("warning"));
	assert_eq!(closed.reason(), Some("the case is open"));
}

// spec: NFO
#[tokio::test]
async fn an_opening_and_a_closing_are_each_recorded() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, mover) = started(&path, Ok(true));
	closed_as(&case, true).await;

	mover.send(false).unwrap();
	closed_as(&case, false).await;
	mover.send(true).unwrap();
	closed_as(&case, true).await;

	let boot = serde_json::json!({"id": BOOT_ID, "offset": 4_321});
	assert_eq!(
		lines(&path),
		[
			serde_json::json!({"event": "opened", "time": "2026-09-21T14:13:20Z", "boot": boot, "found": false}),
			serde_json::json!({"event": "closed", "time": "2026-09-21T14:13:20Z", "boot": boot, "found": false}),
		]
	);
}

// spec: NFO
#[tokio::test]
async fn the_latest_opening_carries_its_boot_and_whether_it_was_found() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, mover) = started(&path, Ok(true));
	closed_as(&case, true).await;

	mover.send(false).unwrap();
	closed_as(&case, false).await;
	// Both facts come from the one snapshot, so the opening is there the moment the case reads open.
	let opened = entry(&case.entries(9), LAST_OPENED).cloned().unwrap();
	assert_eq!(opened.kind, kind::DATETIME);
	assert_eq!(opened.value, Some(Json::from("2026-09-21T14:13:20Z")));
	assert_eq!(opened.status(), Some("passed"));
	assert_eq!(
		opened.traits.get("boot"),
		Some(&serde_json::json!({"id": BOOT_ID, "offset": 4_321}))
	);
	assert_eq!(opened.traits.get("found"), Some(&Json::Bool(false)));

	// Closing leaves the latest opening as it was.
	mover.send(true).unwrap();
	closed_as(&case, true).await;
	assert_eq!(entry(&case.entries(9), LAST_OPENED), Some(&opened));
}

/// An edge that leaves the switch where it was is no movement: the kernel debounces, so this is the
/// only repeat that reaches here.
#[tokio::test]
async fn an_edge_that_leaves_the_switch_where_it_was_records_nothing() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, mover) = started(&path, Ok(true));
	closed_as(&case, true).await;

	mover.send(true).unwrap();
	mover.send(false).unwrap();
	closed_as(&case, false).await;
	assert_eq!(lines(&path).len(), 1);
}

// spec: NFO
#[tokio::test]
async fn a_switch_that_cannot_be_read_is_broken() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, _mover) = started(&path, Err("no GPIO line named GPIO17"));

	settled(&case, |state| {
		state.as_ref().is_some_and(|state| state.closed.is_err())
	})
	.await;
	let entries = case.entries(1);
	let closed = entry(&entries, CLOSED).unwrap();
	assert_eq!(closed.status(), Some("broken"));
	assert_eq!(
		closed.reason(),
		Some("cannot read the case switch: no GPIO line named GPIO17")
	);
	assert!(!path.exists(), "nothing is recorded of a switch never read");
}

// spec: NFO
#[tokio::test]
async fn a_history_that_cannot_be_read_breaks_the_latest_opening() {
	let dir = tempfile::tempdir().unwrap();
	// A directory where the file should be: there, and unreadable as a history.
	let path = dir.path().join("case-history.jsonl");
	fs::create_dir(&path).unwrap();
	let (case, _mover) = started(&path, Ok(false));

	closed_as(&case, false).await;
	let entries = case.entries(1);
	assert_eq!(entry(&entries, CLOSED).unwrap().status(), Some("warning"));
	let opened = entry(&entries, LAST_OPENED).unwrap();
	assert_eq!(opened.status(), Some("broken"));
	assert!(
		opened
			.reason()
			.unwrap()
			.starts_with("cannot read the case history")
	);
}

/// What a start appends, given the history and where the switch is.
async fn found_at_start(history: &[Event], closed: bool) -> Vec<Json> {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	write_history(&path, history);
	let (case, _mover) = started(&path, Ok(closed));
	closed_as(&case, closed).await;
	lines(&path).split_off(history.len())
}

// spec: NFO
#[tokio::test]
async fn a_start_records_a_position_the_history_does_not_hold() {
	let found = |event: &str| {
		vec![serde_json::json!({
			"event": event,
			"time": "2026-09-21T14:13:20Z",
			"boot": {"id": BOOT_ID, "offset": 4_321},
			"found": true,
		})]
	};
	assert_eq!(found_at_start(&[], true).await, Vec::<Json>::new());
	assert_eq!(found_at_start(&[], false).await, found("opened"));
	assert_eq!(
		found_at_start(&[Event::Opened, Event::Closed], false).await,
		found("opened")
	);
	assert_eq!(
		found_at_start(&[Event::Opened], true).await,
		found("closed")
	);
	assert_eq!(
		found_at_start(&[Event::Opened, Event::Closed], true).await,
		Vec::<Json>::new()
	);
	assert_eq!(
		found_at_start(&[Event::Opened], false).await,
		Vec::<Json>::new()
	);
}

// spec: NFO
#[tokio::test]
async fn an_opening_found_at_start_is_reported_as_found() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	let (case, _mover) = started(&path, Ok(false));
	closed_as(&case, false).await;

	let entries = case.entries(1);
	let opened = entry(&entries, LAST_OPENED).unwrap();
	assert_eq!(opened.traits.get("found"), Some(&Json::Bool(true)));
}

// spec: NFO
#[tokio::test]
async fn the_latest_opening_is_read_from_the_history() {
	let dir = tempfile::tempdir().unwrap();
	let path = dir.path().join("case-history.jsonl");
	write_history(&path, &[Event::Opened, Event::Closed]);
	let (case, _mover) = started(&path, Ok(true));
	closed_as(&case, true).await;

	let entries = case.entries(1);
	let opened = entry(&entries, LAST_OPENED).unwrap();
	assert_eq!(opened.value, Some(Json::from("2026-05-28T20:26:40Z")));
	assert_eq!(
		opened.traits.get("boot"),
		Some(&serde_json::json!({"id": "an-earlier-boot", "offset": 99}))
	);
}
