//! A regulatory domain changing with no proposal, as the kernel taking the one an access point joined
//! names changes it (NET).

use super::*;

/// `radio()` as a domain opening channel 13 leaves it.
fn with_channel_13() -> RadioInfo {
	let mut radio = radio();
	radio
		.bands
		.get_mut(&Band::TwoPointFour)
		.unwrap()
		.channels
		.push(Channel {
			number: 13,
			frequency: 2472,
			max_width: 40,
			no_ir: false,
			radar: false,
		});
	radio
}

#[tokio::test(start_paused = true)]
async fn a_domain_the_device_did_not_set_is_taken_in_and_said() {
	let air = FakeAir {
		radios: vec![radio()],
		..FakeAir::default()
	};
	let retuned = air.retuned.clone();
	let rig = Rig::new(air).await;
	let before = rig.stack.capabilities();
	let mut states = rig.stack.states();
	states.mark_unchanged();

	*retuned.lock().unwrap() = Some(vec![with_channel_13()]);
	rig.see([Observation::Regulatory]).await;

	assert_ne!(rig.stack.capabilities(), before);
	assert!(
		states.has_changed().unwrap(),
		"the session is woken to carry the capabilities on `state` (CFG)"
	);
}

#[tokio::test(start_paused = true)]
async fn a_domain_change_leaving_the_capabilities_as_they_were_wakes_nothing() {
	let rig = Rig::wireless().await;
	let before = rig.stack.capabilities();
	let mut states = rig.stack.states();
	states.mark_unchanged();

	rig.see([Observation::Regulatory]).await;

	assert_eq!(rig.stack.capabilities(), before);
	assert!(!states.has_changed().unwrap());
}

#[tokio::test(start_paused = true)]
async fn a_change_arriving_during_a_probe_is_probed_again() {
	let air = FakeAir {
		radios: vec![radio()],
		..FakeAir::default()
	};
	let probed = air.probed.clone();
	let rig = Rig::new(air).await;
	rig.observe.send(Observation::Regulatory).unwrap();
	rig.observe.send(Observation::Regulatory).unwrap();
	idle().await;
	assert_eq!(
		probed.lock().unwrap().len(),
		3,
		"once at start, once for the first change, and again for the one arriving while it ran"
	);
}
