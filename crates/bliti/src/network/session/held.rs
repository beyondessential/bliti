//! The configuration a device holds, how it is faring, and what every session is told of both
//! (CFG).

use bliti_core::channel::{
	capabilities,
	config::{Document, Failure, Invalid, Outcome, path},
};
use serde_json::{Map, Value as Json};

use super::{
	backend::Backend,
	store::{Store, Stored},
};

/// A document as a client sent it, and as the device acts on it.
#[derive(Debug, Clone)]
pub(super) struct Proposal {
	pub(super) raw: Map<String, Json>,
	pub(super) document: Document,
}

impl Proposal {
	pub(super) fn parse(raw: Map<String, Json>) -> Result<Self, Invalid> {
		let document = Document::parse(&raw)?;
		Ok(Self { raw, document })
	}
}

/// What every session is told of the configuration held.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Held {
	/// Counted at each save, so a proposal equal to the one held is still told to the session that
	/// made it.
	pub(super) revision: u64,
	pub(super) document: Map<String, Json>,
	pub(super) outcome: Outcome,
	pub(super) failure: Option<Failure>,
}

/// What the configurator publishes for every session to read.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Snapshot {
	pub(super) held: Held,
	pub(super) capabilities: Map<String, Json>,
	/// The state of each candidate of the held configuration. `None` while the backend has yet to
	/// publish the states of the configuration just saved.
	pub(super) attachments: Option<Vec<Json>>,
}

/// What the configurator owns of the configuration held.
pub(super) struct State {
	store: Store,
	held: Proposal,
	outcome: Outcome,
	failure: Option<Failure>,
	/// The configuration the held one replaced, while it is being verified.
	replaced: Option<Proposal>,
	revision: u64,
}

impl State {
	/// What was recorded, or `fallback` where nothing was or what was cannot be read, the default
	/// being settled as there is nothing in it to judge.
	pub(super) fn load(store: Store, fallback: Map<String, Json>) -> anyhow::Result<Self> {
		match store.load() {
			Ok(Some(stored)) => match Self::stored(&store, stored) {
				Ok(state) => return Ok(state),
				Err(invalid) => tracing::error!(
					at = %invalid.at,
					reason = %invalid.reason,
					"the held network configuration cannot be read; using the default"
				),
			},
			Ok(None) => tracing::info!("no network configuration held; using the default"),
			Err(err) => {
				tracing::error!(%err, "the held network configuration cannot be read; using the default");
			}
		}
		let held = Proposal::parse(fallback).map_err(|invalid| {
			anyhow::anyhow!(
				"the default network configuration is invalid at {:?}: {}",
				invalid.at,
				invalid.reason
			)
		})?;
		Ok(Self {
			store,
			held,
			outcome: Outcome::Applied,
			failure: None,
			replaced: None,
			revision: 0,
		})
	}

	fn stored(store: &Store, stored: Stored) -> Result<Self, Invalid> {
		let held = Proposal::parse(stored.document)?;
		let replaced = stored.replaced.map(Proposal::parse).transpose()?;
		Ok(Self {
			store: store.clone(),
			held,
			outcome: stored.outcome,
			failure: stored.failure,
			replaced,
			revision: 0,
		})
	}

	pub(super) fn outcome(&self) -> Outcome {
		self.outcome
	}

	pub(super) fn document(&self) -> &Document {
		&self.held.document
	}

	/// The configuration whose verification a restart interrupted replaced, to judge it against again.
	pub(super) fn replaced(&self) -> Document {
		self.replaced
			.as_ref()
			.map_or_else(empty, |replaced| replaced.document.clone())
	}

	/// A restore that could not be carried out: the configuration cannot be run, whatever it was
	/// judged. Not recorded, so the next start tries it again.
	pub(super) fn cannot_run(&mut self, failure: Failure) {
		self.outcome = Outcome::Failed;
		self.failure = Some(failure);
	}

	pub(super) fn held(&self) -> Held {
		Held {
			revision: self.revision,
			document: self.held.raw.clone(),
			outcome: self.outcome,
			failure: self.failure.clone(),
		}
	}

	fn stored_now(&self) -> Stored {
		Stored {
			document: self.held.raw.clone(),
			outcome: self.outcome,
			failure: self.failure.clone(),
			replaced: self.replaced.as_ref().map(|replaced| replaced.raw.clone()),
		}
	}

	/// Refuse a proposal the device cannot take, or save it: record it as the configuration held,
	/// replacing the one before, before any of it is applied. Returns what to verify it against.
	///
	/// A verification still under way leaves what the configuration it started from was judged
	/// against standing, as what it added was never judged.
	pub(super) async fn save(
		&mut self,
		backend: &impl Backend,
		raw: Map<String, Json>,
	) -> Result<(Document, Document), Invalid> {
		let proposal = Proposal::parse(raw)?;
		capabilities::admits(&proposal.raw, &backend.capabilities())
			.map_err(|refused| refused.invalid)?;
		backend.check(&proposal.document)?;

		let replaced = match (self.outcome, &self.replaced) {
			(Outcome::Verifying, Some(replaced)) => replaced.clone(),
			_ => self.held.clone(),
		};
		let next = Stored {
			document: proposal.raw.clone(),
			outcome: Outcome::Verifying,
			failure: None,
			replaced: Some(replaced.raw.clone()),
		};
		if let Err(err) = self.store.record(&next).await {
			tracing::error!(%err, "could not record the network configuration");
			return Err(Invalid::at(
				path(&[]),
				format!("could not record the configuration: {err}"),
			));
		}
		tracing::info!("network configuration saved");
		let verify = (proposal.document.clone(), replaced.document.clone());
		self.held = proposal;
		self.outcome = Outcome::Verifying;
		self.failure = None;
		self.replaced = Some(replaced);
		self.revision += 1;
		Ok(verify)
	}

	/// Take in how the verification of the configuration held ended, and record it.
	pub(super) async fn settle(&mut self, verdict: Result<(), Failure>) {
		match verdict {
			Ok(()) => {
				tracing::info!("network configuration applied");
				self.outcome = Outcome::Applied;
				self.failure = None;
			}
			Err(failure) => {
				tracing::info!(at = %failure.at, reason = %failure.reason, reached = ?failure.reached, "network configuration failed");
				self.outcome = Outcome::Failed;
				self.failure = Some(failure);
			}
		}
		self.replaced = None;
		if let Err(err) = self.store.record(&self.stored_now()).await {
			tracing::error!(%err, "could not record the outcome of the network configuration");
		}
	}
}

fn empty() -> Document {
	Document {
		attachments: Vec::new(),
		hotspot: None,
		regulatory_domain: None,
	}
}
