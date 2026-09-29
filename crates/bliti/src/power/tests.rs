use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// What happened while going away, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
	Ended,
	Carried(Act),
}

/// A system that records what it was asked to carry out, and fails where told to.
#[derive(Default)]
struct Recording {
	steps: Arc<Mutex<Vec<Step>>>,
	fail: AtomicBool,
}

impl System for Recording {
	fn carry_out(&self, act: Act) -> anyhow::Result<()> {
		self.steps.lock().unwrap().push(Step::Carried(act));
		if self.fail.load(Ordering::SeqCst) {
			anyhow::bail!("told to fail");
		}
		Ok(())
	}
}

fn controller(acts: Vec<Act>, fail: bool) -> (Controller, Arc<Mutex<Vec<Step>>>) {
	let system = Recording::default();
	system.fail.store(fail, Ordering::SeqCst);
	let steps = system.steps.clone();
	let ended = steps.clone();
	let controller = Controller::new(
		acts,
		Arc::new(system),
		Box::new(move || {
			let ended = ended.clone();
			Box::pin(async move { ended.lock().unwrap().push(Step::Ended) })
		}),
	);
	(controller, steps)
}

/// Wait until `steps` holds `count` of them, or fail the test.
async fn until(steps: &Arc<Mutex<Vec<Step>>>, count: usize) -> Vec<Step> {
	tokio::time::timeout(Duration::from_secs(10), async {
		loop {
			let seen = steps.lock().unwrap().clone();
			if seen.len() >= count {
				return seen;
			}
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	})
	.await
	.expect("the steps happen")
}

#[test]
fn every_act_has_one_wire_name() {
	for act in [Act::Restart, Act::Reboot, Act::PowerOff] {
		assert_eq!(Act::from_name(act.name()), Some(act));
	}
	assert_eq!(Act::from_name("hibernate"), None);
}

#[test]
fn every_cause_has_its_wire_name() {
	assert_eq!(Cause::ManualControl.name(), "manual-control");
	assert_eq!(Cause::LowBattery.name(), "low-battery");
}

const MANUAL: fn(Act) -> Going = |act| Going {
	act,
	cause: Cause::ManualControl,
};

const LOW_BATTERY: Going = Going {
	act: Act::PowerOff,
	cause: Cause::LowBattery,
};

/// An act the device did not list is refused, and so is every act once one has been accepted (CTL).
#[tokio::test]
async fn only_the_first_listed_act_is_accepted() {
	let (controller, _) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let peer = Peer::default();
	// Held open, so the act accepted waits on it rather than being carried out under the test.
	let _session = controller.session();

	assert_eq!(
		controller.ask("restart", &peer),
		Err("this device cannot restart".to_owned())
	);
	assert_eq!(
		controller.ask("hibernate", &peer),
		Err("this device cannot hibernate".to_owned())
	);
	assert_eq!(controller.ask("reboot", &peer), Ok(Act::Reboot));
	assert_eq!(
		controller.ask("power-off", &peer),
		Err("the device is already rebooting".to_owned())
	);
}

/// Every open feed is told before any session is ended, the connections are dropped once every
/// session has closed, and only then is the act carried out (CTL, "Going away").
#[tokio::test]
async fn going_away_tells_every_feed_then_ends_every_session_then_acts() {
	let (controller, steps) = controller(vec![Act::Reboot], false);
	let mut first = controller.feed();
	let mut second = controller.feed();
	let mut session = controller.session();

	let act = controller.ask("reboot", &Peer::default()).unwrap();
	controller.go(MANUAL(act));

	assert_eq!(first.going().await, MANUAL(Act::Reboot));
	assert_eq!(second.going().await, MANUAL(Act::Reboot));
	let ending = tokio::time::timeout(Duration::from_millis(200), session.ending()).await;
	assert!(ending.is_err(), "no session ends until every feed is told");

	first.told();
	drop(second);
	tokio::time::timeout(Duration::from_secs(1), session.ending())
		.await
		.expect("every session ends once every feed is told");
	tokio::time::sleep(Duration::from_millis(100)).await;
	assert!(
		steps.lock().unwrap().is_empty(),
		"nothing is dropped while a session is open"
	);

	drop(session);
	assert_eq!(
		until(&steps, 2).await,
		vec![Step::Ended, Step::Carried(Act::Reboot)]
	);
}

/// A feed that opens once the device is going is told at once.
#[tokio::test]
async fn a_feed_opened_while_going_is_told_at_once() {
	let (controller, _) = controller(vec![Act::Restart], false);
	let _session = controller.session();
	controller.go(MANUAL(controller.ask("restart", &Peer::default()).unwrap()));
	tokio::time::sleep(Duration::from_millis(50)).await;

	let mut late = controller.feed();
	assert_eq!(
		tokio::time::timeout(Duration::from_secs(1), late.going())
			.await
			.unwrap(),
		MANUAL(Act::Restart)
	);
}

/// An act that fails is logged, and the device takes acts again rather than refusing every one for
/// the rest of its life (CTL).
#[tokio::test]
async fn a_failed_act_leaves_the_device_taking_acts_again() {
	let (controller, steps) = controller(vec![Act::PowerOff], true);
	controller.go(MANUAL(
		controller.ask("power-off", &Peer::default()).unwrap(),
	));
	until(&steps, 2).await;
	tokio::time::sleep(Duration::from_millis(50)).await;

	let mut session = controller.session();
	let ending = tokio::time::timeout(Duration::from_millis(100), session.ending()).await;
	assert!(
		ending.is_err(),
		"a session opened after the failure is left alone"
	);
	assert_eq!(
		controller.ask("power-off", &Peer::default()),
		Ok(Act::PowerOff)
	);
}

/// A device offering nothing lists nothing, and refuses whatever it is asked.
#[tokio::test]
async fn a_controller_offering_nothing_refuses_everything() {
	let controller = Controller::none();
	assert!(controller.inner.acts.is_empty());
	assert!(controller.ask("reboot", &Peer::default()).is_err());
}

/// A low-battery shutdown is announced with its cause, runs the whole of going away, and powers off
/// (LOW, "Shutting down").
#[tokio::test]
async fn a_low_battery_shutdown_goes_away_and_powers_off() {
	let (controller, steps) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let mut feed = controller.feed();
	assert_eq!(controller.low_battery(|| {}), Ok(()));
	assert_eq!(feed.going().await, LOW_BATTERY);
	feed.told();
	assert_eq!(
		until(&steps, 2).await,
		vec![Step::Ended, Step::Carried(Act::PowerOff)]
	);
}

/// Once a low-battery shutdown has begun, every act asked for is refused, saying so (CTL).
#[tokio::test]
async fn an_act_after_a_low_battery_shutdown_is_refused() {
	let (controller, _) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let _session = controller.session();
	assert_eq!(controller.low_battery(|| {}), Ok(()));
	for act in ["reboot", "power-off"] {
		assert_eq!(
			controller.ask(act, &Peer::default()),
			Err("the device is already powering off for a low battery".to_owned())
		);
	}
	assert_eq!(
		controller.low_battery(|| {}),
		Err(NotBegun::AlreadyGoing(LOW_BATTERY)),
		"a second shutdown is not begun"
	);
}

/// An act accepted first is the one carried out, and no low-battery shutdown is begun as well (LOW).
#[tokio::test]
async fn no_low_battery_shutdown_after_an_accepted_act() {
	let (controller, _) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let _session = controller.session();
	assert_eq!(controller.ask("reboot", &Peer::default()), Ok(Act::Reboot));
	let not_begun = controller.low_battery(|| {}).unwrap_err();
	assert_eq!(not_begun, NotBegun::AlreadyGoing(MANUAL(Act::Reboot)));
	assert_eq!(not_begun.to_string(), "the device is already rebooting");
}

/// A device that cannot power off says so rather than begin a shutdown, and takes acts as before.
#[tokio::test]
async fn a_device_that_cannot_power_off_begins_no_shutdown() {
	let (controller, steps) = controller(vec![Act::Reboot], false);
	let _session = controller.session();
	let not_begun = controller.low_battery(|| {}).unwrap_err();
	assert_eq!(not_begun, NotBegun::CannotPowerOff);
	assert_eq!(not_begun.to_string(), "this device cannot power off");
	assert_eq!(controller.ask("reboot", &Peer::default()), Ok(Act::Reboot));
	assert!(steps.lock().unwrap().is_empty());
}

/// The shutdown can begin from a thread off the runtime, as the supply watcher's is.
#[tokio::test]
async fn a_low_battery_shutdown_begins_from_a_plain_thread() {
	let (controller, steps) = controller(vec![Act::PowerOff], false);
	let begun = {
		let controller = controller.clone();
		std::thread::spawn(move || controller.low_battery(|| {}))
			.join()
			.unwrap()
	};
	assert_eq!(begun, Ok(()));
	assert_eq!(
		until(&steps, 2).await,
		vec![Step::Ended, Step::Carried(Act::PowerOff)]
	);
}

/// What the run taught is recorded once the shutdown is certain and before any feed is told of it,
/// and not at all for a shutdown not begun (CHG, LOW).
#[tokio::test]
async fn the_run_is_recorded_before_going_away_and_only_when_begun() {
	let (begins, _) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let _session = begins.session();
	let mut told_first = None;
	let begun = begins.low_battery(|| told_first = Some(begins.inner.going.borrow().is_some()));
	assert_eq!(begun, Ok(()));
	assert_eq!(told_first, Some(false));

	let (cannot, _) = controller(vec![Act::Reboot], false);
	let mut ran = false;
	assert_eq!(
		cannot.low_battery(|| ran = true),
		Err(NotBegun::CannotPowerOff)
	);
	assert!(!ran);

	let (already, _) = controller(vec![Act::Reboot, Act::PowerOff], false);
	let _session = already.session();
	assert_eq!(already.ask("reboot", &Peer::default()), Ok(Act::Reboot));
	assert!(already.low_battery(|| ran = true).is_err());
	assert!(!ran);
}
