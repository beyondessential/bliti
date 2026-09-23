//! The configuration session of CFG, served on a stream a client opened with `configure`.
//!
//! One [`Configurator`] is held device-wide and serves at most one session at a time, across every
//! connection. It holds the [`Backend`] and the one recorded configuration; a session holds both
//! exclusively for as long as it is open, which is what answers a second `configure` with `busy`.
//!
//! A proposal is applied to the running system and never recorded, so every way a session can end
//! returns the device to the recorded configuration: a `discard`, the stream closing or failing, the
//! connection's task being dropped, and a reboot, since the daemon restores the recorded
//! configuration as it starts.
//!
//! Messages are handled in the order they arrive. While a proposal is being verified, a `discard` or
//! a newer proposal interrupts it at once; anything else waits for the proposal's answer. Every
//! proposal is answered exactly once, an interrupted one with `invalid`, so a client can pair each
//! answer with the proposal it answers by counting.

use std::{collections::VecDeque, future::Future, io, pin::pin, sync::Arc};

use bliti_core::channel::{
	config::{Document, Invalid, path},
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, Stream, StreamExt, stream};
use serde_json::{Map, Value as Json};
use tokio::sync::{Mutex, OwnedMutexGuard};

use crate::session::SessionError;

pub use self::{
	backend::{Backend, Inert},
	store::{Store, default_path},
};

mod backend;
mod store;
#[cfg(test)]
mod tests;

/// The device's configuration sessions, of which at most one is open at a time.
pub struct Configurator<B> {
	state: Arc<Mutex<State<B>>>,
}

impl<B> Clone for Configurator<B> {
	fn clone(&self) -> Self {
		Self {
			state: self.state.clone(),
		}
	}
}

/// What a session holds exclusively while it is open.
struct State<B> {
	backend: B,
	store: Store,
	recorded: Proposal,
}

impl<B: Backend> State<B> {
	/// Return the running system to the recorded configuration.
	async fn restore(&mut self) {
		if let Err(err) = self.backend.restore(&self.recorded.document).await {
			tracing::error!(%err, "could not restore the recorded network configuration");
		}
	}
}

/// A document as a client sent it, and as the session acts on it.
#[derive(Debug, Clone)]
struct Proposal {
	raw: Map<String, Json>,
	document: Document,
}

impl Proposal {
	fn parse(raw: Map<String, Json>) -> Result<Self, Invalid> {
		let document = Document::parse(&raw)?;
		Ok(Self { raw, document })
	}
}

impl<B: Backend> Configurator<B> {
	/// Load the recorded configuration and put it in force, as the daemon starts.
	///
	/// `fallback` stands in where nothing has been recorded, or where what was recorded cannot be read.
	/// It is not written: the recorded configuration is replaced only on `confirm`.
	pub async fn start(
		backend: B,
		store: Store,
		fallback: Map<String, Json>,
	) -> anyhow::Result<Self> {
		let loaded = match store.load() {
			Ok(Some(raw)) => match Proposal::parse(raw) {
				Ok(recorded) => Some(recorded),
				Err(invalid) => {
					tracing::error!(
						at = %invalid.at,
						reason = %invalid.reason,
						"the recorded network configuration cannot be read; using the fallback"
					);
					None
				}
			},
			Ok(None) => {
				tracing::info!("no network configuration recorded; using the fallback");
				None
			}
			Err(err) => {
				tracing::error!(%err, "the recorded network configuration cannot be read; using the fallback");
				None
			}
		};
		let recorded = match loaded {
			Some(recorded) => recorded,
			None => Proposal::parse(fallback).map_err(|invalid| {
				anyhow::anyhow!(
					"the fallback network configuration is invalid at {:?}: {}",
					invalid.at,
					invalid.reason
				)
			})?,
		};

		// Nothing provisional survives a restart, so what runs now is what was recorded (CFG).
		let mut state = State {
			backend,
			store,
			recorded,
		};
		state.restore().await;
		Ok(Self {
			state: Arc::new(Mutex::new(state)),
		})
	}
}

/// Serve a configuration session on a stream whose `configure` has just been read, until it ends.
///
/// Where a session is already open, on this connection or another, answers `busy` and closes the
/// stream (CFG).
pub async fn serve<S, B>(stream: &mut S, configurator: &Configurator<B>) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
	B: Backend,
{
	let Ok(state) = configurator.state.clone().try_lock_owned() else {
		tracing::info!("a configuration session is already open; answering busy");
		send(stream, &Message::Busy).await?;
		let _ = stream.close().await;
		return Ok(());
	};
	tracing::info!("configuration session opened");

	let mut session = Open {
		state: Some(state),
		applied: None,
		provisional: false,
	};
	let result = session.converse(stream).await;
	session.revert().await;
	tracing::info!("configuration session ended");
	result
}

