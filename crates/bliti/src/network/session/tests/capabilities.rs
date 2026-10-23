//! Holding proposals and acts to the capabilities (NET, CFG).

use super::*;

/// The proposal with one more member laid over it.
fn with(member: &str, value: Json) -> Map<String, Json> {
	let mut document = proposal();
	document.insert(member.to_owned(), value);
	document
}

#[tokio::test]
async fn a_proposal_outside_the_capabilities_is_refused_at_the_first_member_not_covered() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// A member the device does not support at all.
	propose(
		&mut client,
		with(
			"hotspot",
			json!({"enabled": true, "ssid": "bliti-setup", "passphrase": "stay clear of the clinic"}),
		),
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::CannotConfigure {
			at: "$['hotspot']".to_owned(),
			reason: "`hotspot` is not supported".to_owned(),
		}
	);
	let (held, _) = recv_held(&mut client).await;
	assert_eq!(held, recorded(), "and the configuration held, unchanged");

	// A supported member with a value outside those offered.
	let mut elsewhere = proposal();
	elsewhere["attachments"][1]["interface"] = json!("eth1");
	propose(&mut client, elsewhere).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, .. } if at == "$['attachments'][1]['interface']"
	));
	recv_held(&mut client).await;

	// A kind the device does not offer.
	let mut sae = proposal();
	sae["attachments"][0]["security"]["kind"] = json!("sae");
	propose(&mut client, sae).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, .. } if at == "$['attachments'][0]['security']['kind']"
	));
	recv_held(&mut client).await;

	assert_eq!(device.log.calls(), [], "nothing of any was applied");
}

/// A hotspot on the one radio beside a wireless candidate that radio would have to leave for it is
/// refused before anything is applied (HOT).
#[tokio::test]
async fn a_hotspot_a_one_at_a_time_radio_cannot_run_beside_a_client_is_refused() {
	let device = Device::new().await;
	let mut one_radio = capabilities();
	one_radio["document"]["hotspot"] = json!({"interface": {"wlan0": {}}});
	one_radio["radios"] =
		json!({"wlan0": {"model": "onboard", "bands": ["2ghz"], "alongside": "one-at-a-time"}});
	device.log.capabilities(one_radio);
	let (mut client, _task) = device.open().await;
	recv_held(&mut client).await;

	propose(
		&mut client,
		with(
			"hotspot",
			json!({"enabled": true, "ssid": "bliti-setup", "passphrase": "stay clear of the clinic"}),
		),
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, reason }
			if at == "$['hotspot']" && reason.contains("\"Clinic\"")
	));
	assert_eq!(device.log.calls(), [], "nothing was applied");
}

/// A hotspot choosing its channel on the shared-channel radio a wireless candidate could take is
/// refused at the first of band, channel and width it sets (HOT). The capabilities, band keys and all,
/// reach the client readable as they open the session.
#[tokio::test]
async fn a_chosen_channel_on_a_shared_channel_radio_beside_a_client_is_refused() {
	let device = Device::new().await;
	let mut one_radio = capabilities();
	one_radio["document"]["hotspot"] = json!({"interface": {"wlan0": {"band": {
		"2ghz": {"channel": [1, 6, 11], "channel-width": [20]},
		"5ghz": {"channel": [36, 40], "channel-width": [20]}
	}}}});
	one_radio["radios"] = json!({"wlan0": {
		"model": "onboard", "bands": ["2ghz", "5ghz"], "alongside": "shared-channel"
	}});
	device.log.capabilities(one_radio);
	let (mut client, _task) = device.open().await;
	recv_held(&mut client).await;

	propose(
		&mut client,
		with(
			"hotspot",
			json!({"enabled": true, "ssid": "bliti-setup", "passphrase": "stay clear of the clinic", "channel": 36}),
		),
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, .. } if at == "$['hotspot']['channel']"
	));
	assert_eq!(device.log.calls(), [], "nothing was applied");
}

#[tokio::test]
async fn an_act_not_offered_is_cannot_act_at_the_act_or_the_member_at_fault() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// An interface whose radio does not scan.
	send(
		&mut client,
		Message::Scan {
			interface: Some("wlan9".to_owned()),
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotAct { act, at, .. } if act == "scan" && at == "$['interface']"
	));

	// A radio that scans but does not survey.
	send(
		&mut client,
		Message::Survey {
			interface: Some("wlx00c0caa1b2c3".to_owned()),
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotAct { act, at, .. } if act == "survey" && at == "$['interface']"
	));

	// An act the device does not offer at all.
	let mut none = capabilities();
	none["acts"].as_object_mut().unwrap().remove("survey");
	device.log.capabilities(none);
	send(&mut client, Message::Survey { interface: None }).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotAct { act, at, .. } if act == "survey" && at == "$"
	));

	assert_eq!(device.log.calls(), [], "the backend was asked to do none");
}

/// Unchanged by the proposal that follows, which the new capabilities admit and the old did not.
#[tokio::test]
async fn a_proposal_is_held_to_the_capabilities_as_they_stand_when_it_is_made() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	let hotspot =
		json!({"enabled": true, "ssid": "bliti-setup", "passphrase": "stay clear of the clinic"});
	propose(&mut client, with("hotspot", hotspot.clone())).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::CannotConfigure { at, .. } if at == "$['hotspot']"
	));
	recv_held(&mut client).await;

	// A new regulatory domain opens the hotspot up.
	let mut widened = capabilities();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.capabilities(widened);
	proposed(&mut client, with("hotspot", hotspot.clone())).await;
}
