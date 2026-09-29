//! Restarting, rebooting and powering off at a client's request or for a low battery, and telling
//! every client first.
//!
//! Behaviour is specified in `.workhorse/specs/control/power.md` (CTL), and the low-battery
//! shutdown's way in by `.workhorse/specs/battery/shutdown.md` (LOW). One [`Controller`] serves
//! every session and the supply watcher, so the first act accepted anywhere is the one carried out.
//! Accepting one runs the whole of going away: every open `default` feed is told, every session is
//! ended, the connections under them are dropped, and only then is the act carried out.

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
use tokio::{runtime::Handle, sync::watch};
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

/// Why a device is going away (CTL, "Going away").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
	/// A client asked for the act on a power stream.
	ManualControl,
	/// The device is powering off before its battery runs out (LOW).
	LowBattery,
}

impl Cause {
	/// The cause's wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::ManualControl => "manual-control",
			Self::LowBattery => "low-battery",
		}
	}
}

impl fmt::Display for Cause {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.name())
	}
}

/// An act accepted, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Going {
	/// The act being carried out.
	pub act: Act,
	/// Why.
	pub cause: Cause,
}

impl Going {
	/// What the device is doing, for a refusal to say.
	fn doing(self) -> String {
		match self.cause {
			Cause::ManualControl => self.act.doing().to_owned(),
			Cause::LowBattery => format!("{} for a low battery", self.act.doing()),
		}
	}
}

/// Why a low-battery shutdown was not begun.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotBegun {
	/// Powering off is not among the acts this device can carry out.
	CannotPowerOff,
	/// An act was accepted first, and is the one carried out (LOW).
	AlreadyGoing(Going),
}

impl fmt::Display for NotBegun {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::CannotPowerOff => f.write_str("this device cannot power off"),
			Self::AlreadyGoing(going) => write!(f, "the device is already {}", going.doing()),
		}
	}
}

/// What carries an act out once every client has been told and let go.
pub trait System: Send + Sync + 'static {
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
	system: Arc<dyn System>,
	ender: Ender,
	/// Where going away runs, so it can begin from a thread off the runtime.
	runtime: Handle,
	/// The act accepted and not yet failed, which every other asked for is refused against.
	accepted: Mutex<Option<Going>>,
	/// The act every open feed is to announce.
	going: watch::Sender<Option<Going>>,
	/// How many open feeds have not yet handed `going-away` to their link.
	untold: watch::Sender<usize>,
	/// Whether every session is to end.
	ending: watch::Sender<bool>,
	/// How many sessions are open.
	sessions: watch::Sender<usize>,
}

impl Controller {
	/// A controller offering `acts`, carried out by `system`, dropping connections with `ender`.
	///
	/// # Panics
	///
	/// Outside a Tokio runtime, whose handle going away runs on.
	pub fn new(acts: Vec<Act>, system: Arc<dyn System>, ender: Ender) -> Self {
		Self {
			inner: Arc::new(Inner {
				acts,
				system,
				ender,
				runtime: Handle::current(),
				accepted: Mutex::new(None),
				going: watch::Sender::new(None),
				untold: watch::Sender::new(0),
				ending: watch::Sender::new(false),
				sessions: watch::Sender::new(0),
			}),
		}
	}

	/// A controller offering nothing.
	///
	/// # Panics
	///
	/// Outside a Tokio runtime, as [`Controller::new`].
	pub fn none() -> Self {
		struct Nothing;
		impl System for Nothing {
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
				let cause = Cause::ManualControl;
				tracing::info!(%act, %cause, %client, %client_version, "act asked for and accepted");
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
		self.take(Going {
			act,
			cause: Cause::ManualControl,
		})
		.map_err(|already| format!("the device is already {}", already.doing()))?;
		Ok(act)
	}

	/// Take the one accepted slot for `going`, or return what holds it.
	fn take(&self, going: Going) -> Result<(), Going> {
		let mut accepted = self
			.inner
			.accepted
			.lock()
			.unwrap_or_else(PoisonError::into_inner);
		if let Some(already) = *accepted {
			return Err(already);
		}
		*accepted = Some(going);
		Ok(())
	}

	/// Begin powering off for a low battery, as an accepted `power-off` is carried out, telling every
	/// open feed `low-battery` as the cause (LOW, "Shutting down").
	///
	/// `before` runs once the shutdown is certain and before any feed is told, which is where what the
	/// run on battery taught is recorded (CHG). Every act asked for from here on is refused, and nothing
	/// cancels the shutdown once begun (CTL, LOW). Returns once going away has begun, without waiting
	/// for it, and may be called from a thread off the runtime. Not begun, and `before` not run, where
	/// the device cannot power off, or where an act was accepted first, which is then the one carried
	/// out.
	pub fn low_battery(&self, before: impl FnOnce()) -> Result<(), NotBegun> {
		if !self.inner.acts.contains(&Act::PowerOff) {
			return Err(NotBegun::CannotPowerOff);
		}
		let going = Going {
			act: Act::PowerOff,
			cause: Cause::LowBattery,
		};
		self.take(going).map_err(NotBegun::AlreadyGoing)?;
		tracing::info!(act = %going.act, cause = %going.cause, "act accepted");
		before();
		self.go(going);
		Ok(())
	}

	/// Go away and carry out the act of `going`, which has been accepted and answered.
	fn go(&self, going: Going) {
		let controller = self.clone();
		self.inner.runtime.spawn(
			async move {
				controller.going_away(going).await;
			}
			.in_current_span(),
		);
	}

	async fn going_away(&self, going: Going) {
		let Going { act, cause } = going;
		let inner = &self.inner;
		inner.going.send_replace(Some(going));
		let mut untold = inner.untold.subscribe();
		if tokio::time::timeout(TELL_TIMEOUT, untold.wait_for(|untold| *untold == 0))
			.await
			.is_err()
		{
			tracing::warn!(%act, %cause, "not every feed announced the act in time; going regardless");
		}

		inner.ending.send_replace(true);
		let mut sessions = inner.sessions.subscribe();
		if tokio::time::timeout(CLOSE_TIMEOUT, sessions.wait_for(|open| *open == 0))
			.await
			.is_err()
		{
			tracing::warn!(%act, %cause, "not every session closed in time; dropping the connections");
		}
		(inner.ender)().await;

		tracing::info!(%act, %cause, "carrying out the act");
		let system = inner.system.clone();
		let carried = tokio::task::spawn_blocking(move || system.carry_out(act))
			.await
			.unwrap_or_else(|err| Err(anyhow::anyhow!("the act panicked: {err}")));
		if let Err(err) = carried {
			tracing::error!(%act, %cause, err = format!("{err:#}"), "could not carry out the act");
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
	going: watch::Receiver<Option<Going>>,
	counted: bool,
}

impl Feed {
	/// Resolve with the act to announce and its cause, once one has been accepted. Cancel-safe.
	pub async fn going(&mut self) -> Going {
		loop {
			if let Some(going) = *self.going.borrow_and_update() {
				return going;
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

/// Serve a power stream a client opened with `power`, until it ends (CTL, "The exchange").
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
					controller.go(Going {
						act,
						cause: Cause::ManualControl,
					});
				}
				Err(reason) => send(stream, &Message::Refused { reason }).await?,
			},
			Ok(Reading::Message(Message::Hello { name, version })) => {
				tracing::info!(client = %name, client_version = %version, "client named itself");
			}
			Ok(Reading::Message(_)) => {
				// Nothing a power stream does anything about, which MSG makes a no-op.
				tracing::debug!("a message the power stream has nothing to do about");
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
