//! Restarting, rebooting and powering off at a client's request, and telling every client first.
//!
//! Behaviour is specified in `.workhorse/specs/control/overview.md` (CTL). One [`Controller`] serves
//! every session, so the first act accepted anywhere is the one carried out. Accepting one runs the
//! whole of going away: every open `default` feed is told, every session is ended, the connections
//! under them are dropped, and only then is the act carried out.

use std::{
	fmt,
	sync::{Arc, Mutex, PoisonError},
	time::Duration,
};

use bliti_core::channel::{
	envelope::{Reading, read},
	messages::Message,
	stream::{read_message, write_message},
};
use futures::{AsyncRead, AsyncWrite, future::BoxFuture};
use tokio::sync::watch;
use tracing::Instrument;

use crate::session::{Peer, SessionError};

#[cfg(target_os = "linux")]
pub mod systemd;

/// How long every open feed has to hand `going-away` to its link before the sessions are ended
/// regardless (CTL, "Going away").
const TELL_TIMEOUT: Duration = Duration::from_secs(2);

/// How long every session has to close its connection before the links are dropped regardless.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

/// Something a client can ask a device to do (CTL, "The acts").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
	/// Stop bliti and start it again, leaving the rest of the system running.
	Restart,
	/// Restart the whole system.
	Reboot,
	/// Shut the system down, to stay off.
	PowerOff,
}

impl Act {
	/// The act's wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::Restart => "restart",
			Self::Reboot => "reboot",
			Self::PowerOff => "power-off",
		}
	}

	/// The act a wire name names, where it names one this build knows.
	pub fn from_name(name: &str) -> Option<Self> {
		[Self::Restart, Self::Reboot, Self::PowerOff]
			.into_iter()
			.find(|act| act.name() == name)
	}

	/// What a device carrying the act out is doing, for a refusal to say.
	fn doing(self) -> &'static str {
		match self {
			Self::Restart => "restarting",
			Self::Reboot => "rebooting",
			Self::PowerOff => "powering off",
		}
	}
}

impl fmt::Display for Act {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.name())
	}
}

/// What carries an act out once every client has been told and let go.
pub trait Power: Send + Sync + 'static {
	/// Carry `act` out. Blocking, and run off the runtime's worker threads.
	fn carry_out(&self, act: Act) -> anyhow::Result<()>;
}

/// Drops every connection to the device, beneath the sessions that ran over them.
pub type Ender = Box<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// The one controller every session shares.
#[derive(Clone)]
pub struct Controller {
	inner: Arc<Inner>,
}

struct Inner {
	acts: Vec<Act>,
	power: Arc<dyn Power>,
	ender: Ender,
	/// The act accepted and not yet failed, which every other asked for is refused against.
	accepted: Mutex<Option<Act>>,
	/// The act every open feed is to announce.
	going: watch::Sender<Option<Act>>,
	/// How many open feeds have not yet handed `going-away` to their link.
	untold: watch::Sender<usize>,
	/// Whether every session is to end.
	ending: watch::Sender<bool>,
	/// How many sessions are open.
	sessions: watch::Sender<usize>,
}

impl Controller {
	/// A controller offering `acts`, carried out by `power`, dropping connections with `ender`.
	pub fn new(acts: Vec<Act>, power: Arc<dyn Power>, ender: Ender) -> Self {
		Self {
			inner: Arc::new(Inner {
				acts,
				power,
				ender,
				accepted: Mutex::new(None),
				going: watch::Sender::new(None),
				untold: watch::Sender::new(0),
				ending: watch::Sender::new(false),
				sessions: watch::Sender::new(0),
			}),
		}
	}

	/// A controller offering nothing.
	pub fn none() -> Self {
		struct Nothing;
		impl Power for Nothing {
			fn carry_out(&self, act: Act) -> anyhow::Result<()> {
				anyhow::bail!("this device offers no act, and was asked to {act}")
			}
		}
		Self::new(
			Vec::new(),
			Arc::new(Nothing),
			Box::new(|| Box::pin(async {})),
		)
	}

	/// Hold a session open against this controller, until the guard is dropped.
	pub fn session(&self) -> SessionGuard {
		self.inner.sessions.send_modify(|open| *open += 1);
		SessionGuard {
			inner: self.inner.clone(),
			ending: self.inner.ending.subscribe(),
		}
	}

	/// Register an open `default` feed, which is to announce the act accepted.
	pub fn feed(&self) -> Feed {
		self.inner.untold.send_modify(|untold| *untold += 1);
		Feed {
			inner: self.inner.clone(),
			going: self.inner.going.subscribe(),
			counted: true,
		}
	}

	/// Settle what to answer an `act`, logging it with the client that asked (CTL, "Going away").
	fn ask(&self, name: &str, peer: &Peer) -> Result<Act, String> {
		let (client, client_version) = peer.named();
		let answer = self.decide(name);
		match &answer {
			Ok(act) => {
				tracing::info!(%act, %client, %client_version, "act asked for and accepted");
			}
			Err(reason) => {
				tracing::info!(act = %name, %client, %client_version, %reason, "act asked for and refused");
			}
		}
		answer
	}

