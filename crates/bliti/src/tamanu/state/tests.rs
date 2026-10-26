use super::*;
use crate::tamanu::fake::{Fake, RUNNING};

#[tokio::test]
async fn a_status_that_reads_is_observed() {
	let observed = observe(&Fake::reporting(RUNNING)).await;
	assert!(observed.is_running());
}

#[tokio::test]
async fn no_installation_is_absent() {
	assert_eq!(observe(&Fake::absent()).await, Observed::Absent);
}

#[tokio::test]
async fn a_program_that_cannot_be_run_is_broken() {
	let observed = observe(&Fake::unrunnable("cannot run `bestool`: not found")).await;
	assert_eq!(
		observed,
		Observed::Broken("cannot run `bestool`: not found".to_owned())
	);
}

#[tokio::test]
async fn an_installation_whose_status_is_unreadable_is_broken() {
	let observed = observe(&Fake::unreadable("not json")).await;
	let Observed::Broken(reason) = observed else {
		panic!("expected broken, got {observed:?}");
	};
	assert!(reason.starts_with("Tamanu is installed, but the status was unreadable"));
}

#[tokio::test]
async fn an_installation_whose_status_fails_says_what_it_said() {
	let observed = observe(&Fake::failing("the database is out of reach")).await;
	assert_eq!(
		observed,
		Observed::Broken("Tamanu is installed, but the database is out of reach".to_owned())
	);
}

#[tokio::test(start_paused = true)]
async fn the_poller_looks_while_a_session_is_open_and_for_the_idle_period_after() {
	use std::sync::atomic::{AtomicBool, Ordering};

	let fake = Arc::new(Fake::reporting(RUNNING));
	let (view, _) = watch::channel(View {
		observed: Observed::Absent,
		acting: None,
	});
	let view = Arc::new(view);
	let open = Arc::new(AtomicBool::new(true));
	let wanted: Arc<dyn Fn() -> bool + Send + Sync> = {
		let open = open.clone();
		Arc::new(move || open.load(Ordering::Relaxed))
	};
	let _poller = tokio::spawn(poll(
		fake.clone(),
		view.clone(),
		wanted,
		Duration::from_secs(60),
	));

	tokio::time::sleep(Duration::from_secs(12)).await;
	assert!(view.borrow().observed.is_running());
	assert!(fake.calls().len() >= 2, "looked more than once while open");

	open.store(false, Ordering::Relaxed);
	tokio::time::sleep(Duration::from_secs(70)).await;
	let stopped_at = fake.calls().len();
	tokio::time::sleep(Duration::from_secs(120)).await;
	assert_eq!(fake.calls().len(), stopped_at, "stopped once idle");

	open.store(true, Ordering::Relaxed);
	tokio::time::sleep(Duration::from_secs(3)).await;
	assert!(fake.calls().len() > stopped_at, "looks again at once");
}

#[test]
fn an_act_under_way_lies_over_what_is_observed() {
	let view = |observed, acting| View { observed, acting };
	assert_eq!(view(Observed::Absent, None).state(), Some(State::Absent));
	assert_eq!(
		view(Observed::Absent, Some(Act::Start)).state(),
		Some(State::Starting)
	);
	assert_eq!(
		view(Observed::Broken("x".to_owned()), Some(Act::Restart)).state(),
		Some(State::Restarting)
	);
	assert_eq!(view(Observed::Broken("x".to_owned()), None).state(), None);
}
