//! `bliti battery-curve`: the curve document in force written out, a document loaded, or the curves
//! reset, at the device (CRV, "At the device").
//!
//! Each goes through the running daemon's socket where a daemon answers there, so it takes effect in
//! the daemon without a restart, as one over a curve stream does; the daemon logs it. Where none
//! answers, the curve file is read or written directly and the daemon finds it when it next starts.
//! Either way the command line logs what it did, and a refusal is an error carrying its reason.

use std::{
	io::{self, Read, Write},
	path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde_json::Value as Json;

use crate::facts::curve::{
	Document,
	store::{self, Store, Stored},
};

/// What the command line is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
	/// Write the curve document in force to standard output.
	Export,
	/// Load a curve document, as given: the device validates it.
	Import(Json),
	/// Return to the curve the build carries.
	Reset,
}

impl Action {
	fn name(&self) -> &'static str {
		match self {
			Self::Export => "export",
			Self::Import(_) => "load",
			Self::Reset => "reset",
		}
	}
}

/// Where the daemon listens and where the curve file is.
#[derive(Debug, Clone)]
pub struct Paths {
	pub socket: PathBuf,
	pub store: PathBuf,
}

impl Default for Paths {
	fn default() -> Self {
		Self {
			socket: PathBuf::from(DEFAULT_SOCKET),
			store: PathBuf::from(store::DEFAULT_PATH),
		}
	}
}

#[cfg(unix)]
const DEFAULT_SOCKET: &str = super::socket::PATH;
#[cfg(not(unix))]
const DEFAULT_SOCKET: &str = "";

/// A curve document from a file, or from standard input where `source` is `-`.
pub fn read_document(source: &Path) -> Result<Json> {
	let text = if source == Path::new("-") {
		let mut text = String::new();
		io::stdin()
			.read_to_string(&mut text)
			.context("reading the curve document from standard input")?;
		text
	} else {
		std::fs::read_to_string(source)
			.with_context(|| format!("reading the curve document from {}", source.display()))?
	};
	serde_json::from_str(&text).context("the curve document is not JSON")
}

/// Carry `action` out, through the daemon where one answers and on the curve file otherwise,
/// writing an exported document to `out`.
pub async fn run(action: Action, paths: &Paths, out: &mut dyn Write) -> Result<()> {
	#[cfg(unix)]
	if let Some(answer) = daemon::ask(&paths.socket, &action).await? {
		return settle(&action, answer, "the running daemon", out);
	}
	let store = Store::new(&paths.store);
	tracing::info!(file = %paths.store.display(), "no daemon answers; working on the curve file directly");
	let answer = directly(&action, &store).await;
	settle(&action, answer, "the curve file", out)
}

/// What came of an action.
#[derive(Debug)]
enum Answer {
	/// The document in force, none where no backup supply is managed.
	Document(Option<Json>),
	Accepted,
	Refused(String),
}

fn settle(action: &Action, answer: Answer, place: &str, out: &mut dyn Write) -> Result<()> {
	let asked = action.name();
	match answer {
		Answer::Document(Some(document)) => {
			serde_json::to_writer_pretty(&mut *out, &document)?;
			writeln!(out)?;
			Ok(())
		}
		Answer::Document(None) => {
			bail!("this device manages no backup supply, so it holds no curve document")
		}
		Answer::Accepted => {
			tracing::info!(%asked, to = %place, "curve change accepted");
			Ok(())
		}
		Answer::Refused(reason) => {
			tracing::info!(%asked, to = %place, %reason, "curve change refused");
			bail!(reason)
		}
	}
}

/// Carry `action` out on the curve file, keeping the gauge's full reading, which is the device's and
/// not the document's, across a load or a reset, as the daemon does.
async fn directly(action: &Action, store: &Store) -> Answer {
	let action = action.clone();
	let store = store.clone();
	tokio::task::spawn_blocking(move || match action {
		Action::Export => Answer::Document(Some(store.load().document.to_json())),
		Action::Import(document) => match Document::from_json(&document) {
			Ok(document) => replace(&store, document),
			Err(invalid) => Answer::Refused(invalid.to_string()),
		},
		Action::Reset => replace(&store, Document::shipped()),
	})
	.await
	.unwrap_or_else(|err| Answer::Refused(format!("the curve file could not be changed: {err}")))
}

