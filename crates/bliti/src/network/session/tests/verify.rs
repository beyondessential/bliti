//! Whether a proposal is verified, which the backend is told (CFG, "When a proposal fails").

use super::*;

#[tokio::test]
async fn a_proposal_is_verified_unless_it_says_otherwise() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	for verify in [None, Some(true), Some(false)] {
		propose_verifying(&mut client, proposal(), verify).await;
		assert_eq!(
			recv(&mut client).await,
			Message::Applied { capabilities: None }
		);
	}
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal()), true),
			Call::Apply(parsed(&proposal()), true),
			Call::Apply(parsed(&proposal()), false),
		]
	);
}

#[tokio::test]
async fn an_unverified_proposal_is_confirmed_like_any_other() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	propose_verifying(&mut client, proposal(), Some(false)).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(confirmed(&mut client).await, proposal());
	assert_eq!(device.store().load().unwrap(), Some(proposal()));
}

#[tokio::test]
async fn an_unverified_proposal_the_backend_cannot_apply_is_invalid() {
	let device = Device::new().await;
	let (mut client, _task, _) = device.opened().await;
	device.log.set(Applying::Fail(Invalid {
		at: path(&[]),
		reason: "networkd could not be reloaded".to_owned(),
		reached: None,
	}));
	propose_verifying(&mut client, proposal(), Some(false)).await;
	assert_eq!(
		recv(&mut client).await,
		Message::Invalid {
			at: path(&[]),
			reason: "networkd could not be reloaded".to_owned(),
			reached: None,
		}
	);
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal()), false),
			Call::Restore(parsed(&recorded())),
		]
	);
}

#[tokio::test]
async fn a_superseding_proposal_keeps_its_own_verify() {
	let device = Device::new().await;
	device.log.set(Applying::Hang);
	let (mut client, _task, _) = device.opened().await;
	propose(&mut client, proposal()).await;
	device
		.log
		.until(|calls| calls.contains(&Call::Apply(parsed(&proposal()), true)))
		.await;

	device.log.set(Applying::Succeed);
	propose_verifying(&mut client, recorded(), Some(false)).await;
	assert!(matches!(
		recv(&mut client).await,
		Message::Invalid { reached: None, .. }
	));
	assert_eq!(
		recv(&mut client).await,
		Message::Applied { capabilities: None }
	);
	assert_eq!(
		device.log.calls(),
		[
			Call::Apply(parsed(&proposal()), true),
			Call::Aborted,
			Call::Apply(parsed(&recorded()), false),
		]
	);
}
