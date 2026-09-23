//! Joining by WPS (CFG, "Acting now").

use super::*;

#[tokio::test]
async fn wps_proposes_what_it_joined_for_the_client_to_confirm() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.joins(Ok(proposal()));
	send(
		&mut client,
		Message::Wps {
			method: "push-button".to_owned(),
			interface: None,
		},
	)
	.await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: proposal(),
			capabilities: None,
			verify: None,
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
		[Call::Wps("push-button".to_owned(), None)]
	);
}

#[tokio::test]
async fn wps_that_fails_is_invalid_and_restores() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	send(
		&mut client,
		Message::Wps {
			method: "pin".to_owned(),
			interface: Some("wlan0".to_owned()),
		},
	)
	.await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { at, .. } if at == "$"
	));
	assert_eq!(
		device.log.calls(),
		[
			Call::Wps("pin".to_owned(), Some("wlan0".to_owned())),
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
	send(
		&mut client,
		Message::Wps {
			method: "pin".to_owned(),
			interface: Some("wlan0".to_owned()),
		},
	)
	.await;
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
			verify: None,
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
	send(
		&mut client,
		Message::Wps {
			method: "pin".to_owned(),
			interface: None,
		},
	)
	.await;
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
			Call::Wps("pin".to_owned(), None),
			Call::Aborted,
			Call::Restore(parsed(&recorded())),
		]
	);
}
