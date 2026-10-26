//! A `bestool` that answers from a script, for the tests above the command.
//!
//! A start or a restart changes what the status says the way the real one would: when it runs, the
//! status becomes what the script gave for it, and it can be held on a gate so a test sees Tamanu with
//! the act under way.

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, PoisonError},
};

use async_trait::async_trait;
use tokio::sync::Semaphore;

use super::{
	acts::Act,
	run::{Output, Runner},
};

pub const RUNNING: &str = include_str!("fixtures/status-running.json");
pub const DEGRADED: &str = include_str!("fixtures/status-degraded.json");
pub const STOPPED: &str = include_str!("fixtures/status-stopped.json");
pub const DRIFT: &str = include_str!("fixtures/status-drift.json");

/// What running an act does.
struct Effect {
	/// How the command ends.
	ends: Result<(), String>,
	/// What the status says once it has.
	then: Result<Output, String>,
}

struct Inner {
	status: Result<Output, String>,
	found: Result<bool, String>,
	effects: HashMap<&'static str, Effect>,
	calls: Vec<String>,
}

pub struct Fake {
	inner: Mutex<Inner>,
	/// Held by an act until a test lets it end, where a test gates it.
	gate: Option<Arc<Semaphore>>,
}

fn reported(json: &str) -> Result<Output, String> {
	Ok(Output {
		stdout: json.to_owned(),
		failure: None,
	})
}

impl Fake {
	fn with(status: Result<Output, String>, found: Result<bool, String>) -> Self {
		Self {
			inner: Mutex::new(Inner {
				status,
				found,
				effects: HashMap::new(),
				calls: Vec::new(),
			}),
			gate: None,
		}
	}

	/// A device whose status reads as `json`.
	pub fn reporting(json: &str) -> Self {
		Self::with(reported(json), Ok(true))
	}

	/// A device with no installation: the status fails with only an error, and nothing is found.
	pub fn absent() -> Self {
		Self::with(
			Ok(Output {
				stdout: String::new(),
				failure: Some("not a Tamanu host: /etc/tamanu/env not found".to_owned()),
			}),
			Ok(false),
		)
	}

	/// A device whose program cannot be run at all.
	pub fn unrunnable(reason: &str) -> Self {
		Self::with(Err(reason.to_owned()), Err(reason.to_owned()))
	}

	/// A device where an installation is found and the status fails, saying `stderr`.
	pub fn failing(stderr: &str) -> Self {
		Self::with(
			Ok(Output {
				stdout: String::new(),
				failure: Some(stderr.to_owned()),
			}),
			Ok(true),
		)
	}

	/// A device where an installation is found and the status prints something that is not a report.
	pub fn unreadable(stdout: &str) -> Self {
		Self::with(
			Ok(Output {
				stdout: stdout.to_owned(),
				failure: None,
			}),
			Ok(true),
		)
	}

	/// Make `act` end as `ends`, leaving the status as `then`.
	pub fn on_act(self, act: Act, ends: Result<(), &str>, then: &str) -> Self {
		self.lock().effects.insert(
			act.name(),
			Effect {
				ends: ends.map_err(str::to_owned),
				then: reported(then),
			},
		);
		self
	}

	/// Hold every act until a permit is added to the returned semaphore.
	pub fn gated(mut self) -> (Self, Arc<Semaphore>) {
		let gate = Arc::new(Semaphore::new(0));
		self.gate = Some(gate.clone());
		(self, gate)
	}

	/// Change what the status says.
	pub fn set_status(&self, json: &str) {
		self.lock().status = reported(json);
	}

	/// What was run, in order.
	pub fn calls(&self) -> Vec<String> {
		self.lock().calls.clone()
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
		self.inner.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

#[async_trait]
impl Runner for Fake {
	async fn status(&self) -> Result<Output, String> {
		let mut inner = self.lock();
		inner.calls.push("status".to_owned());
		inner.status.clone()
	}

	async fn find(&self) -> Result<bool, String> {
		let mut inner = self.lock();
		inner.calls.push("find".to_owned());
		inner.found.clone()
	}

	async fn act(&self, act: Act) -> Result<(), String> {
		self.lock().calls.push(act.name().to_owned());
		if let Some(gate) = &self.gate {
			gate.acquire().await.expect("the gate stays open").forget();
		}
		let mut inner = self.lock();
		let Some(effect) = inner.effects.remove(act.name()) else {
			return Ok(());
		};
		inner.status = effect.then;
		effect.ends
	}
}
