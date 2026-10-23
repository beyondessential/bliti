//! The state of each candidate (CFG, "The state of each candidate").

use super::fake::observed;
use super::*;

/// The `state` of an observing device whose configuration has `candidates`.
fn observing(outcome: Outcome, candidates: usize) -> Message {
	state(outcome).entries(observed(candidates))
}

#[tokio::test]
async fn state_follows_the_opening_configuration_and_each_change() {
	let device = Device::observing().await;
	let (mut client, _task) = device.open().await;
	let (opened, reported) = recv_held(&mut client).await;
	assert_eq!(opened, recorded());
	assert_eq!(
		reported,
		observing(Outcome::Applied, 1),
		"one entry per candidate of the configuration held"
	);

	let lost = vec![json!({"is": "unavailable", "reached": "carrier", "reason": "no carrier"})];
	device.log.publish(lost.clone());
	assert_eq!(
		recv(&mut client).await,
		state(Outcome::Applied).entries(lost.clone())
	);

	device.log.publish(lost);
	assert!(quiet(&mut client).await, "nothing changed");
}

#[tokio::test]
async fn state_follows_the_proposal_and_waits_for_the_states_of_the_configuration_held() {
	let device = Device::observing().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;

	propose(&mut client, proposal()).await;
	let (held, reported) = recv_held(&mut client).await;
	assert_eq!(held, proposal());
	assert_eq!(
		reported,
		observing(Outcome::Verifying, 2),
		"the proposal's two candidates, not the one before it left"
	);

	// Published for a configuration no longer held: held back.
	device.log.publish(observed(1));
	assert!(quiet(&mut client).await);

	device.log.publish(observed(2));
	assert!(quiet(&mut client).await, "back to what was told");

	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, observing(Outcome::Applied, 2));
}

#[tokio::test]
async fn a_backend_observing_nothing_sends_no_entries_but_still_sends_state() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;
	device.log.release(Ok(()));
	assert_eq!(recv(&mut client).await, state(Outcome::Applied));
}

/// What applying changed of the capabilities is told in the next `state`, once (CFG).
#[tokio::test]
async fn state_carries_the_capabilities_a_proposal_changed() {
	let device = Device::observing().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;

	let mut widened = capabilities();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.after_apply(widened.clone());
	proposed_observing(&mut client, proposal()).await;

	device.log.release(Ok(()));
	assert_eq!(
		recv(&mut client).await,
		observing(Outcome::Applied, 2).told(widened)
	);
	assert!(quiet(&mut client).await, "said once");
}

async fn proposed_observing(client: &mut Client, document: Map<String, Json>) {
	propose(client, document.clone()).await;
	let (echoed, reported) = recv_held(client).await;
	assert_eq!(echoed, document);
	assert_eq!(reported, observing(Outcome::Verifying, 2));
}

/// A regulatory domain the kernel takes from an access point joined changes the capabilities with no
/// proposal, and the backend waking the session with every candidate's state as it was is enough for
/// `state` to say so (NET, CFG).
#[tokio::test]
async fn state_carries_capabilities_changed_with_no_proposal() {
	let device = Device::observing().await;
	let (mut client, _task, _) = device.opened().await;

	let mut widened = capabilities();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.capabilities(widened.clone());
	device.log.publish(observed(1));
	assert_eq!(
		recv(&mut client).await,
		observing(Outcome::Applied, 1).told(widened)
	);

	device.log.publish(observed(1));
	assert!(quiet(&mut client).await, "said once");
}

/// Each session is told the capabilities it has not been told, whenever it opened.
#[tokio::test]
async fn a_session_opened_after_the_capabilities_changed_is_told_them_whole() {
	let device = Device::observing().await;
	let mut widened = capabilities();
	widened["document"]
		.as_object_mut()
		.unwrap()
		.insert("hotspot".to_owned(), json!(true));
	device.log.capabilities(widened.clone());
	device.log.publish(observed(1));
	tokio::time::sleep(Duration::from_millis(50)).await;

	let (mut client, _task) = device.open().await;
	assert_eq!(
		recv(&mut client).await,
		Message::Configuration {
			document: recorded(),
			capabilities: Some(widened),
		}
	);
}
