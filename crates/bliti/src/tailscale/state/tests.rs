use super::*;

fn fixture(name: &str) -> Observed {
	let raw = std::fs::read_to_string(format!(
		"{}/src/tailscale/fixtures/{name}.json",
		env!("CARGO_MANIFEST_DIR")
	))
	.unwrap();
	Observed::from_status(&serde_json::from_str(&raw).unwrap())
}

fn standing(observed: Observed) -> Snapshot {
	match observed {
		Observed::Standing(snapshot) => snapshot,
		Observed::Broken(reason) => panic!("broken: {reason}"),
	}
}

#[test]
fn a_connected_device_is_running_and_named() {
	let snapshot = standing(fixture("status-running"));
	assert_eq!(snapshot.state, State::Running);
	assert_eq!(snapshot.name.as_deref(), Some("bliti-lab.example.com"));
	assert_eq!(snapshot.tailnet.as_deref(), Some("127.0.0.1"));
	assert_eq!(snapshot.key_expiry, None);
}

#[test]
fn a_stopped_device_keeps_its_name_and_tailnet() {
	let snapshot = standing(fixture("status-stopped"));
	assert_eq!(snapshot.state, State::Stopped);
	assert!(snapshot.name.is_some() && snapshot.tailnet.is_some());
	assert_eq!(snapshot.warning, None, "a stopped device warns of nothing");
}

#[test]
fn a_lapsed_sign_in_is_expired_and_keeps_its_name() {
	let snapshot = standing(fixture("status-expired"));
	assert_eq!(snapshot.state, State::Expired);
	assert!(snapshot.name.is_some());
	assert_eq!(
		snapshot.key_expiry,
		Some("2026-10-24T04:41:40Z".parse().unwrap())
	);
}

#[test]
fn a_device_with_no_sign_in_needs_login_and_has_no_name() {
	let snapshot = standing(fixture("status-needs-login"));
	assert_eq!(snapshot.state, State::NeedsLogin);
	assert_eq!((snapshot.name, snapshot.tailnet), (None, None));
}

#[test]
fn machine_authorisation_is_needs_approval() {
	let status: Status = serde_json::from_str(r#"{"BackendState":"NeedsMachineAuth"}"#).unwrap();
	assert_eq!(
		standing(Observed::from_status(&status)).state,
		State::NeedsApproval
	);
}

#[test]
fn connecting_states_are_starting() {
	for backend in ["Starting", "NoState"] {
		let status: Status =
			serde_json::from_str(&format!(r#"{{"BackendState":"{backend}"}}"#)).unwrap();
		assert_eq!(
			standing(Observed::from_status(&status)).state,
			State::Starting
		);
	}
}

#[test]
fn an_unknown_state_is_broken_not_guessed() {
	let status: Status = serde_json::from_str(r#"{"BackendState":"Hibernating"}"#).unwrap();
	assert!(matches!(
		Observed::from_status(&status),
		Observed::Broken(_)
	));
}

#[test]
fn health_is_a_warning_only_while_running() {
	let running: Status =
		serde_json::from_str(r#"{"BackendState":"Running","Health":["no relay"," other"]}"#)
			.unwrap();
	assert_eq!(
		standing(Observed::from_status(&running)).warning.as_deref(),
		Some("no relay")
	);
	let stopped: Status =
		serde_json::from_str(r#"{"BackendState":"Stopped","Health":["Tailscale is stopped."]}"#)
			.unwrap();
	assert_eq!(standing(Observed::from_status(&stopped)).warning, None);
}

#[test]
fn a_zero_key_expiry_does_not_lapse() {
	let status: Status = serde_json::from_str(
		r#"{"BackendState":"Running","Self":{"KeyExpiry":"0001-01-01T00:00:00Z"}}"#,
	)
	.unwrap();
	assert_eq!(standing(Observed::from_status(&status)).key_expiry, None);
}
