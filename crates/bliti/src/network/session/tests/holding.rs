//! Several sessions open at once: the longest-open holds the configuration, the rest watch it
//! (CFG).

use super::*;

/// A second session opens busy, and is told every change the first brings about.
#[tokio::test]
async fn a_second_session_is_busy_and_is_told_every_change() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut first, _first_task, _) = device.opened().await;

	let (mut second, _second_task) = device.open().await;
	assert_eq!(
		recv(&mut second).await,
		Message::Configuration {
			document: recorded(),
			capabilities: Some(capabilities()),
		}
	);
	assert_eq!(recv(&mut second).await, state(Outcome::Applied).busy());

	proposed(&mut first, proposal()).await;
	let (watched, reported) = recv_held(&mut second).await;
	assert_eq!(watched, proposal());
	assert_eq!(reported, state(Outcome::Verifying).busy());

	device.log.release(Err(refused_key()));
	let failed = state(Outcome::Failed).failing(refused_key());
	assert_eq!(recv(&mut first).await, failed);
	assert_eq!(recv(&mut second).await, failed.clone().busy());
}

#[tokio::test]
async fn a_busy_sessions_proposals_and_acts_are_refused_at_the_whole_message() {
	let device = Device::new().await;
	let (_first, _first_task, _) = device.opened().await;
	let (mut second, _second_task, _) = device.opened().await;

	propose(&mut second, proposal()).await;
	assert!(matches!(
		recv(&mut second).await,
		Message::CannotConfigure { at, .. } if at == "$"
	));
	let (held, reported) = recv_held(&mut second).await;
	assert_eq!(held, recorded(), "nothing was saved");
	assert!(matches!(reported, Message::State { busy: true, .. }));

	send(&mut second, Message::Scan { interface: None }).await;
	assert!(matches!(
		recv(&mut second).await,
		Message::CannotAct { act, at, .. } if act == "scan" && at == "$"
	));
	send(&mut second, Message::Survey { interface: None }).await;
	assert!(matches!(
		recv(&mut second).await,
		Message::CannotAct { act, at, .. } if act == "survey" && at == "$"
	));
	assert_eq!(device.log.calls(), [], "none reached the backend");
	assert_eq!(device.store().load().unwrap(), Some(settled(recorded())));
}

#[tokio::test]
async fn the_second_session_holds_once_the_first_closes() {
	let device = Device::new().await;
	device.log.set(Applying::Held);
	let (mut first, first_task, _) = device.opened().await;
	let (mut second, _second_task, _) = device.opened().await;
	let (mut third, _third_task, _) = device.opened().await;

	first.close().await.unwrap();
	first_task.await.unwrap();
	assert_eq!(
		recv(&mut second).await,
		state(Outcome::Applied),
		"the second now holds the configuration"
	);
	assert!(
		quiet(&mut third).await,
		"the third still waits behind the second, and has nothing new to be told"
	);

	proposed(&mut second, proposal()).await;
	let (watched, reported) = recv_held(&mut third).await;
	assert_eq!(watched, proposal());
	assert_eq!(reported, state(Outcome::Verifying).busy());
}
