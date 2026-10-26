use super::*;
use crate::tamanu::{
	acts::Act,
	state::{Snapshot, State},
};

fn find<'a>(entries: &'a [Entry], name: &str) -> &'a Entry {
	entries.iter().find(|entry| entry.name == name).unwrap()
}

fn service(running: Option<&str>, configured: Option<&str>, up: bool) -> ServiceVersion {
	ServiceVersion {
		reported: true,
		up,
		running: running.map(ToOwned::to_owned),
		configured: configured.map(ToOwned::to_owned),
		warning: None,
	}
}

fn standing(state: State, reason: Option<&str>) -> View {
	View {
		observed: Observed::Standing(Snapshot {
			state,
			reason: reason.map(ToOwned::to_owned),
			web: service(Some("v2.10.1"), Some("v2.10.1"), true),
			facility_server: service(Some("v2.10.0"), Some("v2.10.0"), true),
		}),
		acting: None,
	}
}

#[test]
fn every_state_reports_its_name_and_status() {
	let acting = |observed, act| View {
		observed,
		acting: Some(act),
	};
	let running = standing(State::Running, None).observed;
	for (view, name, status) in [
		(standing(State::Running, None), "running", "passed"),
		(
			standing(State::Degraded, Some("tamanu-facility-api is not running")),
			"degraded",
			"warning",
		),
		(standing(State::Stopped, Some("x")), "stopped", "failed"),
		(
			View {
				observed: Observed::Absent,
				acting: None,
			},
			"absent",
			"failed",
		),
		(acting(running.clone(), Act::Start), "starting", "warning"),
		(acting(running, Act::Restart), "restarting", "warning"),
	] {
		let entries = entries(7, &view);
		let entry = find(&entries, "tamanu-state");
		assert_eq!(entry.value, Some(serde_json::json!(name)));
		assert_eq!(entry.status(), Some(status), "{name}");
		if status != "passed" {
			assert!(entry.reason().is_some(), "{name} says why");
		}
	}
}

#[test]
fn a_degraded_state_says_what_is_short() {
	let entries = entries(
		7,
		&standing(
			State::Degraded,
			Some("tamanu-facility-api has 1 of 2 instances running"),
		),
	);
	assert_eq!(
		find(&entries, "tamanu-state").reason(),
		Some("tamanu-facility-api has 1 of 2 instances running")
	);
}

#[test]
fn a_running_service_reports_the_version_it_runs() {
	let entries = entries(7, &standing(State::Running, None));
	let web = find(&entries, "tamanu-web-version");
	assert_eq!(web.value, Some(serde_json::json!("v2.10.1")));
	assert_eq!(web.status(), Some("passed"));
	let server = find(&entries, "tamanu-facility-server-version");
	assert_eq!(server.value, Some(serde_json::json!("v2.10.0")));
}

#[test]
fn a_version_that_is_not_the_configured_one_warns() {
	let mut view = standing(State::Degraded, Some("x"));
	let Observed::Standing(snapshot) = &mut view.observed else {
		unreachable!();
	};
	snapshot.facility_server.warning = Some("runs v2.9.5 where v2.10.0 is configured".to_owned());
	let entries = entries(7, &view);
	let server = find(&entries, "tamanu-facility-server-version");
	assert_eq!(server.status(), Some("warning"));
	assert_eq!(
		server.reason(),
		Some("runs v2.9.5 where v2.10.0 is configured")
	);
}

#[test]
fn a_service_not_running_gives_the_configured_version_as_failed() {
	let mut view = standing(State::Stopped, Some("x"));
	let Observed::Standing(snapshot) = &mut view.observed else {
		unreachable!();
	};
	snapshot.web = service(None, Some("v2.10.1"), false);
	let entries = entries(7, &view);
	let web = find(&entries, "tamanu-web-version");
	assert_eq!(web.value, Some(serde_json::json!("v2.10.1")));
	assert_eq!(web.status(), Some("failed"));
	assert_eq!(web.reason(), Some("the service is not running"));
}

#[test]
fn a_service_running_with_a_version_unread_warns_with_the_configured_one() {
	let mut view = standing(State::Running, None);
	let Observed::Standing(snapshot) = &mut view.observed else {
		unreachable!();
	};
	snapshot.web = service(None, Some("v2.10.1"), true);
	let entries = entries(7, &view);
	assert_eq!(
		find(&entries, "tamanu-web-version").status(),
		Some("warning")
	);
}

#[test]
fn a_service_with_no_version_known_is_skipped() {
	let mut view = standing(State::Running, None);
	let Observed::Standing(snapshot) = &mut view.observed else {
		unreachable!();
	};
	snapshot.web = ServiceVersion::default();
	let entries = entries(7, &view);
	let web = find(&entries, "tamanu-web-version");
	assert_eq!(web.status(), Some("skipped"));
	assert!(web.reason().is_some());
}

#[test]
fn an_absent_tamanu_skips_both_versions() {
	let entries = entries(
		7,
		&View {
			observed: Observed::Absent,
			acting: None,
		},
	);
	for name in ["tamanu-web-version", "tamanu-facility-server-version"] {
		let entry = find(&entries, name);
		assert_eq!(entry.status(), Some("skipped"), "{name}");
		assert_eq!(entry.reason(), Some("Tamanu is not installed"));
	}
}

#[test]
fn a_broken_tamanu_breaks_all_three_with_the_one_reason() {
	let entries = entries(
		7,
		&View {
			observed: Observed::Broken("cannot run `bestool`".to_owned()),
			acting: None,
		},
	);
	assert_eq!(entries.len(), 3);
	for entry in &entries {
		assert_eq!(entry.status(), Some("broken"), "{}", entry.name);
		assert_eq!(entry.reason(), Some("cannot run `bestool`"));
		assert_eq!(entry.value, None);
	}
}

#[test]
fn an_act_under_way_is_reported_whatever_cannot_be_observed() {
	let entries = entries(
		7,
		&View {
			observed: Observed::Broken("the status cannot be read".to_owned()),
			acting: Some(Act::Restart),
		},
	);
	assert_eq!(
		find(&entries, "tamanu-state").value,
		Some(serde_json::json!("restarting"))
	);
	assert_eq!(
		find(&entries, "tamanu-web-version").status(),
		Some("broken")
	);
}
