//! What a configuration session drives: the running system beneath it.
//!
//! The session owns the exchange of CFG and the recorded configuration; a [`Backend`] owns
//! everything that touches the network, including the capabilities it states and the check against
//! them (NET).

use std::future::Future;

use bliti_core::channel::config::{Document, Invalid, path};
use serde_json::{Map, Value as Json, json};
use tokio::sync::{oneshot, watch};

use crate::network::select::State;

/// The running system a configuration session drives.
///
/// A session holds its backend exclusively, so every method may take `&mut self`. Futures are `Send`
/// because a session runs on a spawned task, and a session dropped with a proposal applied restores
/// on another.
pub trait Backend: Send + 'static {
	/// What this device supports now, in the shape of NET: `document`, `acts`, and `radios` where it
	/// has any. The session checks every proposal against `document` and every act against `acts`
	/// before the backend sees it, and asks again after each apply, telling the client where it
	/// changed.
	fn capabilities(&self) -> Map<String, Json>;

	/// Refuse a document for what the mirror of `capabilities.document` cannot express, such as a
	/// hotspot on a one-at-a-time radio that a candidate also needs, before any of it is applied (NET).
	/// The session has already applied [`Document::parse`] and the mirror.
	fn check(&self, document: &Document) -> Result<(), Invalid>;

	/// The state of each candidate of the configuration running, each entry in the shape `state`
	/// carries (CFG), as [`entry`] makes one. `None` where the backend cannot observe its candidates.
	///
	/// Before [`Self::apply`] or [`Self::restore`] returns, what the receiver holds is for the document
	/// it was given, and changes after are published as they are observed. The session sends what it
	/// holds whenever it changes, provided it matches the configuration running position for position.
	fn states(&self) -> watch::Receiver<Option<Vec<Json>>>;

	/// Make the running system match a document, without recording it anywhere, and verify it through
	/// the stages of LINK. A failure of a candidate's verification carries the stage it stopped at.
	///
	/// Dropping the future aborts the attempt; the session restores the recorded configuration after.
	fn apply(&mut self, document: &Document) -> impl Future<Output = Result<(), Invalid>> + Send;

	/// Return the running system to a recorded configuration.
	fn restore(&mut self, document: &Document) -> impl Future<Output = anyhow::Result<()>> + Send;

	/// What the radios can see, each entry in the shape the answer to `scan` carries: on `interface`
	/// alone where one is named, else on every radio able to scan.
	fn scan(
		&mut self,
		interface: Option<&str>,
	) -> impl Future<Output = Result<Vec<Json>, Invalid>> + Send;

	/// What the radios can see of the spectrum, on `interface` alone where one is named, else on every
	/// radio able to survey. `None` where none asked can.
	fn survey(
		&mut self,
		interface: Option<&str>,
	) -> impl Future<Output = Result<Option<Map<String, Json>>, Invalid>> + Send;

	/// Join a wireless network by WPS, leaving the result applied and unrecorded like any proposal.
	///
	/// `base` is the document in force; the answer is that document with the joined network added as a
	/// candidate carrying the credentials WPS yielded. Dropping the future aborts the attempt.
	///
	/// `interface` names the radio to join on; unset, the backend chooses one offering `method`.
	///
	/// Joining by PIN, the backend sends the PIN it generated on `pin` as soon as it has one, for the
	/// session to pass to the operator while the join goes on. Push-button drops it unsent.
	fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		base: &Map<String, Json>,
		pin: oneshot::Sender<String>,
	) -> impl Future<Output = Result<Map<String, Json>, Invalid>> + Send;
}

/// A candidate's state as an entry of `state` (CFG).
///
/// `verifying` carries no stage on the wire: the stage it waits on is the selector's to track.
pub fn entry(state: &State) -> Json {
	match state {
		State::DefaultRoute => json!({"is": "default-route"}),
		State::Up => json!({"is": "up"}),
		State::Verifying { .. } => json!({"is": "verifying"}),
		State::Standby => json!({"is": "standby"}),
		State::Unavailable { reached, reason } => {
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
/// would be a claim about the recorded configuration's candidates it has no grounds for: the network
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
			reached: None,
		})
	}

	async fn apply(&mut self, document: &Document) -> Result<(), Invalid> {
		self.check(document)
	}

	async fn restore(&mut self, _document: &Document) -> anyhow::Result<()> {
		Ok(())
	}

	fn states(&self) -> watch::Receiver<Option<Vec<Json>>> {
		watch::channel(None).1
	}

	async fn scan(&mut self, _interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		Err(Invalid {
			at: path(&[]),
			reason: "this build of bliti cannot scan for wireless networks".to_owned(),
			reached: None,
		})
	}

	async fn survey(
		&mut self,
		_interface: Option<&str>,
	) -> Result<Option<Map<String, Json>>, Invalid> {
		Ok(None)
	}

	async fn wps(
		&mut self,
		_method: &str,
		_interface: Option<&str>,
		_base: &Map<String, Json>,
		_pin: oneshot::Sender<String>,
	) -> Result<Map<String, Json>, Invalid> {
		Err(Invalid {
			at: path(&[]),
			reason: "this build of bliti cannot join a network by WPS".to_owned(),
			reached: None,
		})
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
		let mut inert = Inert;
		assert_eq!(
			Json::Object(inert.capabilities()),
			json!({"document": {}, "acts": {}}),
			"offering no member and no act"
		);
		assert_eq!(*inert.states().borrow(), None, "observing nothing");
		let refused = inert.check(&document).unwrap_err();
		assert_eq!(refused.reached, None);
		assert!(inert.apply(&document).await.is_err());
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
		assert_eq!(
			entry(&State::Unavailable {
				reached: Stage::Carrier,
				reason: "out of range".to_owned(),
			}),
			json!({"is": "unavailable", "reached": "carrier", "reason": "out of range"})
		);
	}
}
