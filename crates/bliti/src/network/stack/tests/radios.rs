//! A device with two radios.

use super::*;

/// `wld0` as the Pi's shared-channel radio has it, and a second radio `wlan1` running its access
/// point as `alongside` says.
fn two_radios(alongside: Alongside) -> FakeAir {
	FakeAir {
		radios: vec![
			radio(),
			RadioInfo {
				station: "wlan1".into(),
				model: "mt7921u".into(),
				alongside: Some(alongside),
				..radio()
			},
		],
		..FakeAir::default()
	}
}

fn pinned(ssid: &str, interface: &str) -> Json {
	json!({
		"kind": "wireless", "label": ssid, "verify": true, "ssid": ssid, "interface": interface,
		"security": {"kind": "psk", "passphrase": "correct horse"}
	})
}

fn calls_starting(rig: &Rig, prefix: &str) -> Vec<String> {
	rig.calls()
		.into_iter()
		.filter(|call| call.starts_with(prefix))
		.collect()
}

/// iwd knows every network on every radio, so a pin holds because bliti joins each candidate only on
/// the station of its radio (LINK), and each radio is scanned for the proposal.
#[tokio::test(start_paused = true)]
async fn a_candidate_pinned_to_each_radio_joins_on_its_own() {
	let mut rig = Rig::new(two_radios(Alongside::Independent)).await;
	rig.answers("192.0.2.1");
	rig.answers("198.51.100.1");
	rig.hears("clinic", Ok(joined("clinic", 2412)));
	rig.hears("depot", Ok(joined("depot", 2462)));

	let proposal = document(json!({
		"attachments": [pinned("clinic", "wlan1"), pinned("depot", "wld0")],
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([
		leased("wlan1", "192.0.2.10"),
		routed("wlan1", "192.0.2.1"),
		leased("wld0", "198.51.100.10"),
		routed("wld0", "198.51.100.1"),
	])
	.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(
		rig.states(),
		[json!({"is": "default-route"}), json!({"is": "up"})]
	);

	let asked = rig.asked();
	for scan in ["scan wld0", "scan wlan1"] {
		assert!(asked.contains(&scan.to_owned()), "{scan} in {asked:?}");
	}
	let joins: Vec<&String> = asked
		.iter()
		.filter(|call| call.starts_with("connect"))
		.collect();
	assert_eq!(joins, ["connect wlan1 clinic", "connect wld0 depot"]);
	assert!(
		rig.read("iwd/clinic.psk").contains("AutoConnect=false\n"),
		"iwd joins nothing by itself"
	);
	assert!(
		rig.read("network/50-bliti-wlan1.network")
			.contains("attachments'][0]")
	);
	assert!(
		rig.read("network/50-bliti-wld0.network")
			.contains("attachments'][1]")
	);
}

/// An unpinned hotspot runs on the radio carrying no wireless candidate, on its own access point
/// interface and its own channel, and not on the shared-channel radio the candidate took (HOT).
#[tokio::test(start_paused = true)]
async fn the_hotspot_runs_on_the_radio_carrying_no_candidate() {
	let mut rig = Rig::new(two_radios(Alongside::SharedChannel)).await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2462)));
	rig.iwd
		.heard_on
		.lock()
		.unwrap()
		.insert("wlan1".into(), BTreeMap::new());

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));

	assert!(rig.asked().contains(&"connect wld0 clinic".to_owned()));
	assert_eq!(calls_starting(&rig, "create"), ["create ap1 on wlan1"]);
	assert_eq!(calls_starting(&rig, "hostapd"), ["hostapd Start ap1"]);
	let conf = rig.read("hostapd/ap1.conf");
	assert!(conf.contains("interface=ap1\n"), "{conf}");
	assert!(
		conf.contains("\nchannel=6\n"),
		"its own channel, not wld0's client's: {conf}"
	);
	assert_eq!(rig.read("hostapd/ap0.conf"), "");
}

