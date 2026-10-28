//! A session's side of CFG: what it tells its client of the configuration held, and what it asks the
//! task on the client's behalf.
//!
//! The session reports the device rather than pairing requests with answers. It sends `configuration`
//! when the configuration held changes, and `state` after it and whenever anything it carries
//! changes, each from the latest the task has published.

use std::{io, pin::pin};

use bliti_core::channel::{
	config::{Failure, Invalid, Outcome, path},
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, Stream, StreamExt, stream};
use serde_json::{Map, Value as Json};
use tokio::sync::{mpsc, oneshot, watch};

use super::{
	held::Snapshot,
	holders::Registration,
	task::{Act, Command},
};
use crate::session::SessionError;

/// The messages of a stream, read so that a read in progress survives the reader being raced against
/// a change and losing.
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

fn stopped() -> SessionError {
	SessionError::Stream("the network configurator has stopped".to_owned())
}

/// What `state` last carried to this client, to send it again only where it changed.
#[derive(Debug, PartialEq)]
struct Reported {
	busy: bool,
	outcome: Outcome,
	failure: Option<Failure>,
	attachments: Vec<Json>,
}

/// An open session.
struct Open {
	commands: mpsc::UnboundedSender<Command>,
	snapshot: watch::Receiver<Snapshot>,
	registration: Registration,
	/// The capabilities the client was last told of.
	told: Option<Map<String, Json>>,
	/// The revision of the configuration last sent.
	revision: Option<u64>,
	reported: Option<Reported>,
}

/// What a session waits on, away from the borrow of its parts.
enum Woken {
	Changed(Result<(), SessionError>),
	Message(Result<Option<Message>, SessionError>),
}

/// Serve a configuration session on a stream whose `configure` has just been read, until it ends.
pub(super) async fn serve<S>(
	stream: &mut S,
	commands: mpsc::UnboundedSender<Command>,
	snapshot: watch::Receiver<Snapshot>,
	registration: Registration,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	tracing::info!(busy = registration.busy(), "configuration session opened");
	let (reader, mut writer) = stream.split();
	let mut incoming = pin!(incoming(reader));
	let mut open = Open {
		commands,
		snapshot,
		registration,
		told: None,
		revision: None,
		reported: None,
	};
	let result = open.converse(&mut writer, &mut incoming).await;
	tracing::info!("configuration session ended");
	result
}

impl Open {
	async fn converse<W: AsyncWrite + Unpin>(
		&mut self,
		writer: &mut W,
		incoming: &mut impl Incoming,
	) -> Result<(), SessionError> {
		self.sync(writer, true).await?;
		loop {
			let woken = tokio::select! {
				biased;
				changed = self.changed() => Woken::Changed(changed),
				message = next(incoming) => Woken::Message(message),
			};
			match woken {
				Woken::Changed(changed) => {
					changed?;
					self.sync(writer, false).await?;
				}
				Woken::Message(message) => match message? {
					None => return Ok(()),
					Some(message) => self.handle(message, writer).await?,
				},
			}
		}
	}

	/// Resolve once what a session tells has changed: the configuration held, or which session holds it.
	async fn changed(&mut self) -> Result<(), SessionError> {
		tokio::select! {
			changed = self.snapshot.changed() => changed.map_err(|_| stopped()),
			changed = self.registration.changed() => changed.map_err(|_| stopped()),
		}
	}

	/// Send what the client has not been told of the latest the task published: `configuration` where
	/// the configuration held changed, or at `resend`, then `state` after it and where anything it
	/// carries differs from what it last carried (CFG).
	async fn sync<W: AsyncWrite + Unpin>(
		&mut self,
		writer: &mut W,
		resend: bool,
	) -> Result<(), SessionError> {
		let snapshot = self.snapshot.borrow_and_update().clone();
		let mut capabilities = (self.told.as_ref() != Some(&snapshot.capabilities))
			.then(|| snapshot.capabilities.clone());
		if capabilities.is_some() {
			self.told = Some(snapshot.capabilities.clone());
		}

		let configured = resend || self.revision != Some(snapshot.held.revision);
		if configured {
			let message = Message::Configuration {
				document: snapshot.held.document.clone(),
				capabilities: capabilities.take(),
			};
			send(writer, &message).await?;
			self.revision = Some(snapshot.held.revision);
		}
		let Some(attachments) = snapshot.attachments else {
			// Not yet known for the configuration just held; sent once it is.
			return Ok(());
		};
		let reported = Reported {
			busy: self.registration.busy(),
			outcome: snapshot.held.outcome,
			failure: snapshot.held.failure,
			attachments,
		};
		if configured || capabilities.is_some() || self.reported.as_ref() != Some(&reported) {
			let message = Message::State {
				busy: reported.busy,
				outcome: reported.outcome,
				failure: reported.failure.clone(),
				attachments: reported.attachments.clone(),
				capabilities,
			};
			send(writer, &message).await?;
			self.reported = Some(reported);
		}
		Ok(())
	}

