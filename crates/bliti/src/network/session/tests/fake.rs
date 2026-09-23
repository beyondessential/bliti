//! A backend standing in for the running system, and the test's view of what it was asked.

use std::{
	sync::{Arc, Mutex as SyncMutex},
	time::Duration,
};

use bliti_core::channel::config::{Document, Invalid, path};
use serde_json::{Map, Value as Json, json};
use tokio::sync::{oneshot, watch};

use super::{super::Backend, object};

/// What the fake backend was asked to do, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
	Apply(Document),
	/// An apply whose future was dropped before it finished.
	Aborted,
	Restore(Document),
	Wps(String, Option<String>),
}

/// How the fake answers `apply`.
#[derive(Debug, Clone)]
pub enum Applying {
	Succeed,
	Fail(Invalid),
	/// Never finishes, as a verification still under way.
	Hang,
}

pub struct Shared {
	calls: Vec<Call>,
	applying: Applying,
	refuse: Option<Invalid>,
	survey: Option<Map<String, Json>>,
	joined: Result<Map<String, Json>, Invalid>,
	capabilities: Map<String, Json>,
	/// What the capabilities become once an apply succeeds, as a new regulatory domain changes them.
	after_apply: Option<Map<String, Json>>,
	/// Whether the fake publishes the state of its candidates.
	observing: bool,
	states: watch::Sender<Option<Vec<Json>>>,
	/// The PIN a join by PIN generates.
	pin: Option<String>,
}

impl Shared {
	/// Publish the states of a configuration just put in force, where the fake observes any.
	fn running(&self, document: &Document) {
		if self.observing {
			self.states
				.send_replace(Some(observed(document.attachments.len())));
		}
	}
}

/// The states a configuration of `candidates` settles into: the first carries the default route and
/// the rest wait on it.
pub fn observed(candidates: usize) -> Vec<Json> {
	(0..candidates)
		.map(|index| match index {
			0 => json!({"is": "default-route"}),
			_ => json!({"is": "standby"}),
		})
		.collect()
}

/// What the fake supports: the tests' wall port and a WPA2 network, the members the tests' newer
/// clients add, and the acts on two radios.
pub fn capabilities() -> Map<String, Json> {
	object(json!({
		"document": {
			"attachments": {"kind": {
				"wired-dynamic": {"interface": ["eth0"]},
				"wireless": {"security": {"kind": {"psk": {}}}},
			}},
			"regulatory-domain": true,
			"x-added-by-a-newer-client": true,
			"x-added-by-this-client": true,
		},
		"radios": {
			"wlan0": {"model": "onboard", "bands": ["2.4GHz"]},
			"wlx00c0caa1b2c3": {"model": "dongle", "bands": ["2.4GHz", "5GHz"]},
		},
		"acts": {
			"scan": {"interface": {"wlan0": {}, "wlx00c0caa1b2c3": {}}},
			"survey": {"interface": {"wlan0": {}}},
			"wps": {"interface": {"wlan0": {"method": ["push-button", "pin"]}}},
		},
	}))
}

/// A backend standing in for the running system, recording what it is asked to do.
pub struct Fake(Arc<SyncMutex<Shared>>);

/// The test's view of a [`Fake`].
#[derive(Clone)]
pub struct Log(Arc<SyncMutex<Shared>>);

impl Log {
	pub fn calls(&self) -> Vec<Call> {
		self.0.lock().unwrap().calls.clone()
	}

	pub fn clear(&self) {
		self.0.lock().unwrap().calls.clear();
	}

	pub fn set(&self, applying: Applying) {
		self.0.lock().unwrap().applying = applying;
	}

	pub fn refuse(&self, invalid: Invalid) {
		self.0.lock().unwrap().refuse = Some(invalid);
	}

	pub fn surveys(&self, spectrum: Map<String, Json>) {
		self.0.lock().unwrap().survey = Some(spectrum);
	}

	pub fn joins(&self, joined: Result<Map<String, Json>, Invalid>) {
		self.0.lock().unwrap().joined = joined;
	}

	pub fn capabilities(&self, capabilities: Map<String, Json>) {
		self.0.lock().unwrap().capabilities = capabilities;
	}

