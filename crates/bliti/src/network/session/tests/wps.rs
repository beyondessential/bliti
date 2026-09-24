//! Joining by WPS (CFG, "Acting now").

use super::{super::Wps, *};

fn asked(method: &str, interface: Option<&str>, ssid: Option<&str>) -> Wps {
	Wps {
		method: method.to_owned(),
		interface: interface.map(ToOwned::to_owned),
		ssid: ssid.map(ToOwned::to_owned),
	}
}

fn wps(method: &str, interface: Option<&str>, ssid: Option<&str>) -> Message {
	let Wps {
		method,
		interface,
		ssid,
	} = asked(method, interface, ssid);
	Message::Wps {
		method,
		interface,
		ssid,
	}
}

#[tokio::test]
async fn wps_proposes_what_it_joined_for_the_client_to_confirm() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.joins(Ok(proposal()));
	send(&mut client, wps("push-button", None, None)).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: proposal(),
			capabilities: None,
		}
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));

	assert_eq!(confirmed(&mut client).await, proposal());
	assert_eq!(device.store().load().unwrap(), Some(proposal()));
	assert_eq!(
		device.log.calls(),
		[Call::Wps(asked("push-button", None, None))]
	);
}

#[tokio::test]
async fn wps_that_fails_is_invalid_and_restores() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, wps("pin", Some("wlan0"), None)).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, .. } if at == "$"
	));
	assert_eq!(
		device.log.calls(),
		[
			Call::Wps(asked("pin", Some("wlan0"), None)),
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn wps_by_pin_passes_on_the_pin_before_the_joined_result() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.pins("12345670");
	device.log.joins(Ok(proposal()));
	send(&mut client, wps("pin", Some("wlan0"), None)).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Pin {
			pin: "12345670".to_owned()
		}
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: proposal(),
			capabilities: None,
		}
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
}

#[tokio::test]
async fn the_pin_reaches_the_operator_while_the_join_is_still_under_way() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	device.log.pins("12345670");
	let (mut client, _task, _) = device.opened().await;
	send(&mut client, wps("pin", None, None)).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Pin {
			pin: "12345670".to_owned()
		}
	);
	assert!(quiet(&mut client).await, "the join has not finished");

	send(&mut client, Message::Discard).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, reached: None, .. } if at == "$"
	));
	assert_eq!(
		device.log.calls(),
		[
			Call::Wps(asked("pin", None, None)),
			Call::Aborted,
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn wps_for_a_named_network_passes_it_on() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.joins(Ok(proposal()));
	send(&mut client, wps("push-button", None, Some("Clinic"))).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: proposal(),
			capabilities: None,
		}
	);
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(
		device.log.calls(),
		[Call::Wps(asked("push-button", None, Some("Clinic")))]
	);
}

/// The backend refuses credentials for another network at the act's `ssid`, and nothing of the join
/// reaches the configuration (CFG, WLAN).
#[tokio::test]
async fn wps_that_yields_another_network_is_invalid_at_its_ssid_and_restores() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	let refused = Invalid {
		at: path(&[Segment::Name("ssid")]),
		reason: "the access point handed over credentials for \"Office\", not \"Clinic\""
			.to_owned(),
		reached: None,
	};
	device.log.joins(Err(refused.clone()));
	send(&mut client, wps("pin", Some("wlan0"), Some("Clinic"))).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: refused.at,
			reason: refused.reason,
			reached: None,
		}
	);
	assert_eq!(
		device.log.calls(),
		[
			Call::Wps(asked("pin", Some("wlan0"), Some("Clinic"))),
			Call::Restore(parsed(&recorded())),
		]
	);
	assert_eq!(
		confirmed(&mut client).await,
		recorded(),
		"nothing was joined"
	);
	assert_eq!(device.store().load().unwrap(), Some(recorded()));
}
