use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

/// What happened while going away, in the order it happened.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Step {
	Ended,
	Carried(Act),
}

/// Power that records what it was asked to carry out, and fails where told to.
#[derive(Default)]
struct Recording {
	steps: Arc<Mutex<Vec<Step>>>,
	fail: AtomicBool,
}

impl Power for Recording {
	fn carry_out(&self, act: Act) -> anyhow::Result<()> {
		self.steps.lock().unwrap().push(Step::Carried(act));
		if self.fail.load(Ordering::SeqCst) {
			anyhow::bail!("told to fail");
		}
		Ok(())
	}
}

fn controller(acts: Vec<Act>, fail: bool) -> (Controller, Arc<Mutex<Vec<Step>>>) {
	let power = Recording::default();
	power.fail.store(fail, Ordering::SeqCst);
	let steps = power.steps.clone();
	let ended = steps.clone();
	let controller = Controller::new(
		acts,
		Arc::new(power),
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
	controller.go(act);

	assert_eq!(first.going().await, Act::Reboot);
	assert_eq!(second.going().await, Act::Reboot);
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
	controller.go(controller.ask("restart", &Peer::default()).unwrap());
	tokio::time::sleep(Duration::from_millis(50)).await;

	let mut late = controller.feed();
	assert_eq!(
		tokio::time::timeout(Duration::from_secs(1), late.going())
			.await
			.unwrap(),
		Act::Restart
	);
}

/// An act that fails is logged, and the device takes acts again rather than refusing every one for
/// the rest of its life (CTL).
#[tokio::test]
async fn a_failed_act_leaves_the_device_taking_acts_again() {
	let (controller, steps) = controller(vec![Act::PowerOff], true);
	controller.go(controller.ask("power-off", &Peer::default()).unwrap());
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
#[test]
fn a_controller_offering_nothing_refuses_everything() {
	let controller = Controller::none();
	assert!(controller.inner.acts.is_empty());
	assert!(controller.ask("reboot", &Peer::default()).is_err());
}
