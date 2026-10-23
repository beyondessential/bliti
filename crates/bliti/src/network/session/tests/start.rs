//! What a device holds and runs as the daemon starts (CFG).

use super::*;

/// A configuration whose verification a restart interrupted is verified again, against the one it
/// replaced, and its outcome recorded.
#[tokio::test]
async fn a_restart_that_interrupted_a_verification_verifies_again_against_what_it_replaced() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut client, _task, _) = device.opened().await;
	proposed(&mut client, proposal()).await;
	device
		.log
		.until(|calls| matches!(calls, [Call::Apply(..)]))
		.await;

	// Power goes: a fresh daemon over the same disk, with nothing of the session or the verification.
	let (restarted, log) = device.restarted().await;
	let (mut next, _task) = open(restarted).await;
	let (held, reported) = recv_held(&mut next).await;
	assert_eq!(held, proposal());
	assert_eq!(
		log.until(|calls| !calls.is_empty()).await,
		[Call::Apply(parsed(&proposal()), parsed(&recorded()))],
		"verified again, not restored, against the configuration it replaced"
	);
	// Told as it was, or as the fake already finished verifying it.
	if reported != state(Outcome::Applied) {
		assert_eq!(reported, state(Outcome::Verifying));
		assert_eq!(recv(&mut next).await, state(Outcome::Applied));
	}
	assert_eq!(
		device.store().load().unwrap(),
		Some(settled(proposal())),
		"and recorded as settled"
	);
}

#[tokio::test]
async fn a_restart_with_the_outcome_settled_restores_without_verifying() {
	for outcome in [Outcome::Applied, Outcome::Failed] {
		let failure = (outcome == Outcome::Failed).then(refused_key);
		let device = Device::holding(
			Stored {
				outcome,
				failure: failure.clone(),
				..settled(proposal())
			},
			false,
		)
		.await;
		let (_configurator, log) = device.restarted().await;
		assert_eq!(
			log.calls(),
			[Call::Restore(parsed(&proposal()))],
			"{outcome:?} is put in force as it is"
		);

		let (mut client, _task) = device.open().await;
		let (held, reported) = recv_held(&mut client).await;
		assert_eq!(held, proposal());
		assert_eq!(
			reported,
			state(outcome).failing(failure),
			"keeping the outcome across the restart"
		);
	}
}

#[tokio::test]
async fn a_device_with_nothing_held_starts_on_the_default_and_does_not_record_it() {
	let scratch = Scratch::new();
	let (backend, log) = fake();
	let fallback = object(json!({"attachments": []}));
	let configurator = Configurator::start(backend, scratch.store(), fallback.clone())
		.await
		.unwrap();
	assert_eq!(log.calls(), [Call::Restore(parsed(&fallback))]);
	assert_eq!(scratch.store().load().unwrap(), None);
	let device = Device {
		configurator,
		log,
		scratch,
	};
	let (mut client, _task) = device.open().await;
	let (held, reported) = recv_held(&mut client).await;
	assert_eq!(held, fallback);
	assert_eq!(
		reported,
		state(Outcome::Applied),
		"judged on no candidate, so applied"
	);
}

/// What a build before the held shape left is not read: a device with one starts on the default.
#[tokio::test]
async fn a_bare_document_from_an_earlier_build_is_not_read() {
	let scratch = Scratch::new();
	std::fs::create_dir_all(&scratch.0).unwrap();
	std::fs::write(
		scratch.0.join("network.json"),
		serde_json::to_vec(&Json::Object(proposal())).unwrap(),
	)
	.unwrap();
	let (backend, log) = fake();
	let fallback = object(json!({"attachments": []}));
	let _configurator = Configurator::start(backend, scratch.store(), fallback.clone())
		.await
		.unwrap();
	assert_eq!(log.calls(), [Call::Restore(parsed(&fallback))]);
}

/// A configuration that cannot be put in force is failed, though what was held settled; the outcome
/// is not recorded, so the next start tries it again.
#[tokio::test]
async fn a_held_configuration_that_cannot_be_put_in_force_is_failed_but_not_recorded() {
	let scratch = Scratch::new();
	scratch.store().record(&settled(proposal())).await.unwrap();
	let (backend, log) = fake();
	let failure = Failure {
		at: "$['regulatory-domain']".to_owned(),
		reason: "regdom was not written".to_owned(),
		reached: None,
	};
	log.cannot_restore(failure.clone());
	let configurator = Configurator::start(backend, scratch.store(), recorded())
		.await
		.unwrap();
	let device = Device {
		configurator,
		log,
		scratch,
	};
	let (mut client, _task) = device.open().await;
	let (held, reported) = recv_held(&mut client).await;
	assert_eq!(held, proposal());
	assert_eq!(reported, state(Outcome::Failed).failing(failure));
	assert_eq!(
		device.store().load().unwrap(),
		Some(settled(proposal())),
		"as it was recorded"
	);
}