/// An open session. Dropped with the running system provisional, it restores the recorded
/// configuration on a task of its own, and holds the session closed until that is done.
struct Open<B: Backend> {
	/// Always present until the session is dropped.
	state: Option<OwnedMutexGuard<State<B>>>,
	/// The proposal applied and verified, awaiting `confirm` or `discard`.
	applied: Option<Proposal>,
	/// Whether the running system may differ from the recorded configuration: a proposal is applied,
	/// or an attempt at one was started.
	provisional: bool,
}

/// What a session does to the running system, which a `discard` or a newer one interrupts.
enum Attempt {
	Propose(Map<String, Json>),
	Wps(String, Option<String>),
}

/// How an attempt ended.
enum Outcome<T> {
	Finished(T),
	Discarded,
	Superseded(Attempt),
	Ended,
}

/// Whether the session carries on after a message.
enum Step {
	Carry,
	Ended,
}

/// The messages of a stream, read so that a read in progress survives the reader being raced against
/// an attempt and losing.
trait Incoming: Stream<Item = io::Result<Option<Vec<u8>>>> + Unpin {}
impl<T: Stream<Item = io::Result<Option<Vec<u8>>>> + Unpin> Incoming for T {}

fn incoming<R: AsyncRead + Unpin>(reader: R) -> impl Stream<Item = io::Result<Option<Vec<u8>>>> {
	stream::unfold(Some(reader), |reader| async move {
		let mut reader = reader?;
		match read_message(&mut reader).await {
			Ok(Some(raw)) => Some((Ok(Some(raw)), Some(reader))),
			ended => Some((ended, None)),
		}
	})
}

/// The next message a session acts on, or `None` at end of stream. What is skipped or refused is
/// passed over here, and a fault ends the session (MSG).
async fn next(incoming: &mut impl Incoming) -> Result<Option<Message>, SessionError> {
	loop {
		let raw = match incoming.next().await {
			Some(Ok(Some(raw))) => raw,
			Some(Ok(None)) | None => return Ok(None),
			Some(Err(err)) => return Err(SessionError::Fault(err.to_string())),
		};
		match read::<Message>(&raw) {
			Ok(Reading::Message(message)) => return Ok(Some(message)),
			Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
			Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
			Err(fault) => {
				tracing::warn!(%fault, "protocol fault; closing the configuration session");
				return Err(SessionError::Fault(fault.to_string()));
			}
		}
	}
}

