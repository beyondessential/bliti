use super::*;

fn now() -> Timestamp {
	"2026-10-26T00:00:00Z".parse().unwrap()
}

fn snapshot(state: State) -> Snapshot {
	Snapshot {
		state,
		name: Some("iti.fleet.example".to_owned()),
		tailnet: Some("fleet.example".to_owned()),
		key_expiry: None,
		warning: None,
	}
}

fn entries_for(snapshot: Snapshot) -> Vec<Entry> {
	entries(7, Some(&Observed::Standing(snapshot)), now())
}

fn find<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
	entries.iter().find(|entry| entry.name == name).unwrap()
}

#[test]
fn every_state_reports_its_name_and_status() {
	for (state, name, status) in [
		(State::Running, "running", "passed"),
		(State::Starting, "starting", "warning"),
		(State::Stopped, "stopped", "failed"),
		(State::NeedsLogin, "needs-login", "failed"),
		(State::Expired, "expired", "failed"),
		(State::NeedsApproval, "needs-approval", "failed"),
	] {
		let entries = entries_for(snapshot(state));
		let entry = find(&entries, "tailscale-state");
		assert_eq!(entry.value, Some(serde_json::json!(name)));
		assert_eq!(entry.status(), Some(status), "{name}");
		if status != "passed" {
			assert!(entry.reason().is_some(), "{name} says why");
		}
	}
}

#[test]
fn a_connection_problem_warns_with_tailscales_own_words() {
	let entries = entries_for(Snapshot {
		warning: Some("no relay server reachable".to_owned()),
		..snapshot(State::Running)
	});
	let state = find(&entries, "tailscale-state");
	assert_eq!(state.status(), Some("warning"));
	assert_eq!(state.reason(), Some("no relay server reachable"));
}

#[test]
fn a_lapsed_device_keeps_its_name_and_tailnet_passed() {
	for state in [State::Expired, State::Stopped, State::NeedsApproval] {
		let entries = entries_for(snapshot(state));
		assert_eq!(find(&entries, "tailscale-name").status(), Some("passed"));
		assert_eq!(find(&entries, "tailnet").status(), Some("passed"));
	}
}

#[test]
fn a_device_with_no_sign_in_skips_its_name_and_tailnet() {
	let entries = entries_for(Snapshot {
		name: None,
		tailnet: None,
		..snapshot(State::NeedsLogin)
	});
	for name in ["tailscale-name", "tailnet"] {
		let entry = find(&entries, name);
		assert_eq!(entry.status(), Some("skipped"));
		assert_eq!(entry.reason(), Some("the device is not signed in"));
	}
}

fn expiring(expiry: &str) -> Snapshot {
	Snapshot {
		key_expiry: Some(expiry.parse().unwrap()),
		..snapshot(State::Running)
	}
}

#[test]
fn a_sign_in_lapsing_later_passes() {
	let entries = entries_for(expiring("2026-12-01T00:00:00Z"));
	assert_eq!(
		find(&entries, "tailscale-key-expiry").status(),
		Some("passed")
	);
}

#[test]
fn a_sign_in_lapsing_within_seven_days_warns_saying_when() {
	let entries = entries_for(expiring("2026-10-29T06:00:00Z"));
	let entry = find(&entries, "tailscale-key-expiry");
	assert_eq!(entry.status(), Some("warning"));
	assert_eq!(entry.reason(), Some("the sign-in lapses in 3 days"));
}

#[test]
fn a_sign_in_lapsing_within_the_hour_says_hours() {
	let entries = entries_for(expiring("2026-10-26T00:20:00Z"));
	assert_eq!(
		find(&entries, "tailscale-key-expiry").reason(),
		Some("the sign-in lapses in 1 hours")
	);
}

#[test]
fn a_lapsed_sign_in_fails() {
	let entries = entries_for(expiring("2026-10-25T00:00:00Z"));
	let entry = find(&entries, "tailscale-key-expiry");
	assert_eq!(entry.status(), Some("failed"));
	assert_eq!(entry.reason(), Some("the sign-in has lapsed"));
}

#[test]
fn a_sign_in_that_does_not_lapse_or_is_not_held_is_skipped_saying_which() {
	let entries = entries_for(snapshot(State::Running));
	let entry = find(&entries, "tailscale-key-expiry");
	assert_eq!(entry.status(), Some("skipped"));
	assert_eq!(entry.reason(), Some("the sign-in does not lapse"));
	let entries = entries_for(snapshot(State::NeedsLogin));
	assert_eq!(
		find(&entries, "tailscale-key-expiry").reason(),
		Some("the device holds no sign-in")
	);
}

#[test]
fn a_daemon_that_does_not_answer_breaks_all_four_with_one_reason() {
	let entries = entries(7, Some(&Observed::Broken("down".to_owned())), now());
	assert_eq!(entries.len(), 4);
	for entry in &entries {
		assert_eq!(entry.status(), Some("broken"), "{}", entry.name);
		assert_eq!(entry.reason(), Some("down"));
	}
}

#[test]
fn a_device_without_tailscale_reports_none() {
	assert!(entries(7, None, now()).is_empty());
}
