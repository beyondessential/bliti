//! Candidates turned off: never brought up, verified or scanned for, and brought up anew once turned
//! on again (LINK, CFG).

use super::*;

fn off(mut candidate: Json) -> Json {
	candidate["enabled"] = json!(false);
	candidate
}

/// A device on its wall port, leased and routed, under `running`.
async fn on_the_wall(rig: &mut Rig, running: Json) {
	rig.answers("192.0.2.1");
	rig.see([carrier(true)]).await;
	rig.stack.restore(&document(running)).await.unwrap();
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
}

/// A proposal turning a candidate off applies without verifying it, and nothing joins, scans for
/// or renders it.
#[tokio::test(start_paused = true)]
async fn a_candidate_turned_off_is_applied_without_being_verified() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [dynamic()]})).await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);

	let answer = applying(
		&mut rig,
		document(json!({"attachments": [off(clinic()), dynamic()]})),
	);
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(
		rig.states(),
		[json!({"is": "off"}), json!({"is": "default-route"})]
	);
	tokio::time::sleep(driver::RETRY_MAX * 2).await;
	assert!(
		rig.asked().is_empty(),
		"nothing is joined or scanned for: {:?}",
		rig.asked()
	);
	assert_eq!(rig.read("iwd/clinic.psk"), "");
}

/// Turning a joined candidate off takes its radio off the network and its file away from iwd.
#[tokio::test(start_paused = true)]
async fn turning_a_joined_candidate_off_leaves_its_network() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2412)));
	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_ne!(rig.read("iwd/clinic.psk"), "");

	let answer = applying(&mut rig, document(json!({"attachments": [off(clinic())]})));
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(rig.states(), [json!({"is": "off"})]);
	assert!(
		rig.asked().iter().any(|call| call == "disconnect wld0"),
		"{:?}",
		rig.asked()
	);
	assert_eq!(rig.read("iwd/clinic.psk"), "");
}

/// A candidate turned on again is changed, so it is judged.
#[tokio::test(start_paused = true)]
async fn a_candidate_turned_on_again_is_judged() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [off(clinic()), dynamic()]})).await;
	assert_eq!(rig.states()[0], json!({"is": "off"}));

	let answer = applying(
		&mut rig,
		document(json!({"attachments": [clinic(), dynamic()]})),
	);
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.at, "$['attachments'][0]");
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
}

/// A candidate turned off and on again takes nothing its link still holds until it is announced
/// again, as a candidate other than the one that left it would.
#[tokio::test(start_paused = true)]
async fn a_candidate_turned_on_again_is_brought_up_anew() {
	let mut rig = Rig::wired().await;
	on_the_wall(&mut rig, json!({"attachments": [dynamic()]})).await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);

	let answer = applying(&mut rig, document(json!({"attachments": [off(dynamic())]})));
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(rig.states(), [json!({"is": "off"})]);

	rig.answers("192.0.2.1");
	rig.stack
		.restore(&document(json!({"attachments": [dynamic()]})))
		.await
		.unwrap();
	idle().await;
	assert_eq!(rig.states(), [json!({"is": "verifying"})]);
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);
}
