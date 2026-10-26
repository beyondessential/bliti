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