async fn send<W: AsyncWrite + Unpin>(
	writer: &mut W,
	message: &Message,
) -> Result<(), SessionError> {
	write_message(writer, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}

async fn send_invalid<W: AsyncWrite + Unpin>(
	writer: &mut W,
	invalid: Invalid,
) -> Result<(), SessionError> {
	tracing::info!(at = %invalid.at, reason = %invalid.reason, reached = ?invalid.reached, "answering invalid");
	send(
		writer,
		&Message::Invalid {
			at: invalid.at,
			reason: invalid.reason,
			reached: invalid.reached,
		},
	)
	.await
}

/// A fault found before anything was applied, which reaches no stage.
fn before_applying(invalid: Invalid) -> Invalid {
	Invalid {
		reached: None,
		..invalid
	}
}

/// The answer to a proposal interrupted before it was answered, naming the whole document.
fn interrupted(reason: &str) -> Invalid {
	Invalid {
		at: path(&[]),
		reason: reason.to_owned(),
		reached: None,
	}
}

/// Race an attempt against the stream. A `discard` or a newer proposal interrupts it, dropping the
/// attempt, which aborts it; anything else waits in `deferred` for the attempt's answer.
async fn verify<T>(
	attempt: impl Future<Output = T>,
	incoming: &mut impl Incoming,
	deferred: &mut VecDeque<Message>,
) -> Result<Outcome<T>, SessionError> {
	let mut attempt = pin!(attempt);
	loop {
		tokio::select! {
			biased;

			message = next(incoming) => match message? {
				None => return Ok(Outcome::Ended),
				Some(Message::Discard) => return Ok(Outcome::Discarded),
				Some(Message::Configuration { document, .. }) => {
					return Ok(Outcome::Superseded(Attempt::Propose(document)));
				}
				Some(Message::Wps { method, interface }) => {
					return Ok(Outcome::Superseded(Attempt::Wps(method, interface)));
				}
				Some(other) => deferred.push_back(other),
			},

			result = &mut attempt => return Ok(Outcome::Finished(result)),
		}
	}
}

impl<B: Backend> Open<B> {
	fn state(&mut self) -> &mut State<B> {
		self.state
			.as_mut()
			.expect("held until the session is dropped")
	}

	/// The document the running system is meant to match: the applied proposal, or the recorded one.
	fn in_force(&mut self) -> Map<String, Json> {
		match &self.applied {
			Some(proposal) => proposal.raw.clone(),
			None => self.state().recorded.raw.clone(),
		}
	}

	async fn converse<S: AsyncRead + AsyncWrite + Unpin>(
		&mut self,
		stream: &mut S,
	) -> Result<(), SessionError> {
		let (reader, mut writer) = stream.split();
		let mut incoming = pin!(incoming(reader));
		let mut deferred = VecDeque::new();

		self.answer_configure(&mut writer).await?;
		loop {
			let message = match deferred.pop_front() {
				Some(message) => message,
				None => match next(&mut incoming).await? {
					Some(message) => message,
					None => return Ok(()),
				},
			};
			if let Step::Ended = self
				.handle(message, &mut writer, &mut incoming, &mut deferred)
				.await?
			{
				return Ok(());
			}
		}
	}

	async fn handle(
		&mut self,
		message: Message,
		writer: &mut (impl AsyncWrite + Unpin),
		incoming: &mut impl Incoming,
		deferred: &mut VecDeque<Message>,
	) -> Result<Step, SessionError> {
		match message {
			Message::Configure => self.answer_configure(writer).await?,
			Message::Configuration { document, .. } => {
				return self
					.attempt(Attempt::Propose(document), writer, incoming, deferred)
					.await;
			}
			Message::Wps { method, interface } => {
				return self
					.attempt(Attempt::Wps(method, interface), writer, incoming, deferred)
					.await;
			}
			Message::Confirm => self.confirm(writer).await?,
			Message::Discard => {
				tracing::info!("proposal discarded");
				self.revert().await;
			}
			Message::Scan { interface } => {
				match self.state().backend.scan(interface.as_deref()).await {
					Ok(networks) => {
						send(
							writer,
							&Message::Networks {
								access_points: networks,
							},
						)
						.await?
					}
					Err(invalid) => send_invalid(writer, invalid).await?,
				}
			}
			Message::Survey { interface } => {
				match self.state().backend.survey(interface.as_deref()).await {
					Ok(Some(spectrum)) => send(writer, &Message::Spectrum { spectrum }).await?,
					Ok(None) => {
						send_invalid(
							writer,
							Invalid {
								// Rooted at the act's own message: the whole `survey` is what cannot be done.
								at: path(&[]),
								reason: "no radio asked can survey its spectrum".to_owned(),
								reached: None,
							},
						)
						.await?;
					}
					Err(invalid) => send_invalid(writer, invalid).await?,
				}
			}
			Message::Hello { name, version } => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Message::Subscribe { .. }
			| Message::Fact(_)
			| Message::Reading(_)
			| Message::Applied { .. }
			| Message::State { .. }
			| Message::Pin { .. }
			| Message::Invalid { .. }
			| Message::Busy
			| Message::Networks { .. }
			| Message::Spectrum { .. } => {
				// Nothing a configuration session does anything about, which MSG makes a no-op.
				tracing::debug!("a message the configuration session has nothing to do about");
			}
		}
		Ok(Step::Carry)
	}

	/// Answer `configure` with the document in force and the capabilities (CFG).
	async fn answer_configure(
		&mut self,
		writer: &mut (impl AsyncWrite + Unpin),
	) -> Result<(), SessionError> {
		let document = self.in_force();
		let capabilities = self.state().backend.capabilities();
		send(
			writer,
			&Message::Configuration {
				document,
				capabilities: Some(capabilities),
			},
		)
		.await
	}

	/// Carry out an attempt, and each that supersedes it, answering each exactly once.
	async fn attempt(
		&mut self,
		first: Attempt,
		writer: &mut (impl AsyncWrite + Unpin),
		incoming: &mut impl Incoming,
		deferred: &mut VecDeque<Message>,
	) -> Result<Step, SessionError> {
		let mut next = Some(first);
		while let Some(attempt) = next.take() {
			let outcome = match attempt {
				Attempt::Propose(raw) => {
					let proposal = match Proposal::parse(raw).and_then(|proposal| {
						self.state().backend.check(&proposal.document)?;
						Ok(proposal)
					}) {
						Ok(proposal) => proposal,
						Err(invalid) => {
							send_invalid(writer, before_applying(invalid)).await?;
							continue;
						}
					};
					tracing::info!("applying a proposal");
					self.applied = None;
					self.provisional = true;
					let applying = self.state().backend.apply(&proposal.document);
					match verify(applying, incoming, deferred).await? {
						Outcome::Finished(Ok(())) => Outcome::Finished(Ok((proposal, false))),
						Outcome::Finished(Err(invalid)) => Outcome::Finished(Err(invalid)),
						Outcome::Discarded => Outcome::Discarded,
						Outcome::Superseded(attempt) => Outcome::Superseded(attempt),
						Outcome::Ended => Outcome::Ended,
					}
				}
				Attempt::Wps(method, interface) => {
					tracing::info!(%method, ?interface, "joining by WPS");
					let base = self.in_force();
					self.applied = None;
					self.provisional = true;
					let joining = self
						.state()
						.backend
						.wps(&method, interface.as_deref(), &base);
					match verify(joining, incoming, deferred).await? {
						Outcome::Finished(Ok(raw)) => {
							Outcome::Finished(Proposal::parse(raw).map(|proposal| (proposal, true)))
						}
						Outcome::Finished(Err(invalid)) => Outcome::Finished(Err(invalid)),
						Outcome::Discarded => Outcome::Discarded,
						Outcome::Superseded(attempt) => Outcome::Superseded(attempt),
						Outcome::Ended => Outcome::Ended,
					}
				}
			};

			match outcome {
				Outcome::Finished(Ok((proposal, announce))) => {
					// What WPS joined is a document the client has not seen, so it is sent before the
					// answer, for the client to confirm or discard like any other.
					if announce {
						send(
							writer,
							&Message::Configuration {
								document: proposal.raw.clone(),
								capabilities: None,
							},
						)
						.await?;
					}
					self.applied = Some(proposal);
					tracing::info!("proposal applied");
					send(writer, &Message::Applied { capabilities: None }).await?;
				}
				Outcome::Finished(Err(invalid)) => {
					self.revert().await;
					send_invalid(writer, invalid).await?;
				}
				Outcome::Discarded => {
					tracing::info!("proposal discarded while being verified");
					self.revert().await;
					send_invalid(writer, interrupted("discarded before it was verified")).await?;
				}
				Outcome::Superseded(attempt) => {
					tracing::info!("proposal superseded while being verified");
					send_invalid(
						writer,
						interrupted("superseded by a later proposal before it was verified"),
					)
					.await?;
					next = Some(attempt);
				}
				Outcome::Ended => return Ok(Step::Ended),
			}
		}
		Ok(Step::Carry)
	}

	/// Record the applied proposal and answer with what is now in force (CFG). With nothing applied,
	/// nothing is recorded, and the answer is the recorded configuration.
	async fn confirm(
		&mut self,
		writer: &mut (impl AsyncWrite + Unpin),
	) -> Result<(), SessionError> {
		let Some(proposal) = self.applied.take() else {
			tracing::info!(
				"confirm with nothing applied; answering with the recorded configuration"
			);
			let document = self.state().recorded.raw.clone();
			return send(
				writer,
				&Message::Configuration {
					document,
					capabilities: None,
				},
			)
			.await;
		};

		let state = self.state();
		if let Err(err) = state.store.record(proposal.raw.clone()).await {
			tracing::error!(%err, "could not record the network configuration");
			self.applied = Some(proposal);
			return send_invalid(
				writer,
				Invalid {
					at: path(&[]),
					reason: format!("could not record the configuration: {err}"),
					reached: None,
				},
			)
			.await;
		}
		tracing::info!("proposal confirmed and recorded");
		let document = proposal.raw.clone();
		state.recorded = proposal;
		self.provisional = false;
		send(
			writer,
			&Message::Configuration {
				document,
				capabilities: None,
			},
		)
		.await
	}

	/// Return to the recorded configuration, keeping nothing of the proposal (CFG).
	async fn revert(&mut self) {
		self.applied = None;
		if self.provisional {
			tracing::info!("restoring the recorded network configuration");
			self.state().restore().await;
			self.provisional = false;
		}
	}
}

impl<B: Backend> Drop for Open<B> {
	fn drop(&mut self) {
		let Some(mut state) = self.state.take() else {
			return;
		};
		if !self.provisional {
			return;
		}
		// Dropped part-way, with a proposal applied or being verified: the session's task was dropped
		// with the connection. The guard moves to the restoring task, so the next session waits for it.
		match tokio::runtime::Handle::try_current() {
			Ok(runtime) => {
				runtime.spawn(async move {
					tracing::info!(
						"configuration session dropped; restoring the recorded configuration"
					);
					state.restore().await;
				});
			}
			Err(_) => tracing::error!(
				"configuration session dropped outside a runtime; the recorded configuration is restored \
				 when the daemon next starts"
			),
		}
	}
}
