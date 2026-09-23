//! What a proposal comes to: judged per interface on what it adds or changes (CFG), or only on
//! applying where it is sent unverified.

use super::{driver::Judged, *};
use crate::network::select::{Attempt, Decision, Link, Stage, State};

fn wall() -> Observation {
	carrier(true)
}

/// A device on its wall port, leased and routed, with the wall port the configuration running.
async fn on_the_wall(rig: &mut Rig, running: Json) {
	rig.answers("192.0.2.1");
	rig.see([wall()]).await;
	rig.stack.restore(&document(running)).await.unwrap();
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
}

#[test]
fn a_candidate_is_changed_where_the_configuration_running_carries_none_equal_to_it() {
	let running = document(json!({"attachments": [dynamic(), clinic()]}));
	assert_eq!(
		driver::changed(
			&running,
			&document(json!({"attachments": [clinic(), dynamic()]}))
		),
		Vec::<usize>::new(),
		"a reorder changes nothing"
	);
	let mut edited = clinic();
	edited["security"]["passphrase"] = json!("battery staple");
	assert_eq!(
		driver::changed(
			&running,
			&document(json!({"attachments": [dynamic(), edited]}))
		),
		[1]
	);
	assert_eq!(
		driver::changed(
			&running,
			&document(json!({"attachments": [clinic(), dynamic(), clinic()]}))
		),
		[2],
		"each candidate running matches one proposed"
	);
	assert_eq!(
		driver::changed(&running, &document(json!({"attachments": []}))),
		Vec::<usize>::new()
	);
}

#[test]
fn a_proposal_fails_at_the_changed_candidate_that_got_furthest_on_an_interface_with_none_up() {
	let failed = |reached, reason: &str| State::Unavailable {
		reached,
		reason: reason.into(),
	};
	let on = |candidate, interface: &str| Judged {
		candidate,
		interfaces: vec![interface.into()],
	};
	let decision = Decision {
		links: BTreeMap::from([(
			"eth0".to_owned(),
			Link {
				candidate: 4,
				attempt: Attempt::test(1),
			},
		)]),
		hotspot: None,
		default_route: Some(4),
		states: vec![
			failed(Stage::Carrier, "out of range"),
			failed(Stage::Gateway, "silent"),
			failed(Stage::Association, "wrong key"),
			failed(Stage::Gateway, "also silent"),
			State::DefaultRoute,
		],
	};
	assert_eq!(driver::verdict(&decision, &[]), Ok(()));
	assert_eq!(
		driver::verdict(&decision, &[on(1, "eth0"), on(4, "eth0")]),
		Ok(()),
		"a failed static beside an established one on its port"
	);
	let invalid =
		driver::verdict(&decision, &[on(0, "wld0"), on(2, "wld0"), on(1, "eth0")]).unwrap_err();
	assert_eq!(invalid.at, "$['attachments'][2]");
	assert_eq!(invalid.reached.as_deref(), Some("association"));
	assert_eq!(invalid.reason, "wrong key");

	let invalid = driver::verdict(
		&Decision {
			links: BTreeMap::new(),
			default_route: None,
			..decision
		},
		&[on(0, "wld0"), on(3, "eth0"), on(1, "eth0")],
	)
	.unwrap_err();
	assert_eq!(
		invalid.at, "$['attachments'][1]",
		"the first of those that got furthest"
	);
	assert_eq!(invalid.reason, "silent");
}

/// Found on a device beside a working wall port, where any candidate established used to be enough.
#[tokio::test(start_paused = true)]
async fn a_wrong_passphrase_beside_a_working_wall_port_fails_at_the_wireless_candidate() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [dynamic()]})).await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);

	let answer = applying(
		&mut rig,
		document(json!({"attachments": [clinic(), dynamic()]})),
	);
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.at, "$['attachments'][0]");
	assert_eq!(failed.reached.as_deref(), Some("association"));
	assert!(failed.reason.contains("passphrase"), "{}", failed.reason);
	assert_eq!(rig.states()[1], json!({"is": "default-route"}));
}

/// Two sites' statics on one port: the proposal holds at the first site as at the second.
#[tokio::test(start_paused = true)]
async fn two_sites_statics_hold_at_the_first_site() {
	let mut rig = Rig::wired().await;
	rig.answers("192.0.2.1");
	rig.see([carrier(true)]).await;
	let answer = applying(
		&mut rig,
		document(json!({"attachments": [
			fixed("192.0.2.10/24", "192.0.2.1"),
			fixed("198.51.100.10/24", "198.51.100.1"),
		]})),
	);
	idle().await;
	rig.see([configured("192.0.2.10")]).await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(rig.states()[0], json!({"is": "default-route"}));
}

#[tokio::test(start_paused = true)]
async fn a_candidate_out_of_range_carried_over_does_not_fail_a_change_elsewhere() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [clinic(), dynamic()]})).await;
	assert_eq!(rig.states()[0]["reached"], "carrier");

	let answer = applying(
		&mut rig,
		document(json!({"attachments": [
			dynamic(),
			clinic(),
			fixed("198.51.100.10/24", "198.51.100.1"),
		]})),
	);
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(rig.states()[1]["reached"], "carrier");
}

#[tokio::test(start_paused = true)]
async fn an_added_network_out_of_range_fails_beside_a_working_wall_port() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [dynamic()]})).await;
	let answer = applying(
		&mut rig,
		document(json!({"attachments": [clinic(), dynamic()]})),
	);
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.at, "$['attachments'][0]");
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
}

/// A network that has just gone is not joined from what iwd heard before the proposal's scan.
#[tokio::test(start_paused = true)]
async fn a_join_waits_for_the_proposals_scan() {
	let mut rig = Rig::wireless().await;
	rig.see([Observation::Heard {
		interface: "wld0".into(),
		networks: BTreeMap::from([("clinic".to_owned(), -60)]),
	}])
	.await;
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));
	*rig.iwd.scan_takes.lock().unwrap() = Duration::from_secs(5);

	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
	assert!(
		!rig.asked().iter().any(|call| call.starts_with("connect")),
		"nothing out of range is joined: {:?}",
		rig.asked()
	);
}

#[tokio::test(start_paused = true)]
async fn an_unverified_proposal_is_applied_whatever_verifying_finds() {
	let mut rig = Rig::wireless().await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);
	let answer = proposing(
		&mut rig,
		document(json!({"attachments": [clinic()]})),
		false,
	);
	assert_eq!(answer.await.unwrap(), Ok(()));
	tokio::time::sleep(Duration::from_secs(1)).await;
	assert_eq!(rig.states()[0]["reached"], "association");
}

/// A hotspot that does not start is no candidate's verification, so no stage is reached, verified
/// or not.
#[tokio::test(start_paused = true)]
async fn a_hotspot_that_does_not_start_fails_at_the_hotspot_reaching_no_stage() {
	for verify in [true, false] {
		let mut rig = Rig::wireless().await;
		rig.failing.lock().unwrap().insert("hostapd Start".into());
		let answer = proposing(
			&mut rig,
			document(json!({
				"attachments": [],
				"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
			})),
			verify,
		);
		let failed = answer.await.unwrap().unwrap_err();
		assert_eq!(failed.at, "$['hotspot']");
		assert_eq!(failed.reached, None);
		assert!(failed.reason.contains("hostapd"), "{}", failed.reason);
	}
}
