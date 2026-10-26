use super::*;
use crate::tailscale::fake::{Fake, JOINED};

#[tokio::test]
async fn another_tailnet_is_refused_naming_both() {
	let fake = Fake::in_state("Running");
	let err = check_at_startup(&*fake, Some("other.example"))
		.await
		.unwrap_err();
	let message = err.to_string();
	assert!(
		message.contains("other.example") && message.contains(JOINED),
		"{message}"
	);
}

#[tokio::test]
async fn the_same_tailnet_is_accepted() {
	let fake = Fake::in_state("Running");
	assert!(check_at_startup(&*fake, Some(JOINED)).await.is_ok());
}

#[tokio::test]
async fn signed_out_is_accepted() {
	let fake = Fake::in_state("NeedsLogin");
	assert!(check_at_startup(&*fake, Some(JOINED)).await.is_ok());
}

#[tokio::test]
async fn an_unrestricted_device_accepts_any() {
	let fake = Fake::in_state("Running");
	assert!(check_at_startup(&*fake, None).await.is_ok());
}

#[tokio::test]
async fn a_daemon_that_does_not_answer_is_not_a_refusal() {
	let fake = Fake::in_state("Running");
	fake.go_down();
	assert!(
		check_at_startup(&*fake, Some("other.example"))
			.await
			.is_ok()
	);
}

#[tokio::test(start_paused = true)]
async fn a_daemon_still_starting_is_waited_for() {
	let fake = Fake::in_state("Running");
	fake.unsettled("Starting");
	let waiting = {
		let fake = fake.clone();
		tokio::spawn(async move { check_at_startup(&*fake, Some("other.example")).await })
	};
	tokio::time::sleep(Duration::from_secs(2)).await;
	fake.move_to("Running");
	let err = waiting.await.unwrap().unwrap_err();
	assert!(err.to_string().contains(JOINED), "{err}");
}

#[tokio::test(start_paused = true)]
async fn a_daemon_that_never_settles_is_not_a_refusal() {
	let fake = Fake::in_state("Running");
	fake.unsettled("NoState");
	let started = Instant::now();
	assert!(
		check_at_startup(&*fake, Some("other.example"))
			.await
			.is_ok()
	);
	assert!(started.elapsed() >= SETTLE);
}

#[tokio::test(start_paused = true)]
async fn a_daemon_with_no_tailnet_once_settled_is_accepted_at_once() {
	let fake = Fake::in_state("Running");
	fake.unsettled("Stopped");
	let started = Instant::now();
	assert!(
		check_at_startup(&*fake, Some("other.example"))
			.await
			.is_ok()
	);
	assert!(started.elapsed() < SETTLE_POLL);
}
