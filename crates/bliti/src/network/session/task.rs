//! The task that owns the backend, the store and the configuration held (CFG).
//!
//! Sessions send it commands and read what it publishes. It serves a scan, and a proposal that
//! replaces the one being verified, while a verification runs; dropping the verification abandons it.
//! A session ending leaves all of it alone.
//!
//! Nothing reaches the backend that the capabilities do not cover: a proposal is held to the mirror
//! of `capabilities.document` and an act to `capabilities.acts` (NET), with [`bliti_core`]'s checker,
//! the one a client checks with. The backend's own [`Backend::check`] is left what the mirror cannot
//! say.

use std::{future::Future, pin::Pin};

use bliti_core::channel::{
	capabilities,
	config::{Document, Failure, Invalid, Outcome, path},
	messages::Message,
};
use futures::{StreamExt, stream::FuturesUnordered};
use serde_json::{Map, Value as Json};
use tokio::sync::{mpsc, oneshot, watch};

use super::{
	backend::Backend,
	held::{Snapshot, State},
};

/// What a session asks of the task.
pub(super) enum Command {
	/// Save a proposal, or say why it cannot be.
	Propose {
		raw: Map<String, Json>,
		reply: oneshot::Sender<Result<(), Invalid>>,
	},
	/// Carry out an act.
	Act {
		act: Act,
		reply: oneshot::Sender<Result<Message, Invalid>>,
	},
}

/// An act a client may ask for (CFG).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Act {
	Scan(Option<String>),
	Survey(Option<String>),
}

impl Act {
	/// The act's message type, which `cannot-act` names.
	pub(super) fn name(&self) -> &'static str {
		match self {
			Self::Scan(_) => "scan",
			Self::Survey(_) => "survey",
		}
	}

	fn interface(&self) -> Option<&str> {
		match self {
			Self::Scan(interface) | Self::Survey(interface) => interface.as_deref(),
		}
	}
}

type Verification<'a> = Pin<Box<dyn Future<Output = Result<(), Failure>> + Send + 'a>>;

/// What the snapshot is, given what the backend can do and observes.
pub(super) fn snapshot(
	backend: &impl Backend,
	state: &State,
	states: &watch::Receiver<Option<Vec<Json>>>,
) -> Snapshot {
	let candidates = state.document().attachments.len();
	let attachments = match &*states.borrow() {
		// A backend that observes nothing has nothing to say of its candidates.
		None => Some(Vec::new()),
		Some(entries) if entries.len() == candidates => Some(entries.clone()),
		// Published for a configuration the one held has since replaced, or not yet caught up.
		Some(_) => None,
	};
	Snapshot {
		held: state.held(),
		capabilities: backend.capabilities(),
		attachments,
	}
}

fn verifying<'a>(
	backend: &'a impl Backend,
	document: Document,
	replacing: Document,
) -> Verification<'a> {
	Box::pin(async move { backend.apply(&document, &replacing).await })
}

/// Serve the commands until every [`super::Configurator`] is gone.
///
/// A configuration whose verification a restart interrupted is verified again from here.
pub(super) async fn run<B: Backend>(
	backend: B,
	mut state: State,
	mut commands: mpsc::UnboundedReceiver<Command>,
	published: watch::Sender<Snapshot>,
) {
	let backend = &backend;
	let mut states = backend.states();
	let mut observing = true;
	let publish = |state: &State, states: &watch::Receiver<Option<Vec<Json>>>| {
		let next = snapshot(backend, state, states);
		published.send_if_modified(|current| {
			let changed = *current != next;
			if changed {
				*current = next;
			}
			changed
		});
	};

	let mut verification = (state.outcome() == Outcome::Verifying)
		.then(|| verifying(backend, state.document().clone(), state.replaced()));
	let mut acts = FuturesUnordered::new();
	loop {
		tokio::select! {
			command = commands.recv() => match command {
				None => return,
				Some(Command::Propose { raw, reply }) => {
					let saved = state.save(backend, raw).await;
					publish(&state, &states);
					match saved {
						Ok((document, replacing)) => {
							verification = Some(verifying(backend, document, replacing));
							let _ = reply.send(Ok(()));
						}
						Err(invalid) => {
							let _ = reply.send(Err(invalid));
						}
					}
				}
				Some(Command::Act { act, reply }) => {
					acts.push(async move {
						let _ = reply.send(carry_out(backend, &act).await);
					});
				}
			},
			verdict = async {
				match verification.as_mut() {
					Some(running) => running.await,
					None => std::future::pending().await,
				}
			}, if verification.is_some() => {
				verification = None;
				state.settle(verdict).await;
				publish(&state, &states);
			},
			Some(()) = acts.next(), if !acts.is_empty() => {}
			changed = states.changed(), if observing => {
				observing = changed.is_ok();
				publish(&state, &states);
			}
		}
	}
}

/// Refuse an act the capabilities do not offer, at the act's own message (NET, CFG): `$` for an act
/// not offered at all, and the member at fault for one offered but not so.
fn offered(capabilities: &Map<String, Json>, act: &Act) -> Result<(), Invalid> {
	let message: Map<String, Json> = act
		.interface()
		.map(|interface| ("interface".to_owned(), Json::from(interface)))
		.into_iter()
		.collect();
	match capabilities
		.get("acts")
		.and_then(Json::as_object)
		.and_then(|acts| acts.get(act.name()))
	{
		Some(Json::Bool(true)) => Ok(()),
		Some(Json::Object(mirror)) => capabilities::check(&message, mirror),
		_ => Err(Invalid::at(
			path(&[]),
			format!("this device does not offer `{}`", act.name()),
		)),
	}
}

async fn carry_out(backend: &impl Backend, act: &Act) -> Result<Message, Invalid> {
	offered(&backend.capabilities(), act)?;
	match act {
		Act::Scan(interface) => Ok(Message::Networks {
			access_points: backend.scan(interface.as_deref()).await?,
		}),
		Act::Survey(interface) => match backend.survey(interface.as_deref()).await? {
			Some(spectrum) => Ok(Message::Spectrum { spectrum }),
			// Rooted at the act's own message: the whole `survey` is what cannot be done.
			None => Err(Invalid::at(
				path(&[]),
				"no radio asked can survey its spectrum",
			)),
		},
	}
}
