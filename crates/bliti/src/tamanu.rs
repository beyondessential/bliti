//! Reporting Tamanu's state and carrying out a client's acts on it.
//!
//! Behaviour is specified in `.workhorse/specs/control/tamanu.md` (TMU), and the facts it reports in
//! the Tamanu section of `.workhorse/specs/device-info.md` (NFO). One [`Tamanu`] serves every session,
//! since the rule that one act is under way at a time spans every stream on every channel.
//!
//! `bestool` has no feed of changes, so the state is polled, by one task that owns the latest
//! observation. Everything else reads the observation, with the act under way laid over it.

use std::{sync::Arc, time::Duration};

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::{
	sync::{mpsc, watch},
	time::Instant,
};

use crate::{
	sampler::{IDLE_STOP, Sampler},
	session::{Peer, SessionError},
};
use acts::{Act, CannotAct};
use run::Runner;
use state::{Observed, View};

pub mod acts;
mod run;
pub mod state;
pub mod status;

#[cfg(test)]
pub(crate) mod fake;

/// How long after an act's command ends Tamanu has to reach `running`, since the services come up
/// behind it (TMU, "Carrying it out").
const SETTLE: Duration = Duration::from_secs(30);

/// How long an act's command may take, which for a restart rolls through the instances behind
/// readiness probes (TMU, "Carrying it out").
const ACT: Duration = Duration::from_secs(15 * 60);

/// How often Tamanu is looked at while an act settles.
const SETTLE_POLL: Duration = Duration::from_secs(3);

/// The one handle every session shares.
#[derive(Clone)]
pub struct Tamanu {
	inner: Arc<Inner>,
}

struct Inner {
	runner: Arc<dyn Runner>,
	/// What the device reports, which holds the act under way too, so that taking and releasing the
	/// slot republish the state.
	view: Arc<watch::Sender<View>>,
}

impl Tamanu {
	/// The Tamanu of this machine, looked at while `sampler` has a session open and for as long after as
	/// it keeps sampling.
	pub async fn connect(sampler: Sampler) -> Self {
		let wanted: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(move || sampler.has_session());
		Self::start(Arc::new(run::Command::on_path()), wanted, IDLE_STOP).await
	}

	/// A device whose Tamanu is read through `runner`, looked at while `wanted` says so and for
	/// `idle_stop` after.
	///
	/// # Panics
	///
	/// Outside a Tokio runtime, which the poller runs on.
	pub(crate) async fn start(
		runner: Arc<dyn Runner>,
		wanted: Arc<dyn Fn() -> bool + Send + Sync>,
		idle_stop: Duration,
	) -> Self {
		let first = state::observe(&*runner).await;
		let (view, _) = watch::channel(View {
			observed: first,
			acting: None,
		});
		let view = Arc::new(view);
		tokio::spawn(state::poll(runner.clone(), view.clone(), wanted, idle_stop));
		Self {
			inner: Arc::new(Inner { runner, view }),
		}
	}

	/// A device with nothing to say of Tamanu, which finds none.
	#[cfg(test)]
	pub fn none() -> Self {
		let runner: Arc<dyn Runner> = Arc::new(fake::Fake::absent());
		let (view, _) = watch::channel(View {
			observed: Observed::Absent,
			acting: None,
		});
		Self {
			inner: Arc::new(Inner {
				runner,
				view: Arc::new(view),
			}),
		}
	}

	/// What the device reports of Tamanu now.
	pub fn view(&self) -> View {
		self.inner.view.borrow().clone()
	}

	/// The acts listed now.
	pub fn listed(&self) -> Vec<Act> {
		acts::listed(self.view().state())
	}

	/// Settle what to answer an act asked for, logging it with the client that asked (TMU, "Logging").
	fn ask(&self, act: &str, peer: &Peer) -> Result<Slot, CannotAct> {
		let (client, client_version) = peer.named();
		let answer = self.decide(act);
		match &answer {
			Ok(_) => {
				tracing::info!(%act, %client, %client_version, "act asked for and accepted");
			}
			Err(CannotAct { at, reason }) => {
				tracing::info!(%act, %client, %client_version, %at, %reason, "act asked for and refused");
			}
		}
		answer
	}

	fn decide(&self, act: &str) -> Result<Slot, CannotAct> {
		// First, since an act under way lists none, and TMU refuses an act then at the message as a whole.
		if let Some(under_way) = self.view().acting {
			return Err(CannotAct::busy(under_way));
		}
		let Some(listed) = Act::from_name(act).filter(|act| self.listed().contains(act)) else {
			return Err(CannotAct::not_listed(act));
		};
		Slot::take(&self.inner, listed).map_err(CannotAct::busy)
	}
}

impl Inner {
	/// Look at Tamanu now, publishing what is seen.
	async fn refresh(&self) -> Observed {
		let observed = state::observe(&*self.runner).await;
		state::publish(&self.view, observed.clone());
		observed
	}

