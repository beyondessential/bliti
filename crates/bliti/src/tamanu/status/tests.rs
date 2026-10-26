use serde_json::{Value as Json, json};

use super::*;
use crate::tamanu::fake::{DEGRADED, DRIFT, RUNNING, STOPPED};

fn report(json: &str) -> Report {
	serde_json::from_str(json).unwrap()
}

fn snapshot(json: &str) -> Snapshot {
	match Observed::from_report(&report(json)) {
		Observed::Standing(snapshot) => snapshot,
		other => panic!("expected a standing observation, got {other:?}"),
	}
}

fn with_expectation(json: &str, expectation: Json, any_short: bool) -> Snapshot {
	let mut value: Json = serde_json::from_str(json).unwrap();
	value["any_short"] = Json::Bool(any_short);
	value["expectations"]
		.as_array_mut()
		.unwrap()
		.push(expectation);
	snapshot(&value.to_string())
}

#[test]
fn a_report_with_nothing_short_is_running_with_both_versions() {
	let snapshot = snapshot(RUNNING);
	assert_eq!(snapshot.state, State::Running);
	assert_eq!(snapshot.reason, None);
	assert_eq!(snapshot.web.running.as_deref(), Some("v2.10.1"));
	assert_eq!(snapshot.web.warning, None);
	assert_eq!(snapshot.facility_server.running.as_deref(), Some("v2.10.0"));
	assert_eq!(snapshot.facility_server.warning, None);
}

#[test]
fn a_service_short_of_its_instances_is_degraded_and_named() {
	let snapshot = snapshot(DEGRADED);
	assert_eq!(snapshot.state, State::Degraded);
	assert_eq!(
		snapshot.reason.as_deref(),
		Some("tamanu-facility-api has 1 of 2 instances running")
	);
	assert!(snapshot.facility_server.up);
	assert_eq!(snapshot.facility_server.running.as_deref(), Some("v2.10.0"));
}

#[test]
fn every_required_service_down_is_stopped_and_keeps_the_configured_versions() {
	let snapshot = snapshot(STOPPED);
	assert_eq!(snapshot.state, State::Stopped);
	for (service, configured) in [
		(&snapshot.web, "v2.10.1"),
		(&snapshot.facility_server, "v2.10.0"),
	] {
		assert!(!service.up);
		assert_eq!(service.running, None);
		assert_eq!(service.configured.as_deref(), Some(configured));
	}
}

#[test]
fn instances_on_different_versions_give_the_highest_and_a_warning() {
	let snapshot = snapshot(DRIFT);
	assert_eq!(snapshot.state, State::Degraded);
	assert_eq!(snapshot.facility_server.running.as_deref(), Some("v2.10.0"));
	assert_eq!(
		snapshot.facility_server.warning.as_deref(),
		Some("runs different versions, v2.9.5 and v2.10.0")
	);
	assert_eq!(snapshot.web.warning, None);
}

#[test]
fn a_version_other_than_the_configured_one_warns_with_both() {
	let mut value: Json = serde_json::from_str(RUNNING).unwrap();
	value["any_short"] = Json::Bool(true);
	let api = value["expectations"]
		.as_array_mut()
		.unwrap()
		.iter_mut()
		.find(|e| e["name"] == "tamanu-facility-api")
		.unwrap();
	for instance in api["instances"].as_array_mut().unwrap() {
		instance["version_actual"] = json!("v2.9.5");
		instance["version_status"] = json!("mismatch");
	}
	let snapshot = snapshot(&value.to_string());
	assert_eq!(snapshot.state, State::Degraded);
	assert_eq!(
		snapshot.facility_server.warning.as_deref(),
		Some("runs v2.9.5 where v2.10.0 is configured")
	);
}

#[test]
fn an_expectation_that_is_unknown_does_not_degrade() {
	let unknown = json!({
		"name": "tamanu-patientportal",
		"expected_state": "unknown",
		"running": 0,
		"min_count": 1,
		"status": "unknown",
		"reason": "DB unreachable",
		"legacy": false,
		"behind_caddy": true,
		"instances": []
	});
	let snapshot = with_expectation(RUNNING, unknown, false);
	assert_eq!(snapshot.state, State::Running);
}

#[test]
fn a_service_present_that_must_not_be_is_degraded_with_every_required_one_running() {
	let forbidden = json!({
		"name": "tamanu-facility",
		"expected_state": "down",
		"running": 1,
		"min_count": 1,
		"status": "forbidden",
		"reason": "legacy singleton unit must not be present",
		"legacy": true,
		"behind_caddy": false,
		"instances": [{"name": "tamanu-facility", "instance": null, "pm_id": null, "running": true, "version_status": "unknown"}]
	});
	let snapshot = with_expectation(RUNNING, forbidden, true);
	assert_eq!(snapshot.state, State::Degraded);
	assert_eq!(
		snapshot.reason.as_deref(),
		Some("tamanu-facility is present and must not be")
	);
}

#[test]
fn a_report_without_the_services_holds_no_versions() {
	let snapshot = snapshot(r#"{"any_short":false,"expectations":[]}"#);
	assert_eq!(snapshot.state, State::Running);
	assert!(!snapshot.web.reported);
	assert_eq!(snapshot.facility_server, ServiceVersion::default());
}

#[test]
fn versions_order_as_versions_and_not_as_text() {
	assert_eq!(compare("v2.10.0", "v2.9.5"), Ordering::Greater);
	assert_eq!(compare("2.10.0", "v2.10.0"), Ordering::Equal);
	assert_eq!(compare("odd", "stuff"), Ordering::Less);
}
