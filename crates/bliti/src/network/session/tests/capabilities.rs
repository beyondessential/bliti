//! Holding proposals and acts to the capabilities, and telling the client where applying changed them
//! (NET, CFG).

use super::*;

/// The proposal with one more member laid over it.
fn with(member: &str, value: Json) -> Map<String, Json> {
	let mut document = proposal();
	document.insert(member.to_owned(), value);
	document
}

#[tokio::test]
async fn a_proposal_outside_the_capabilities_is_invalid_at_the_first_member_not_covered() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;

	// A member the device does not support at all.
	propose(
		&mut client,
		with(
			"hotspot",
			json!({"ssid": "bliti-setup", "passphrase": "stay clear of the clinic"}),
		),
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: "$['hotspot']".to_owned(),
			reason: "`hotspot` is not supported".to_owned(),
			reached: None,
		}
	);

	// A supported member with a value outside those offered.
	let mut elsewhere = proposal();
	elsewhere["attachments"][1]["interface"] = json!("eth1");
	propose(&mut client, elsewhere).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['attachments'][1]['interface']"
	));

	// A kind the device does not offer.
	let mut sae = proposal();
	sae["attachments"][0]["security"]["kind"] = json!("sae");
	propose(&mut client, sae).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['attachments'][0]['security']['kind']"
	));

	assert_eq!(device.log.calls(), [], "nothing of any was applied");
	assert_eq!(confirmed(&mut client).await, recorded());
}

/// A hotspot on the one radio beside a wireless candidate that radio would have to leave for it is
/// refused before anything is applied (HOT).
#[tokio::test]
async fn a_hotspot_a_one_at_a_time_radio_cannot_run_beside_a_client_is_invalid() {
	let device = Device::new().await;
	let mut one_radio = capabilities();
	one_radio["document"]["hotspot"] = json!({"interface": {"wlan0": {}}});
	one_radio["radios"] =
		json!({"wlan0": {"model": "onboard", "bands": ["2.4ghz"], "alongside": "one-at-a-time"}});
	device.log.capabilities(one_radio);
	let (mut client, _task, _) = device.opened().await;

	propose(
		&mut client,
		with(
			"hotspot",
			json!({"ssid": "bliti-setup", "passphrase": "stay clear of the clinic"}),
		),
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reason, reached: None }
			if at == "$['hotspot']" && reason.contains("\"Clinic\"")
	));
	assert_eq!(device.log.calls(), [], "nothing was applied");
}

/// A hotspot choosing its channel on the shared-channel radio a wireless candidate could take is
/// refused at the first of band, channel and width it sets (HOT).
#[tokio::test]
async fn a_chosen_channel_on_a_shared_channel_radio_beside_a_client_is_invalid() {
	let device = Device::new().await;
	let mut one_radio = capabilities();
	one_radio["document"]["hotspot"] = json!({"interface": {"wlan0": {"band": {
		"5ghz": {"channel": [36, 40], "channel-width": [20]}
	}}}});
	one_radio["radios"] =
		json!({"wlan0": {"model": "onboard", "bands": ["5ghz"], "alongside": "shared-channel"}});
	device.log.capabilities(one_radio);
	let (mut client, _task, _) = device.opened().await;

	propose(
		&mut client,
		with(
			"hotspot",
			json!({"ssid": "bliti-setup", "passphrase": "stay clear of the clinic", "channel": 36}),
		),
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['hotspot']['channel']"
	));
	assert_eq!(device.log.calls(), [], "nothing was applied");
}

#[tokio::test]
async fn an_act_not_offered_is_invalid_at_the_act_or_the_member_at_fault() {
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
		Message::Invalid { at, reached: None, .. } if at == "$['interface']"
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
		Message::Invalid { at, reached: None, .. } if at == "$['interface']"
	));

	// A WPS method no radio offers, and a radio that offers no WPS.
	send(
		&mut client,
		Message::Wps {
			method: "nfc".to_owned(),
			interface: None,
			ssid: None,
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['method']"
	));
	send(
		&mut client,
		Message::Wps {
			method: "push-button".to_owned(),
			interface: Some("wlx00c0caa1b2c3".to_owned()),
			ssid: None,
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['interface']"
	));

	// A network named where the device joins by WPS for none in particular.
	let mut unnamed = capabilities();
	unnamed["acts"]["wps"]["interface"]["wlan0"]
		.as_object_mut()
		.unwrap()
		.remove("ssid");
	device.log.capabilities(unnamed);
	send(
		&mut client,
		Message::Wps {
			method: "push-button".to_owned(),
			interface: None,
			ssid: Some("Clinic".to_owned()),
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['ssid']"
	));

	// An act the device does not offer at all.
	let mut none = capabilities();
	none["acts"].as_object_mut().unwrap().remove("survey");
	device.log.capabilities(none);
	send(&mut client, Message::Survey { interface: None }).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$"
	));

	assert_eq!(device.log.calls(), [], "the backend was asked to do none");
	assert_eq!(
		confirmed(&mut client).await,
		recorded(),
		"nothing was joined"
	);
}

#[tokio::test]
async fn applied_carries_the_capabilities_only_where_applying_changed_them() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);

	// A new regulatory domain opens the hotspot up.
	let mut widened = capabilities();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.after_apply(widened.clone());
	propose(&mut client, with("regulatory-domain", json!("AU"))).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied {
			capabilities: Some(widened)
		}
	);

	// Unchanged by the next, which the new capabilities admit and the old did not.
	propose(
		&mut client,
		with(
			"hotspot",
			json!({"ssid": "bliti-setup", "passphrase": "stay clear of the clinic"}),
		),
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
}

#[tokio::test]
async fn an_invalid_proposal_after_one_it_superseded_restores_the_recorded_configuration() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()))))
		.await;

	propose(
		&mut client,
		with("hotspot", json!({"ssid": "x", "passphrase": "y"})),
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$"
	));
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$['hotspot']"
	));
	assert_eq!(
		device.log.until(|calls| calls.len() == 3).await,
		[
			Call::Apply(parsed(&proposal())),
			Call::Aborted,
			Call::Restore(parsed(&recorded())),
		],
		"what the superseded attempt left running is not kept"
	);
}
