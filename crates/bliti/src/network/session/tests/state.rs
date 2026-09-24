//! The state of each candidate (CFG, "The state of each candidate").

use super::*;

async fn state(client: &mut Client) -> Vec<Json> {
	match recv(client).await {
		Message::State { attachments, .. } => attachments,
		other => panic!("expected the state of each candidate, got {other:?}"),
	}
}

#[tokio::test]
async fn state_follows_the_opening_configuration_and_each_change() {
	let device = Device::observing().await;
	let (mut client, _task, opened) = device.opened().await;
	assert_eq!(opened, recorded());
	assert_eq!(
		state(&mut client).await,
		[json!({"is": "default-route"})],
		"one entry per candidate of the recorded configuration"
	);

	let lost = vec![json!({"is": "unavailable", "reached": "carrier", "reason": "no carrier"})];
	device.log.publish(lost.clone());
	assert_eq!(state(&mut client).await, lost);

	device.log.publish(lost);
	assert!(quiet(&mut client).await, "nothing changed");
}

#[tokio::test]
async fn state_follows_the_proposal_once_applied_and_the_recorded_configuration_after_a_revert() {
	let device = Device::observing().await;
	let (mut client, _task, _) = device.opened().await;
	assert_eq!(state(&mut client).await, observed(1));

	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(
		state(&mut client).await,
		observed(2),
		"the proposal's two candidates"
	);

	// Published for a configuration no longer running: held back.
	device.log.publish(observed(1));
	assert!(quiet(&mut client).await);

	send(&mut client, Message::Discard).await;
	assert_eq!(
		state(&mut client).await,
		observed(1),
		"the recorded configuration's one candidate, though it was the last published"
	);

	// A failed proposal is answered, then the recorded configuration's states follow.
	device.log.set(Applying::Fail(
		Stage::Association.failed(passphrase(), "the key was refused"),
	));
	propose(&mut client, proposal()).await;
	assert!(matches!(recv(&mut client).await, Message::Invalid { .. }));
	assert_eq!(state(&mut client).await, observed(1));
}

#[tokio::test]
async fn a_backend_observing_nothing_sends_no_state() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	assert!(quiet(&mut client).await);
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert!(quiet(&mut client).await);
}

/// Returning to the recorded configuration can change the capabilities back, and the next `state`
/// carries them where no answer would (CFG).
#[tokio::test]
async fn state_carries_the_capabilities_a_revert_changed_back() {
	let device = Device::observing().await;
	let (mut client, _task, _) = device.opened().await;
	assert_eq!(state(&mut client).await, observed(1));

	let original = capabilities();
	let mut widened = original.clone();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.after_apply(widened.clone());
	propose(&mut client, proposal()).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied {
			capabilities: Some(widened)
		}
	);
	let Message::State { capabilities, .. } = recv(&mut client).await else {
		panic!("expected the state of each candidate")
	};
	assert_eq!(capabilities, None, "applied already said so");

	device.log.capabilities(original.clone());
	send(&mut client, Message::Discard).await;
	assert_eq!(
		recv(&mut client).await,
		Message::State {
			attachments: observed(1),
			capabilities: Some(original),
		}
	);
}
