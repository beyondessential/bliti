//! A backend standing in for the running system, and the test's view of what it was asked.

use std::{
	sync::{Arc, Mutex as SyncMutex},
	time::Duration,
};

use bliti_core::channel::config::{Document, Failure, Invalid};
use serde_json::{Map, Value as Json, json};
use tokio::sync::watch;

use super::{super::Backend, object};

/// What the fake backend was asked to do, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Call {
	/// A document to verify, and the one it replaces.
	Apply(Document, Document),
	/// An apply whose future was dropped before it finished.
	Aborted,
	Restore(Document),
}

/// How the fake answers `apply`.
#[derive(Debug, Clone)]
pub enum Applying {
	Succeed,
	Fail(Failure),
	/// Does not finish until [`Log::release`] says how, as a verification still under way.
	Held,
}

pub struct Shared {
	calls: Vec<Call>,
	applying: Applying,
	refuse: Option<Invalid>,
	survey: Option<Map<String, Json>>,
	/// What putting a settled configuration in force fails with.
	restoring: Option<Failure>,
	/// How a verification [`Applying::Held`] ends, once it does.
	release: watch::Sender<Option<Result<(), Failure>>>,
	capabilities: Map<String, Json>,
	/// What the capabilities become once an apply succeeds, as a new regulatory domain changes them.
	after_apply: Option<Map<String, Json>>,
	/// Whether the fake publishes the state of its candidates.
	observing: bool,
	states: watch::Sender<Option<Vec<Json>>>,
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
			"wlan0": {"model": "onboard", "bands": ["2ghz"]},
			"wlx00c0caa1b2c3": {"model": "dongle", "bands": ["2ghz", "5ghz"]},
		},
		"acts": {
			"scan": {"interface": {"wlan0": {}, "wlx00c0caa1b2c3": {}}},
			"survey": {"interface": {"wlan0": {}}},
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

	/// Fail putting a settled configuration in force.
	pub fn cannot_restore(&self, failure: Failure) {
		self.0.lock().unwrap().restoring = Some(failure);
	}

	/// End a verification held by [`Applying::Held`].
	pub fn release(&self, verdict: Result<(), Failure>) {
		self.0.lock().unwrap().release.send_replace(Some(verdict));
	}

	pub fn capabilities(&self, capabilities: Map<String, Json>) {
		self.0.lock().unwrap().capabilities = capabilities;
	}

	pub fn after_apply(&self, capabilities: Map<String, Json>) {
		self.0.lock().unwrap().after_apply = Some(capabilities);
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
		restoring: None,
		release: watch::Sender::new(None),
		capabilities: capabilities(),
		after_apply: None,
		observing,
		states: watch::Sender::new(None),
	}));
	(Fake(shared.clone()), Log(shared))
}

/// Records that an apply was dropped before it finished.
struct OnAbort(Option<Arc<SyncMutex<Shared>>>);

impl OnAbort {
	fn finished(mut self) {
		self.0 = None;
	}
}

impl Drop for OnAbort {
	fn drop(&mut self) {
		if let Some(shared) = &self.0 {
			shared.lock().unwrap().calls.push(Call::Aborted);
		}
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

	async fn apply(&self, document: &Document, replacing: &Document) -> Result<(), Failure> {
		let (applying, mut release) = {
			let mut shared = self.0.lock().unwrap();
			shared
				.calls
				.push(Call::Apply(document.clone(), replacing.clone()));
			shared.running(document);
			shared.release.send_replace(None);
			(shared.applying.clone(), shared.release.subscribe())
		};
		match applying {
			Applying::Succeed => {
				let mut shared = self.0.lock().unwrap();
				if let Some(capabilities) = shared.after_apply.take() {
					shared.capabilities = capabilities;
				}
				Ok(())
			}
			Applying::Fail(failure) => Err(failure),
			Applying::Held => {
				let abort = OnAbort(Some(self.0.clone()));
				let verdict = release
					.wait_for(Option::is_some)
					.await
					.expect("the fake outlives its verification")
					.clone()
					.expect("waited for a verdict");
				if verdict.is_ok() {
					let mut shared = self.0.lock().unwrap();
					if let Some(capabilities) = shared.after_apply.take() {
						shared.capabilities = capabilities;
					}
				}
				abort.finished();
				verdict
			}
		}
	}

	async fn restore(&self, document: &Document) -> Result<(), Failure> {
		let mut shared = self.0.lock().unwrap();
		shared.calls.push(Call::Restore(document.clone()));
		shared.running(document);
		match shared.restoring.clone() {
			Some(failure) => Err(failure),
			None => Ok(()),
		}
	}

	async fn scan(&self, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		Ok(vec![
			json!({"interface": interface.unwrap_or("wlan0"), "ssid": "Clinic", "signal": -61}),
		])
	}

	async fn survey(&self, _interface: Option<&str>) -> Result<Option<Map<String, Json>>, Invalid> {
		Ok(self.0.lock().unwrap().survey.clone())
	}
}