	pub fn after_apply(&self, capabilities: Map<String, Json>) {
		self.0.lock().unwrap().after_apply = Some(capabilities);
	}

	pub fn pins(&self, pin: &str) {
		self.0.lock().unwrap().pin = Some(pin.to_owned());
	}

	/// Publish a change of state, as the backend observes one.
	pub fn publish(&self, states: Vec<Json>) {
		self.0.lock().unwrap().states.send_replace(Some(states));
	}

	/// Wait for the backend to have been asked to do something, as a session ends on a task.
	pub async fn until(&self, predicate: impl Fn(&[Call]) -> bool) -> Vec<Call> {
		for _ in 0..200 {
			let calls = self.calls();
			if predicate(&calls) {
				return calls;
			}
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
		panic!("the backend was never asked; it saw {:?}", self.calls());
	}
}

pub fn fake() -> (Fake, Log) {
	fake_observing(false)
}

pub fn fake_observing(observing: bool) -> (Fake, Log) {
	let shared = Arc::new(SyncMutex::new(Shared {
		calls: Vec::new(),
		applying: Applying::Succeed,
		refuse: None,
		survey: None,
		joined: Err(Invalid {
			at: path(&[]),
			reason: "no access point is offering WPS".to_owned(),
			reached: None,
		}),
		capabilities: capabilities(),
		after_apply: None,
		observing,
		states: watch::Sender::new(None),
		pin: None,
	}));
	(Fake(shared.clone()), Log(shared))
}

/// Records that an apply was dropped before it finished.
struct OnAbort(Arc<SyncMutex<Shared>>);

impl Drop for OnAbort {
	fn drop(&mut self) {
		self.0.lock().unwrap().calls.push(Call::Aborted);
	}
}

impl Backend for Fake {
	fn capabilities(&self) -> Map<String, Json> {
		self.0.lock().unwrap().capabilities.clone()
	}

	fn states(&self) -> watch::Receiver<Option<Vec<Json>>> {
		self.0.lock().unwrap().states.subscribe()
	}

	fn check(&self, _document: &Document) -> Result<(), Invalid> {
		match &self.0.lock().unwrap().refuse {
			Some(invalid) => Err(invalid.clone()),
			None => Ok(()),
		}
	}

	async fn apply(&mut self, document: &Document) -> Result<(), Invalid> {
		let applying = {
			let mut shared = self.0.lock().unwrap();
			shared.calls.push(Call::Apply(document.clone()));
			shared.running(document);
			shared.applying.clone()
		};
		match applying {
			Applying::Succeed => {
				let mut shared = self.0.lock().unwrap();
				if let Some(capabilities) = shared.after_apply.take() {
					shared.capabilities = capabilities;
				}
				Ok(())
			}
			Applying::Fail(invalid) => Err(invalid),
			Applying::Hang => {
				let _abort = OnAbort(self.0.clone());
				std::future::pending().await
			}
		}
	}

	async fn restore(&mut self, document: &Document) -> anyhow::Result<()> {
		let mut shared = self.0.lock().unwrap();
		shared.calls.push(Call::Restore(document.clone()));
		shared.running(document);
		Ok(())
	}

	async fn scan(&mut self, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		Ok(vec![
			json!({"interface": interface.unwrap_or("wlan0"), "ssid": "Clinic", "signal": -61}),
		])
	}

	async fn survey(
		&mut self,
		_interface: Option<&str>,
	) -> Result<Option<Map<String, Json>>, Invalid> {
		Ok(self.0.lock().unwrap().survey.clone())
	}

	async fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		_base: &Map<String, Json>,
		pin: oneshot::Sender<String>,
	) -> Result<Map<String, Json>, Invalid> {
		let (joined, hang) = {
			let mut shared = self.0.lock().unwrap();
			shared.calls.push(Call::Wps(
				method.to_owned(),
				interface.map(ToOwned::to_owned),
			));
			if method == "pin"
				&& let Some(generated) = shared.pin.clone()
			{
				let _ = pin.send(generated);
			}
			(
				shared.joined.clone(),
				matches!(shared.applying, Applying::Hang),
			)
		};
		if hang {
			let _abort = OnAbort(self.0.clone());
			std::future::pending::<()>().await;
		}
		joined
	}
}
