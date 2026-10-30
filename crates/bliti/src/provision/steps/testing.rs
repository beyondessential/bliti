//! What the tests of the steps share: a run to carry a step out in, and a switch for an operator to
//! work.

use std::{
	io,
	sync::{Arc, Mutex},
	time::Duration,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::{Answer, StepReport};

use crate::provision::{
	Act, Provisioner, Snapshot, button::Line, fake::BUILT, step::Step, store::Store,
};

/// A run holding one step, which a test drives.
pub struct Rig {
	pub provisioner: Provisioner,
	id: String,
	/// Keeps the run's state, which a run needs somewhere to be kept.
	_medium: tempfile::TempDir,
}

impl Rig {
	pub async fn new(step: Arc<dyn Step>) -> Self {
		let id = step.id().to_owned();
		let medium = tempfile::tempdir().expect("a directory");
		let store = Store::new(medium.path(), &[0xaa, 0xbb]);
		Self {
			provisioner: Provisioner::start(vec![step], Some(store), BUILT).await,
			id,
			_medium: medium,
		}
	}

	/// Start a run of the step.
	pub fn start(&self) {
		self.provisioner
			.act(&Act {
				act: "start".to_owned(),
				steps: Some(vec![self.id.clone()]),
				prompt: None,
				value: None,
			})
			.expect("the run starts");
	}

	/// Answer the prompt `prompt` with a yes or a no.
	pub fn answer(&self, prompt: &str, yes: bool) {
		self.provisioner
			.act(&Act {
				act: "answer".to_owned(),
				steps: None,
				prompt: Some(prompt.to_owned()),
				value: Some(Answer::boolean(yes)),
			})
			.expect("the answer is taken");
	}

	/// Wait until the state satisfies `until`, and return it.
	pub async fn until(&self, until: impl Fn(&Snapshot) -> bool) -> Snapshot {
		let mut changes = self.provisioner.watch();
		let held = tokio::time::timeout(
			Duration::from_secs(60 * 60),
			changes.wait_for(|snapshot| until(snapshot)),
		)
		.await
		.expect("the state never came to what was waited for")
		.expect("the provisioner outlives its watchers");
		held.clone()
	}

	/// Wait until the prompt `id` is open.
	pub async fn prompted(&self, id: &str) -> Snapshot {
		self.until(|snapshot| snapshot.prompts.iter().any(|prompt| prompt.id == id))
			.await
	}

	/// Wait until the run has stopped, passed or failed.
	pub async fn ended(&self) -> StepReport {
		use bliti_core::provisioning::report::RunState::{Failed, Finished};
		let held = self
			.until(|snapshot| matches!(snapshot.run, Failed | Finished))
			.await;
		held.steps.into_iter().next().expect("the step")
	}
}

/// The ids of the prompts open.
pub fn open(snapshot: &Snapshot) -> Vec<&str> {
	snapshot.prompts.iter().map(|p| p.id.as_str()).collect()
}

/// A switch an operator works, read as a line: high until it is pressed.
#[derive(Clone, Default)]
pub struct Switch(Arc<Mutex<Wiring>>);

#[derive(Default)]
struct Wiring {
	held: bool,
	broken: bool,
}

impl Switch {
	pub fn press(&self) {
		self.0.lock().unwrap().held = true;
	}

	pub fn release(&self) {
		self.0.lock().unwrap().held = false;
	}

	pub fn break_wire(&self) {
		self.0.lock().unwrap().broken = true;
	}

	/// Press the switch, hold it for a while and let it go, as an operator does.
	pub async fn work(&self) {
		tokio::time::sleep(Duration::from_millis(200)).await;
		self.press();
		tokio::time::sleep(Duration::from_millis(200)).await;
		self.release();
	}
}

#[async_trait]
impl Line for Switch {
	async fn is_high(&mut self) -> io::Result<bool> {
		let wiring = self.0.lock().unwrap();
		if wiring.broken {
			return Err(io::Error::other("no such line"));
		}
		Ok(!wiring.held)
	}
}
