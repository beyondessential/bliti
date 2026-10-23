//! What the configuration session drives: the running system beneath it.
//!
//! The session owns the exchange of CFG and the held configuration; a [`Backend`] owns everything
//! that touches the network, including the capabilities it states and the check against them (NET).

use std::future::Future;

use bliti_core::channel::config::{Document, Failure, Invalid, path};
use serde_json::{Map, Value as Json, json};
use tokio::sync::watch;

use crate::network::select::State;

/// The running system the configurator drives.
///
/// Every method takes `&self`, so a scan can be served while a verification runs. Futures are `Send`
/// because the configurator runs on a spawned task.
pub trait Backend: Send + Sync + 'static {
	/// What this device supports now, in the shape of NET: `document`, `acts`, and `radios` where it
	/// has any. The configurator checks every proposal against `document` and every act against
	/// `acts` before the backend sees it, and asks again whenever the backend's states change,
	/// telling each session where it changed.
	fn capabilities(&self) -> Map<String, Json>;

	/// Refuse a document for what the mirror of `capabilities.document` cannot express, such as a
	/// hotspot on a one-at-a-time radio that a candidate also needs, before any of it is applied (NET).
	/// The configurator has already applied [`Document::parse`] and the mirror.
	fn check(&self, document: &Document) -> Result<(), Invalid>;

	/// The state of each candidate of the configuration running, each entry in the shape `state`
	/// carries (CFG), as [`entry`] makes one. `None` where the backend cannot observe its candidates.
	///
	/// Before [`Self::apply`] or [`Self::restore`] returns, what the receiver holds is for the document
	/// it was given, and changes after are published as they are observed. The configurator publishes
	/// what it holds whenever it changes, provided it matches the configuration held position for
	/// position.
	fn states(&self) -> watch::Receiver<Option<Vec<Json>>>;

	/// Make the running system match `document` and verify it through the stages of LINK, judging it
	/// on the candidates it adds or changes against `replacing`. A failure of a candidate's
	/// verification carries the stage it stopped at.
	///
	/// Dropping the future abandons the verification. The system keeps running `document`.
	fn apply(
		&self,
		document: &Document,
		replacing: &Document,
	) -> impl Future<Output = Result<(), Failure>> + Send;

	/// Make the running system match a document that was settled before, without verifying it.
	fn restore(&self, document: &Document) -> impl Future<Output = Result<(), Failure>> + Send;

	/// What the radios can see, each entry in the shape the answer to `scan` carries: on `interface`
	/// alone where one is named, else on every radio able to scan.
	fn scan(
		&self,
		interface: Option<&str>,
	) -> impl Future<Output = Result<Vec<Json>, Invalid>> + Send;

	/// What the radios can see of the spectrum, on `interface` alone where one is named, else on every
	/// radio able to survey. `None` where none asked can.
	fn survey(
		&self,
		interface: Option<&str>,
	) -> impl Future<Output = Result<Option<Map<String, Json>>, Invalid>> + Send;
}

/// A candidate's state as an entry of `state` (CFG).
///
/// `verifying` carries no stage on the wire: the stage it waits on is the selector's to track. Nor
/// does `unavailable` carry the member at fault, which only a failure names.
pub fn entry(state: &State) -> Json {
	match state {
		State::DefaultRoute => json!({"is": "default-route"}),
		State::Up => json!({"is": "up"}),
		State::Verifying { .. } => json!({"is": "verifying"}),
		State::Standby => json!({"is": "standby"}),
		State::Off => json!({"is": "off"}),
		State::Unavailable {
			reached, reason, ..
		} => {
			json!({"is": "unavailable", "reached": reached.as_str(), "reason": reason})
		}
	}
}

/// The backend this build runs until one that configures the network lands.
///
/// It configures nothing: it states capabilities offering no member and no act, refuses every
/// proposal saying so, and leaves the running system alone when asked to restore, so a device running
/// it keeps whatever network its image brought up.
///
/// It publishes no states. It brings no candidate up and observes none, so every value `is` can take
/// would be a claim about the held configuration's candidates it has no grounds for: the network
/// the image brought up may or may not be the one a candidate names.
pub struct Inert;

/// Why [`Inert`] refuses a proposal.
const INERT: &str = "this build of bliti cannot configure the network";

impl Backend for Inert {
	fn capabilities(&self) -> Map<String, Json> {
		Map::from_iter([
			("document".to_owned(), Json::Object(Map::new())),
			("acts".to_owned(), Json::Object(Map::new())),
		])
	}

	fn check(&self, _document: &Document) -> Result<(), Invalid> {
		Err(Invalid {
			at: path(&[]),
			reason: INERT.to_owned(),
		})
	}

	async fn apply(&self, _document: &Document, _replacing: &Document) -> Result<(), Failure> {
		Err(Failure {
			at: path(&[]),
			reason: INERT.to_owned(),
			reached: None,
		})
	}

	async fn restore(&self, _document: &Document) -> Result<(), Failure> {
		Ok(())
	}

	fn states(&self) -> watch::Receiver<Option<Vec<Json>>> {
		watch::channel(None).1
	}

	async fn scan(&self, _interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		Err(Invalid {
			at: path(&[]),
			reason: "this build of bliti cannot scan for wireless networks".to_owned(),
		})
	}

	async fn survey(&self, _interface: Option<&str>) -> Result<Option<Map<String, Json>>, Invalid> {
		Ok(None)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn inert_refuses_every_proposal_before_applying_it() {
		let document = Document::parse(&Map::from_iter([(
			"attachments".to_owned(),
			Json::Array(Vec::new()),
		)]))
		.unwrap();
		let inert = Inert;
		assert_eq!(
			Json::Object(inert.capabilities()),
			json!({"document": {}, "acts": {}}),
			"offering no member and no act"
		);
		assert_eq!(*inert.states().borrow(), None, "observing nothing");
		assert!(inert.check(&document).is_err());
		assert!(inert.apply(&document, &document).await.is_err());
		assert!(inert.restore(&document).await.is_ok());
		assert_eq!(inert.survey(None).await, Ok(None));
	}

	#[test]
	fn a_candidates_state_is_the_entry_state_carries() {
		use crate::network::select::Stage;
		assert_eq!(entry(&State::DefaultRoute), json!({"is": "default-route"}));
		assert_eq!(entry(&State::Up), json!({"is": "up"}));
		assert_eq!(
			entry(&State::Verifying {
				at: Stage::Addressing
			}),
			json!({"is": "verifying"})
		);
		assert_eq!(entry(&State::Standby), json!({"is": "standby"}));
		assert_eq!(entry(&State::Off), json!({"is": "off"}));
		assert_eq!(
			entry(&State::Unavailable {
				reached: Stage::Carrier,
				member: &[],
				reason: "out of range".to_owned(),
			}),
			json!({"is": "unavailable", "reached": "carrier", "reason": "out of range"})
		);
	}
}
