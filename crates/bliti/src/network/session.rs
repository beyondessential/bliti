//! The configuration session of CFG, served on a stream a client opened with `configure`.
//!
//! One [`Configurator`] is held device-wide, and any number of sessions may be open on it, across every
//! connection. A [`task`] owns the [`Backend`], the [`Store`] and the one configuration the device
//! holds, with how it is faring; a session ending changes none of that, nor interrupts a verification
//! under way. Sessions send the task commands and read what it publishes, each telling its client of
//! the configuration held and its `state` as they change.
//!
//! The session open longest holds the configuration, and every other is busy: it is told of every
//! change but has its proposals and acts refused ([`holders`]).

use bliti_core::channel::config::Outcome;
use serde_json::{Map, Value as Json};
use tokio::sync::{mpsc, watch};

use crate::session::SessionError;

pub use self::{
	backend::{Backend, Inert, entry},
	store::{Store, default_path},
};
use self::{
	held::State,
	holders::Holders,
	task::Command,
};

mod backend;
mod held;
mod holders;
mod open;
mod store;
mod task;
#[cfg(test)]
mod tests;

/// The device's configuration, which every session reads and the longest-open one changes.
#[derive(Clone)]
pub struct Configurator {
	commands: mpsc::UnboundedSender<Command>,
	snapshot: watch::Receiver<held::Snapshot>,
	holders: Holders,
}

impl Configurator {
	/// Load the configuration held and put it in force, as the daemon starts, then run the task that
	/// owns it. One whose verification a restart interrupted is verified again, against the
	/// configuration it replaced.
	///
	/// `fallback` is what a device that has never saved a proposal holds. It stands too where what
	/// was recorded cannot be read, as the file a build before this shape left. It is not written
	/// until a proposal is saved.
	pub async fn start<B: Backend>(
		backend: B,
		store: Store,
		fallback: Map<String, Json>,
	) -> anyhow::Result<Self> {
		let mut state = State::load(store, fallback)?;
		if state.outcome() != Outcome::Verifying
			&& let Err(failure) = backend.restore(state.document()).await
		{
			tracing::error!(at = %failure.at, reason = %failure.reason, "could not put the held network configuration in force");
			state.cannot_run(failure);
		}

		let initial = task::snapshot(&backend, &state, &backend.states());
		let (published, snapshot) = watch::channel(initial);
		let (commands, receiver) = mpsc::unbounded_channel();
		tokio::spawn(task::run(backend, state, receiver, published));
		Ok(Self {
			commands,
			snapshot,
			holders: Holders::new(),
		})
	}
}

/// Serve a configuration session on a stream whose `configure` has just been read, until it ends.
///
/// The session opens behind every one already open, which hold the configuration ahead of it (CFG).
pub async fn serve<S>(stream: &mut S, configurator: &Configurator) -> Result<(), SessionError>
where
	S: futures::AsyncRead + futures::AsyncWrite + Unpin,
{
	open::serve(
		stream,
		configurator.commands.clone(),
		configurator.snapshot.clone(),
		configurator.holders.register(),
	)
	.await
}
