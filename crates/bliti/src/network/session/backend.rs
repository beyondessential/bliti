//! What a configuration session drives: the running system beneath it.
//!
//! The session owns the exchange of CFG and the recorded configuration; a [`Backend`] owns
//! everything that touches the network, including the capabilities it states and the check against
//! them (NET).

use std::future::Future;

use bliti_core::channel::config::{Document, Invalid, path};
use serde_json::{Map, Value as Json};

/// A verification stage of LINK, as `reached` names it on an `invalid`.
///
/// `reached` names the stage an attempt stopped at, the one that failed: `gateway` says carrier,
/// association and addressing passed and the gateway did not answer, and `carrier` says nothing
/// passed. A wired candidate has no `association`, which it neither passes nor fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
	not(test),
	expect(
		dead_code,
		reason = "the backend that verifies through these stages has not landed"
	)
)]
pub enum Stage {
	/// The interface has carrier.
	Carrier,
	/// A wireless interface has associated.
	Association,
	/// An address is held.
	Addressing,
	/// The gateway answers.
	Gateway,
}

#[cfg_attr(
	not(test),
	expect(
		dead_code,
		reason = "the backend that verifies through these stages has not landed"
	)
)]
impl Stage {
	/// The stage as `reached` carries it on the wire.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Carrier => "carrier",
			Self::Association => "association",
			Self::Addressing => "addressing",
			Self::Gateway => "gateway",
		}
	}

	/// An apply-time failure that stopped at this stage, at the part of the document named by `at`,
	/// a Normalized Path made with [`path`].
	pub fn failed(self, at: impl Into<String>, reason: impl Into<String>) -> Invalid {
		Invalid {
			at: at.into(),
			reason: reason.into(),
			reached: Some(self.as_str().to_owned()),
		}
	}
}

/// The running system a configuration session drives.
///
/// A session holds its backend exclusively, so every method may take `&mut self`. Futures are `Send`
/// because a session runs on a spawned task, and a session dropped with a proposal applied restores
/// on another.
pub trait Backend: Send + 'static {
	/// What this device supports, sent on the first `configuration` of a session. Opaque to the
	/// session, which passes it through.
	fn capabilities(&self) -> Map<String, Json>;

	/// Refuse a document asking for anything outside the capabilities, before any of it is applied
	/// (NET). The structural rules have already been applied by [`Document::parse`].
	fn check(&self, document: &Document) -> Result<(), Invalid>;

	/// Make the running system match a document, without recording it anywhere, and verify it through
	/// the stages of LINK. A failure carries the [`Stage`] it stopped at.
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
	fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		base: &Map<String, Json>,
	) -> impl Future<Output = Result<Map<String, Json>, Invalid>> + Send;
}

/// The backend this build runs until one that configures the network lands.
///
/// It configures nothing: it states no capabilities, refuses every proposal saying so, and leaves the
/// running system alone when asked to restore, so a device running it keeps whatever network its
/// image brought up.
pub struct Inert;

/// Why [`Inert`] refuses a proposal.
const INERT: &str = "this build of bliti cannot configure the network";

impl Backend for Inert {
	fn capabilities(&self) -> Map<String, Json> {
		Map::new()
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
	use bliti_core::channel::config::Segment;

	use super::*;

	#[test]
	fn stages_carry_the_wire_strings_of_link() {
		let names: Vec<&str> = [
			Stage::Carrier,
			Stage::Association,
			Stage::Addressing,
			Stage::Gateway,
		]
		.into_iter()
		.map(Stage::as_str)
		.collect();
		assert_eq!(names, ["carrier", "association", "addressing", "gateway"]);
	}

	#[test]
	fn a_failure_names_the_stage_it_stopped_at() {
		let at = path(&[Segment::Name("attachments"), Segment::Index(0)]);
		let invalid = Stage::Gateway.failed(at.clone(), "the gateway did not answer");
		assert_eq!(invalid.at, at);
		assert_eq!(invalid.reached.as_deref(), Some("gateway"));
	}

	#[tokio::test]
	async fn inert_refuses_every_proposal_before_applying_it() {
		let document = Document::parse(&Map::from_iter([(
			"attachments".to_owned(),
			Json::Array(Vec::new()),
		)]))
		.unwrap();
		let mut inert = Inert;
		assert!(inert.capabilities().is_empty());
		let refused = inert.check(&document).unwrap_err();
		assert_eq!(refused.reached, None);
		assert!(inert.apply(&document).await.is_err());
		assert!(inert.restore(&document).await.is_ok());
		assert_eq!(inert.survey(None).await, Ok(None));
	}
}
