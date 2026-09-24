//! The hotspot on a shared-channel radio: its bring-up beside a station, and the channels it may not follow it onto (HOT).

use super::*;

/// A shared-channel radio's hotspot starts once its station has joined, on the station's channel,
/// rather than holding the radio on a channel of its own that the station could then join only on.
#[tokio::test(start_paused = true)]
async fn the_hotspot_waits_for_its_station_to_join() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2462)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	let hostapd: Vec<String> = rig
		.calls()
		.into_iter()
		.filter(|call| call.starts_with("hostapd"))
		.collect();
	assert_eq!(hostapd, ["hostapd Start ap0"]);
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=11\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);
}

/// brcmfmac takes the station off its network as the hotspot starts beside it, and the station
/// joins again on the same channel rather than the candidate failing.
#[tokio::test(start_paused = true)]
async fn a_station_knocked_off_as_the_hotspot_starts_joins_again() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2412)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	assert!(rig.calls().contains(&"hostapd Start ap0".to_owned()));
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Disconnected,
	}])
	.await;
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	let joins = rig
		.asked()
		.iter()
		.filter(|call| call.starts_with("connect"))
		.count();
	assert_eq!(joins, 2, "{:?}", rig.asked());

	// A later drop, with the hotspot running as rendered, is a drop.
	let states = rig.stack.states();
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Disconnected,
	}])
	.await;
	assert_eq!(states.borrow().clone().unwrap()[0]["is"], "unavailable");
}

/// iwd's diagnostics can answer without a frequency. The channel is then read from nl80211, and a
/// station reported joined with none leaves the hotspot where it is.
#[tokio::test(start_paused = true)]
async fn a_join_reported_without_a_channel_takes_it_from_the_radio() {
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio()],
		operating: BTreeMap::from([(
			"wld0".into(),
			Operating {
				frequency: 2412,
				width: Some(20),
			},
		)]),
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	let unknown = Joined {
		frequency: None,
		..joined("clinic", 2412)
	};
	rig.hears("clinic", Ok(unknown.clone()));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=1\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);

	rig.system.lock().unwrap().clear();
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Connected(unknown),
	}])
	.await;
	idle().await;
	assert_eq!(rig.calls(), Vec::<String>::new());
}

/// A client joined on a channel no access point may start on leaves a shared-channel radio's hotspot
/// no channel to use, which fails the proposal at the hotspot, saying why.
#[tokio::test(start_paused = true)]
async fn a_hotspot_cannot_follow_its_station_onto_a_radar_channel() {
	let mut radio = radio();
	radio.bands.insert(
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
		radios: vec![radio],
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 5700)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	let refused = answer.await.unwrap().unwrap_err();
	assert_eq!(refused.at, "$['hotspot']");
	assert!(refused.reason.contains("channel 140"), "{}", refused.reason);
	assert!(refused.reason.contains("radar"), "{}", refused.reason);
	assert_eq!(refused.reached, None);
	assert!(!rig.calls().iter().any(|call| call.starts_with("hostapd")));
}

/// A hotspot that would have to share a radar channel with a connection the radio is joined to now,
/// and that the document keeps, is refused before anything is applied (HOT).
#[tokio::test(start_paused = true)]
async fn a_hotspot_beside_a_connection_on_a_radar_channel_is_refused_up_front() {
	let mut radio = radio();
	radio.bands.insert(
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
		radios: vec![radio],
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 5700)));
	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));

	let hotspot = json!({"ssid": "bliti", "passphrase": "read me aloud"});
	let refused = rig
		.stack
		.check(&document(
			json!({"attachments": [clinic()], "hotspot": hotspot}),
		))
		.unwrap_err();
	assert_eq!(refused.at, "$['hotspot']");
	assert!(
		refused
			.reason
			.starts_with("the hotspot cannot run while wld0 is connected to \"clinic\""),
		"{}",
		refused.reason
	);
	assert!(refused.reason.contains("channel 140"), "{}", refused.reason);
	assert!(
		refused
			.reason
			.ends_with("Turn the connection off to run the hotspot"),
		"{}",
		refused.reason
	);
	assert_eq!(
		rig.stack.check(&document(
			json!({"attachments": [dynamic()], "hotspot": hotspot})
		)),
		Ok(()),
		"without that connection the hotspot can run"
	);
	let mut off = clinic();
	off["enabled"] = json!(false);
	assert_eq!(
		rig.stack.check(&document(
			json!({"attachments": [off, dynamic()], "hotspot": hotspot})
		)),
		Ok(()),
		"with that connection turned off the hotspot can run"
	);
}

/// Where its station cannot join, the hotspot stops waiting and runs on a channel of its own.
#[tokio::test(start_paused = true)]
async fn the_hotspot_runs_where_its_station_does_not_join() {
	let mut rig = Rig::wireless().await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);

	let proposal = document(json!({
		"attachments": [{
			"kind": "wireless", "label": "clinic", "enabled": true, "verify": false, "ssid": "clinic",
			"security": {"kind": "psk", "passphrase": "correct horse"}
		}],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=6\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);
}

/// A shared-channel radio's hotspot follows its station onto each channel it joins (HOT).
#[tokio::test(start_paused = true)]
async fn the_hotspot_follows_the_station_onto_a_new_channel() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -55);
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=1\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);

	rig.system.lock().unwrap().clear();
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Connected(joined("clinic", 2462)),
	}])
	.await;
	idle().await;
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=11\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);
	assert_eq!(rig.calls(), ["hostapd Restart ap0"]);
}

/// A wireless candidate turned off takes no radio, so the hotspot has no client to wait for and
/// starts on its own channel at once (HOT, LINK).
#[tokio::test(start_paused = true)]
async fn a_connection_turned_off_holds_the_hotspot_back_from_nothing() {
	let mut rig = Rig::wireless().await;
	rig.hears("clinic", Ok(joined("clinic", 2412)));
	let mut off = clinic();
	off["enabled"] = json!(false);

	let proposal = document(json!({
		"attachments": [off],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(!rig.asked().iter().any(|call| call.starts_with("connect")));
	assert!(
		rig.read("hostapd/ap0.conf").contains("\nchannel=6\n"),
		"{}",
		rig.read("hostapd/ap0.conf")
	);
}
