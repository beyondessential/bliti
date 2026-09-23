//! Scanning for a wireless candidate ranked above the default route while it is out of range, and
//! settling where there is none (LINK).

use super::*;

fn scans(rig: &Rig) -> usize {
	rig.asked()
		.iter()
		.filter(|call| call.starts_with("scan"))
		.count()
}

/// The wall port up, carrying the default route, under `document`.
async fn on_the_wall(rig: &mut Rig, running: Json) {
	rig.answers("192.0.2.1");
	rig.see([carrier(true)]).await;
	rig.stack.restore(&document(running)).await.unwrap();
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
}

#[tokio::test(start_paused = true)]
async fn a_better_network_coming_into_range_is_joined() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [clinic(), dynamic()]})).await;
	assert_eq!(rig.states()[0]["reached"], "carrier");
	assert_eq!(rig.states()[1], json!({"is": "default-route"}));
	assert_eq!(scans(&rig), 0, "the first scan waits");

	rig.hears("clinic", Ok(joined("clinic", 2412)));
	tokio::time::sleep(driver::RETRY).await;
	idle().await;
	assert_eq!(rig.asked(), ["scan wld0", "connect wld0 clinic"]);

	rig.answers("198.51.100.1");
	rig.see([
		leased("wld0", "198.51.100.10"),
		routed("wld0", "198.51.100.1"),
	])
	.await;
	assert_eq!(rig.states()[0], json!({"is": "default-route"}));
	tokio::time::sleep(driver::RETRY_MAX * 2).await;
	assert_eq!(scans(&rig), 1, "nothing is left to look for");
}

#[tokio::test(start_paused = true)]
async fn scans_back_off() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [clinic(), dynamic()]})).await;
	tokio::time::sleep(driver::RETRY).await;
	idle().await;
	assert_eq!(scans(&rig), 1);
	tokio::time::sleep(driver::RETRY).await;
	assert_eq!(scans(&rig), 1, "the second waits twice as long");
	tokio::time::sleep(driver::RETRY + Duration::from_secs(1)).await;
	assert_eq!(scans(&rig), 2);

	// Waits of 30s doubling to at most 15 min: 30, 60, 120, 240, 480, then 900 thereafter.
	tokio::time::sleep(Duration::from_secs(120 + 240 + 480) + Duration::from_secs(1)).await;
	assert_eq!(scans(&rig), 5);
	tokio::time::sleep(driver::RETRY_MAX * 2).await;
	assert_eq!(scans(&rig), 7);
}

#[tokio::test(start_paused = true)]
async fn with_nothing_carrying_the_default_route_every_network_out_of_range_is_looked_for() {
	let mut rig = Rig::wireless().await;
	rig.stack
		.restore(&document(json!({"attachments": [dynamic(), clinic()]})))
		.await
		.unwrap();
	tokio::time::sleep(driver::RETRY).await;
	idle().await;
	assert_eq!(scans(&rig), 1);
}

#[tokio::test(start_paused = true)]
async fn a_device_on_its_best_candidate_does_not_scan() {
	let mut rig = Rig::wireless().await;
	rig.answers("198.51.100.1");
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));
	rig.see([Observation::Heard {
		interface: "wld0".into(),
		networks: BTreeMap::from([("clinic".to_owned(), -55)]),
	}])
	.await;
	rig.stack
		.restore(&document(json!({"attachments": [clinic(), dynamic()]})))
		.await
		.unwrap();
	idle().await;
	rig.see([
		leased("wld0", "198.51.100.10"),
		routed("wld0", "198.51.100.1"),
	])
	.await;
	assert_eq!(rig.states()[0], json!({"is": "default-route"}));

	tokio::time::sleep(driver::RETRY_MAX * 2).await;
	assert_eq!(scans(&rig), 0);
}

#[tokio::test(start_paused = true)]
async fn a_network_ranked_below_the_default_route_is_not_looked_for() {
	let mut rig = Rig::wireless().await;
	on_the_wall(&mut rig, json!({"attachments": [dynamic(), clinic()]})).await;
	assert_eq!(rig.states()[0], json!({"is": "default-route"}));
	assert_eq!(rig.states()[1]["reached"], "carrier");

	tokio::time::sleep(driver::RETRY_MAX * 2).await;
	assert_eq!(scans(&rig), 0);
}

/// Found on a device: a network gone out of range between its failing and its retry was never
/// joined once it returned, since nothing looked for it.
#[tokio::test(start_paused = true)]
async fn a_network_that_failed_and_went_out_of_range_is_joined_when_it_returns() {
	let mut rig = Rig::wireless().await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);
	rig.see([Observation::Heard {
		interface: "wld0".into(),
		networks: BTreeMap::from([("clinic".to_owned(), -55)]),
	}])
	.await;
	on_the_wall(&mut rig, json!({"attachments": [clinic(), dynamic()]})).await;
	assert_eq!(rig.states()[0]["reached"], "association");

	rig.hears_nothing();
	rig.see([Observation::Heard {
		interface: "wld0".into(),
		networks: BTreeMap::new(),
	}])
	.await;
	assert_eq!(rig.states()[0]["reached"], "carrier");

	rig.hears("clinic", Ok(joined("clinic", 2412)));
	tokio::time::sleep(driver::RETRY).await;
	idle().await;
	assert!(
		rig.asked()
			.ends_with(&["scan wld0".to_owned(), "connect wld0 clinic".to_owned()]),
		"{:?}",
		rig.asked()
	);
}