fn replace(store: &Store, document: Document) -> Answer {
	let stored = Stored {
		document,
		gauge_full: store.load().gauge_full,
	};
	match store.save(&stored) {
		Ok(()) => Answer::Accepted,
		Err(err) => Answer::Refused(format!("the curves could not be saved: {err}")),
	}
}

/// The running daemon, reached through its socket.
#[cfg(unix)]
mod daemon {
	use std::{io, path::Path, pin::Pin, time::Duration};

	use anyhow::{Context, Result, bail};
	use bliti_core::channel::{
		envelope::{Reading, read},
		messages::Message,
	};
	use futures::{Stream, StreamExt};
	use tokio::net::UnixStream;
	use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

	use super::{Action, Answer};
	use crate::battery::{Carrier, incoming};

	/// What the command line names itself as to the daemon, for its log (CRV).
	const NAME: &str = "bliti battery-curve";
	const VERSION: &str = env!("BLITI_VERSION");

	/// How long the daemon has to answer, a curve file written included.
	const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);

	/// Ask the daemon listening on `socket`, or `None` where no daemon listens there.
	pub(super) async fn ask(socket: &Path, action: &Action) -> Result<Option<Answer>> {
		let stream = match UnixStream::connect(socket).await {
			Ok(stream) => stream,
			Err(err)
				if matches!(
					err.kind(),
					io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
				) =>
			{
				return Ok(None);
			}
			Err(err) => {
				return Err(err)
					.with_context(|| format!("reaching the daemon at {}", socket.display()));
			}
		};
		tokio::time::timeout(ANSWER_TIMEOUT, exchange(stream, action))
			.await
			.context("the daemon did not answer in time")?
			.map(Some)
	}

	async fn exchange(stream: UnixStream, action: &Action) -> Result<Answer> {
		let (reader, writer) = stream.into_split();
		let mut writer = writer.compat_write();
		let mut incoming = std::pin::pin!(incoming(reader.compat(), Carrier::Socket));

		let hello = Message::Hello {
			name: NAME.to_owned(),
			version: VERSION.to_owned(),
		};
		for message in [hello, Message::Curve] {
			Carrier::Socket.write(&mut writer, &message).await?;
		}
		let document = match next(incoming.as_mut()).await? {
			Message::Curves { document, .. } => document,
			other => bail!("the daemon answered `curve` with {other:?}"),
		};

		let asked = match action {
			Action::Export => return Ok(Answer::Document(document)),
			Action::Import(document) => Message::Load {
				document: document.clone(),
			},
			Action::Reset => Message::Reset,
		};
		Carrier::Socket.write(&mut writer, &asked).await?;
		loop {
			match next(incoming.as_mut()).await? {
				Message::Accepted => return Ok(Answer::Accepted),
				Message::Refused { reason } => return Ok(Answer::Refused(reason)),
				// What the curves were or have become, which the answer settles.
				Message::Curves { .. } => {}
				other => bail!("the daemon answered with {other:?}"),
			}
		}
	}

	/// The next message the daemon sends, passing over what this build does not know.
	async fn next(
		mut incoming: Pin<&mut impl Stream<Item = io::Result<Vec<u8>>>>,
	) -> Result<Message> {
		loop {
			let Some(raw) = incoming.next().await else {
				bail!("the daemon closed the connection without answering");
			};
			match read::<Message>(&raw.context("reading from the daemon")?) {
				Ok(Reading::Message(message)) => return Ok(message),
				Ok(Reading::Skipped(_) | Reading::Refused(_)) => {}
				Err(fault) => bail!("the daemon sent something that is not a message: {fault}"),
			}
		}
	}
}