	/// Wait for the task's answer, sending whatever changes meanwhile.
	async fn answered<T, W: AsyncWrite + Unpin>(
		&mut self,
		writer: &mut W,
		mut answer: oneshot::Receiver<T>,
	) -> Result<T, SessionError> {
		loop {
			let woken = tokio::select! {
				biased;
				answer = &mut answer => return answer.map_err(|_| stopped()),
				changed = self.changed() => changed,
			};
			woken?;
			self.sync(writer, false).await?;
		}
	}

	async fn handle<W: AsyncWrite + Unpin>(
		&mut self,
		message: Message,
		writer: &mut W,
	) -> Result<(), SessionError> {
		match message {
			Message::Configure => {
				self.told = None;
				self.sync(writer, true).await?;
			}
			Message::Configuration { document, .. } => self.propose(document, writer).await?,
			Message::Scan { interface } => self.act(Act::Scan(interface), writer).await?,
			Message::Survey { interface } => self.act(Act::Survey(interface), writer).await?,
			Message::Hello { name, version } => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Message::Subscribe { .. }
			| Message::Fact(_)
			| Message::Reading(_)
			| Message::EndOfBatch
			| Message::State { .. }
			| Message::CannotAct { .. }
			| Message::CannotConfigure { .. }
			| Message::Networks { .. }
			| Message::Spectrum { .. }
			| Message::Power
			| Message::Acts { .. }
			| Message::Act { .. }
			| Message::Accepted
			| Message::GoingAway { .. }
			| Message::Curve
			| Message::Curves { .. }
			| Message::Load { .. }
			| Message::Reset
			| Message::Tailscale
			| Message::TailscaleActs { .. }
			| Message::TailscaleAct { .. }
			| Message::TailscaleAccepted
			| Message::TailscaleLoginLink { .. }
			| Message::TailscaleResult { .. }
			| Message::TailscaleFailed { .. }
			| Message::Tamanu
			| Message::TamanuActs { .. }
			| Message::TamanuAct { .. }
			| Message::TamanuAccepted
			| Message::TamanuFailed { .. }
			| Message::Provision
			| Message::ProvisionState { .. }
			| Message::ProvisionAct { .. }
			| Message::ProvisionAccepted => {
				// Nothing a configuration session does anything about, which MSG makes a no-op.
				tracing::debug!("a message the configuration session has nothing to do about");
			}
		}
		Ok(())
	}

	/// Propose `document` to the task. A proposal it saves is told back by the `configuration` it
	/// publishes; one it refuses is answered with `cannot-configure` and the configuration held, unchanged.
	async fn propose<W: AsyncWrite + Unpin>(
		&mut self,
		document: Map<String, Json>,
		writer: &mut W,
	) -> Result<(), SessionError> {
		let refused = if self.registration.busy() {
			Some(Invalid::at(path(&[]), BUSY))
		} else {
			let (reply, answer) = oneshot::channel();
			self.commands
				.send(Command::Propose {
					raw: document,
					reply,
				})
				.map_err(|_| stopped())?;
			self.answered(writer, answer).await?.err()
		};
		let Some(invalid) = refused else {
			return Ok(());
		};
		tracing::info!(at = %invalid.at, reason = %invalid.reason, "proposal refused");
		send(
			writer,
			&Message::CannotConfigure {
				at: invalid.at,
				reason: invalid.reason,
			},
		)
		.await?;
		self.sync(writer, true).await
	}

	/// Carry out an act through the task, or say why it cannot be.
	async fn act<W: AsyncWrite + Unpin>(
		&mut self,
		act: Act,
		writer: &mut W,
	) -> Result<(), SessionError> {
		let name = act.name();
		let answer = if self.registration.busy() {
			Err(Invalid::at(path(&[]), BUSY))
		} else {
			let (reply, answer) = oneshot::channel();
			self.commands
				.send(Command::Act { act, reply })
				.map_err(|_| stopped())?;
			self.answered(writer, answer).await?
		};
		match answer {
			Ok(message) => send(writer, &message).await,
			Err(invalid) => {
				tracing::info!(act = name, at = %invalid.at, reason = %invalid.reason, "act refused");
				let message = Message::CannotAct {
					act: name.to_owned(),
					at: invalid.at,
					reason: invalid.reason,
				};
				send(writer, &message).await
			}
		}
	}
}

/// Why a session that does not hold the configuration is refused.
const BUSY: &str = "another session is configuring this device";