	fn decide(&self, name: &str) -> Result<Act, String> {
		let Some(act) = Act::from_name(name).filter(|act| self.inner.acts.contains(act)) else {
			return Err(format!("this device cannot {name}"));
		};
		let mut accepted = self
			.inner
			.accepted
			.lock()
			.unwrap_or_else(PoisonError::into_inner);
		if let Some(already) = *accepted {
			return Err(format!("the device is already {}", already.doing()));
		}
		*accepted = Some(act);
		Ok(act)
	}

	/// Go away and carry out `act`, which has been accepted and answered.
	fn go(&self, act: Act) {
		let controller = self.clone();
		tokio::spawn(
			async move {
				controller.going_away(act).await;
			}
			.in_current_span(),
		);
	}

	async fn going_away(&self, act: Act) {
		let inner = &self.inner;
		inner.going.send_replace(Some(act));
		let mut untold = inner.untold.subscribe();
		if tokio::time::timeout(TELL_TIMEOUT, untold.wait_for(|untold| *untold == 0))
			.await
			.is_err()
		{
			tracing::warn!(%act, "not every feed announced the act in time; going regardless");
		}

		inner.ending.send_replace(true);
		let mut sessions = inner.sessions.subscribe();
		if tokio::time::timeout(CLOSE_TIMEOUT, sessions.wait_for(|open| *open == 0))
			.await
			.is_err()
		{
			tracing::warn!(%act, "not every session closed in time; dropping the connections");
		}
		(inner.ender)().await;

		tracing::info!(%act, "carrying out the act");
		let power = inner.power.clone();
		let carried = tokio::task::spawn_blocking(move || power.carry_out(act))
			.await
			.unwrap_or_else(|err| Err(anyhow::anyhow!("the act panicked: {err}")));
		if let Err(err) = carried {
			tracing::error!(%act, err = format!("{err:#}"), "could not carry out the act");
			// Nothing is left connected to be told, and the next client may ask again.
			inner.going.send_replace(None);
			inner.ending.send_replace(false);
			*inner
				.accepted
				.lock()
				.unwrap_or_else(PoisonError::into_inner) = None;
		}
	}
}

/// A session held open against the controller, which ends when every session is to.
pub struct SessionGuard {
	inner: Arc<Inner>,
	ending: watch::Receiver<bool>,
}

impl SessionGuard {
	/// Resolve once every session is to end.
	pub async fn ending(&mut self) {
		if self.ending.wait_for(|ending| *ending).await.is_err() {
			std::future::pending::<()>().await;
		}
	}
}

impl Drop for SessionGuard {
	fn drop(&mut self) {
		self.inner.sessions.send_modify(|open| *open -= 1);
	}
}

/// An open `default` feed, counted until it has announced the act accepted or has closed.
pub struct Feed {
	inner: Arc<Inner>,
	going: watch::Receiver<Option<Act>>,
	counted: bool,
}

impl Feed {
	/// Resolve with the act to announce, once one has been accepted. Cancel-safe.
	pub async fn going(&mut self) -> Act {
		loop {
			if let Some(act) = *self.going.borrow_and_update() {
				return act;
			}
			if self.going.changed().await.is_err() {
				std::future::pending::<()>().await;
			}
		}
	}

	/// Record that this feed has handed `going-away` to its link.
	pub fn told(&mut self) {
		if std::mem::take(&mut self.counted) {
			self.inner.untold.send_modify(|untold| *untold -= 1);
		}
	}
}

impl Drop for Feed {
	fn drop(&mut self) {
		self.told();
	}
}

/// Serve a control stream a client opened with `control`, until it ends (CTL, "The exchange").
pub async fn serve<S>(
	stream: &mut S,
	controller: &Controller,
	peer: &Peer,
) -> Result<(), SessionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let acts = controller
		.inner
		.acts
		.iter()
		.map(|act| act.name().to_owned())
		.collect();
	send(stream, &Message::Acts { acts }).await?;

	loop {
		let raw = match read_message(stream).await {
			Ok(Some(raw)) => raw,
			Ok(None) => return Ok(()),
			Err(err) => return Err(SessionError::Fault(err.to_string())),
		};
		match read::<Message>(&raw) {
			Ok(Reading::Message(Message::Act { act })) => match controller.ask(&act, peer) {
				Ok(act) => {
					send(stream, &Message::Accepted).await?;
					controller.go(act);
				}
				Err(reason) => send(stream, &Message::Refused { reason }).await?,
			},
			Ok(Reading::Message(Message::Hello { name, version })) => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Ok(Reading::Message(_)) => {
				// Nothing a control stream does anything about, which MSG makes a no-op.
				tracing::debug!("a message the control stream has nothing to do about");
			}
			Ok(Reading::Skipped(skip)) => tracing::debug!(%skip, "message passed over"),
			Ok(Reading::Refused(refusal)) => tracing::warn!(%refusal, "message refused"),
			Err(fault) => {
				tracing::warn!(%fault, "protocol fault; closing the stream");
				return Err(SessionError::Fault(fault.to_string()));
			}
		}
	}
}

async fn send<S: AsyncWrite + Unpin>(
	stream: &mut S,
	message: &Message,
) -> Result<(), SessionError> {
	write_message(stream, &message.to_json())
		.await
		.map_err(|err| SessionError::Stream(err.to_string()))
}

#[cfg(test)]
mod tests;