/// A hotspot pinned to the second radio shares that radio's client's channel, and waits for that
/// client alone, whatever the first radio's client is on (HOT).
#[tokio::test(start_paused = true)]
async fn a_shared_channel_hotspot_follows_the_client_on_its_own_radio() {
	let mut rig = Rig::new(FakeAir {
		radios: vec![
			RadioInfo {
				alongside: Some(Alongside::Independent),
				..radio()
			},
			RadioInfo {
				station: "wlan1".into(),
				..radio()
			},
		],
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.answers("198.51.100.1");
	rig.hears("clinic", Ok(joined("clinic", 2412)));
	rig.hears("depot", Ok(joined("depot", 2462)));

	let proposal = document(json!({
		"attachments": [pinned("clinic", "wld0"), pinned("depot", "wlan1")],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud", "interface": "wlan1"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([
		leased("wld0", "192.0.2.10"),
		routed("wld0", "192.0.2.1"),
		leased("wlan1", "198.51.100.10"),
		routed("wlan1", "198.51.100.1"),
	])
	.await;
	assert_eq!(answer.await.unwrap(), Ok(()));

	assert_eq!(calls_starting(&rig, "hostapd"), ["hostapd Start ap1"]);
	let conf = rig.read("hostapd/ap1.conf");
	assert!(conf.contains("\nchannel=11\n"), "wlan1's channel: {conf}");
}

/// Only a shared-channel radio's hotspot has to share its client's channel, so a client of an
/// independent radio on a channel no access point may start on leaves its hotspot running.
#[tokio::test(start_paused = true)]
async fn an_independent_radios_client_on_a_radar_channel_leaves_its_hotspot_alone() {
	let mut independent = radio();
	independent.alongside = Some(Alongside::Independent);
	independent.bands.insert(
		Band::Five,
		BandInfo {
			channels: vec![Channel {
				number: 140,
				frequency: 5700,
				max_width: 20,
				no_ir: true,
				radar: true,
			}],
			widths: vec![20],
		},
	);
	let mut rig = Rig::new(FakeAir {
		radios: vec![independent],
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.answers("198.51.100.1");
	rig.hears("clinic", Ok(joined("clinic", 5700)));
	rig.see([carrier(true)]).await;

	let proposal = document(json!({
		"attachments": [clinic(), dynamic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([
		leased("wld0", "192.0.2.10"),
		routed("wld0", "192.0.2.1"),
		leased("eth0", "198.51.100.10"),
		routed("eth0", "198.51.100.1"),
	])
	.await;
	assert_eq!(answer.await.unwrap(), Ok(()));

	// Rendered again with the client on channel 140, for the wired candidate going down.
	rig.system.lock().unwrap().clear();
	rig.see([carrier(false)]).await;
	idle().await;
	assert!(rig.calls().contains(&"reload networkd".to_owned()));
	assert_eq!(calls_starting(&rig, "hostapd"), Vec::<String>::new());
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=6\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);
}

/// A scan and a survey run on each radio, or on the one named, and leave out the device's own
/// hotspot whichever radio it runs on.
#[tokio::test(start_paused = true)]
async fn a_scan_and_a_survey_run_on_each_radio() {
	let heard = |bssid: [u8; 6], ssid: &[u8]| {
		let mut elements = vec![0, ssid.len() as u8];
		elements.extend_from_slice(ssid);
		AccessPoint::read(bssid, 2412, -5000, 0, &elements)
	};
	let mut rig = Rig::new(FakeAir {
		heard: vec![
			heard([2, 0, 0, 0, 0, 1], b"clinic"),
			heard([2, 0, 0, 0, 0, 3], b"bliti"),
		],
		addresses: BTreeMap::from([("ap1".into(), "02:00:00:00:00:03".into())]),
		..two_radios(Alongside::SharedChannel)
	})
	.await;

	let entries = rig.stack.scan(None).await.unwrap();
	let heard_by: Vec<(&str, &str)> = entries
		.iter()
		.map(|entry| {
			(
				entry["interface"].as_str().unwrap(),
				entry["ssid"].as_str().unwrap(),
			)
		})
		.collect();
	assert_eq!(heard_by, [("wld0", "clinic"), ("wlan1", "clinic")]);

	rig.iwd.calls.lock().unwrap().clear();
	let entries = rig.stack.scan(Some("wlan1")).await.unwrap();
	assert_eq!(entries.len(), 1);
	assert_eq!(entries[0]["interface"], "wlan1");
	assert_eq!(rig.asked(), ["scan wlan1"]);

	let spectrum = rig.stack.survey(None).await.unwrap().unwrap();
	let surveyed: BTreeSet<&str> = spectrum["channels"]
		.as_array()
		.unwrap()
		.iter()
		.map(|channel| channel["interface"].as_str().unwrap())
		.collect();
	assert_eq!(surveyed, BTreeSet::from(["wld0", "wlan1"]));
}