	/// Carry `act` out, and say why where Tamanu does not reach `running`.
	async fn carry_out(&self, act: Act) -> Result<(), String> {
		let ran = tokio::time::timeout(ACT, self.runner.act(act))
			.await
			.unwrap_or_else(|_| Err(format!("the {act} took too long")));
		let settles_by = Instant::now() + SETTLE;
		let observed = loop {
			let observed = self.refresh().await;
			if observed.is_running() {
				return Ok(());
			}
			// A command that failed has said so; waiting would only wait for what it gave up on.
			if ran.is_err() || Instant::now() >= settles_by {
				break observed;
			}
			tokio::time::sleep(SETTLE_POLL).await;
		};
		let shortfall = observed.shortfall();
		Err(match ran {
			Ok(()) => shortfall,
			Err(failure) => format!("the {act} failed ({failure}); {shortfall}"),
		})
	}
}

/// The one act under way, held until it is carried out or has failed (TMU, "The exchange").
struct Slot {
	inner: Arc<Inner>,
	act: Act,
}

impl Slot {
	/// Take the slot for `act`, or return the act holding it.
	fn take(inner: &Arc<Inner>, act: Act) -> Result<Self, Act> {
		let mut already = None;
		inner.view.send_if_modified(|view| {
			already = view.acting;
			if already.is_some() {
				return false;
			}
			view.acting = Some(act);
			true
		});
		match already {
			Some(already) => Err(already),
			None => Ok(Self {
				inner: inner.clone(),
				act,
			}),
		}
	}
}

impl Drop for Slot {
	fn drop(&mut self) {
		let act = self.act;
		self.inner.view.send_modify(|view| {
			if view.acting == Some(act) {
				view.acting = None;
			}
		});
	}
}

/// Serve a Tamanu stream a client opened with `tamanu`, until it ends (TMU, "The exchange").
pub async fn serve<S>(stream: &mut S, tamanu: &Tamanu, peer: &Peer) -> Result<(), SessionError>
where
	S: futures::AsyncRead + AsyncWrite + Unpin,
{
	let (mut reader, mut writer) = stream.split();
	let mut changes = tamanu.inner.view.subscribe();

	let mut listed = tamanu.listed();
	send(&mut writer, &acts_message(&listed)).await?;

	// Read in a future of its own, so a message half read is never abandoned for another event.
	let (received, mut messages) = mpsc::unbounded_channel();
	let mut reading = std::pin::pin!(async move {
		loop {
			match read_message(&mut reader).await {
				Ok(Some(raw)) => {
					if received.send(Ok(raw)).is_err() {
						return;
					}
				}
				Ok(None) => return,
				Err(err) => {
					let _ = received.send(Err(err));
					return;
				}
			}
		}
	});
	let mut reading_done = false;

	let (outgoing, mut outbox) = mpsc::unbounded_channel::<Message>();

	loop {
		tokio::select! {
			() = &mut reading, if !reading_done => reading_done = true,

			message = messages.recv() => {
				let Some(message) = message else {
					return Ok(());
				};
				let raw = message.map_err(|err| SessionError::Fault(err.to_string()))?;
				match read::<Message>(&raw) {
					Ok(Reading::Message(Message::TamanuAct { act })) => match tamanu.ask(&act, peer) {
						Ok(slot) => {
							send(&mut writer, &Message::TamanuAccepted).await?;
							// Not owned by this stream, so the act is carried out however the stream ends.
							let inner = tamanu.inner.clone();
							let outgoing = outgoing.clone();
							tokio::spawn(async move {
								let act = slot.act;
								let outcome = inner.carry_out(act).await;
								drop(slot);
								if let Err(reason) = outcome {
									tracing::warn!(%act, %reason, "an act was accepted and not carried out");
									let _ = outgoing.send(Message::TamanuFailed { reason });
								}
							});
						}
						Err(CannotAct { at, reason }) => {
							send(&mut writer, &Message::CannotAct { act, at, reason }).await?;
						}
					},
					Ok(Reading::Message(Message::Hello { name, version })) => {
						tracing::info!(client = %name, client_version = %version, "client named itself");
						peer.name(name, version);
					}
					Ok(Reading::Message(_)) => {
						// Nothing a Tamanu stream does anything about, which MSG makes a no-op.
						tracing::debug!("a message the Tamanu stream has nothing to do about");
					}
					Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
					Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
					Err(fault) => {
						tracing::warn!(%fault, "protocol fault; closing the stream");
						return Err(SessionError::Fault(fault.to_string()));
					}
				}
			}

			Some(message) = outbox.recv() => send(&mut writer, &message).await?,

			changed = changes.changed() => {
				if changed.is_err() {
					return Ok(());
				}
				let now = tamanu.listed();
				if now != listed {
					send(&mut writer, &acts_message(&now)).await?;
					listed = now;
				}
			}
		}
	}
}

fn acts_message(listed: &[Act]) -> Message {
	Message::TamanuActs {
		acts: listed.iter().map(|act| act.name().to_owned()).collect(),
	}
}

async fn send<W: AsyncWrite + Unpin>(
	writer: &mut W,
	message: &Message,
) -> Result<(), SessionError> {
	write_message(writer, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))?;
	writer
		.flush()
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}

#[cfg(test)]
mod tests;
