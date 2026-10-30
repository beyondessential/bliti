//! Carrying out the acts once they are accepted (TSC).

use std::time::Duration;

use bliti_core::channel::messages::{Message, Secret};
use serde_json::{Map, Value as Json};
use tokio::sync::{broadcast, mpsc, watch};

use super::{
	Daemon, Inner,
	acts::{Method, Request},
	netcheck, ping,
	state::{Observed, State},
};

/// How long a change of state has to arrive, since TSC leaves the bound to the device.
const TRANSITION: Duration = Duration::from_secs(30);

/// How long a sign-in has, long enough for a person to open a login link.
const SIGN_IN: Duration = Duration::from_secs(180);

/// How an act ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
	/// Carried out, which a client reads from `tailscale-state`.
	Done,
	/// Carried out, with a result to report.
	Result(Map<String, Json>),
	/// Not carried out, and why.
	Failed(String),
}

impl Outcome {
	/// What the stream tells the client the act ended in, if anything.
	pub fn message(self) -> Option<Message> {
		match self {
			Self::Done => None,
			Self::Result(result) => Some(Message::TailscaleResult { result }),
			Self::Failed(reason) => Some(Message::TailscaleFailed { reason }),
		}
	}
}

/// An act carried out by one call.
#[derive(Debug, Clone, Copy)]
enum Change {
	TurnOn,
	TurnOff,
	LogOut,
}

impl Change {
	fn what(self) -> &'static str {
		match self {
			Self::TurnOn => "turn on",
			Self::TurnOff => "turn off",
			Self::LogOut => "log out",
		}
	}
}

impl Inner {
	/// Carry `request` out, sending each login link a sign-in is issued to `links`.
	pub(super) async fn carry_out(
		&self,
		request: &Request,
		links: &mpsc::UnboundedSender<Message>,
	) -> Outcome {
		let outcome = match (request, &self.daemon) {
			(Request::Netcheck, daemon) => self.netcheck(daemon.as_ref()).await,
			(_, None) => Outcome::Failed("this device runs no Tailscale".to_owned()),
			(Request::TurnOn, Some(daemon)) => {
				self.transition(daemon, Change::TurnOn, |state| {
					matches!(state, State::Running | State::Starting)
				})
				.await
			}
			(Request::TurnOff, Some(daemon)) => {
				self.transition(daemon, Change::TurnOff, |state| state == State::Stopped)
					.await
			}
			(Request::LogOut, Some(daemon)) => {
				self.transition(daemon, Change::LogOut, |state| state == State::NeedsLogin)
					.await
			}
			(Request::SignIn { method, name }, Some(daemon)) => {
				self.sign_in(daemon, method, name.as_deref(), links).await
			}
			(Request::Ping { peer }, Some(daemon)) => self.ping(daemon, peer.as_deref()).await,
		};
		if let Outcome::Failed(reason) = &outcome {
			tracing::info!(act = %request.act(), %reason, "act accepted and not carried out");
		}
		outcome
	}

	/// A sign-in by auth key for a caller that is not a client: a machine that holds a sign-in is
	/// signed out first, so the key's replaces it. Whether the machine then waits on approval.
	pub(super) async fn sign_in_by_key(&self, key: Secret, name: &str) -> Result<bool, String> {
		let Some(daemon) = &self.daemon else {
			return Err("this device runs no Tailscale".to_owned());
		};
		let mut observed = daemon.observed.clone();
		let settled = |state: State| state != State::Starting;
		if !reach(&mut observed, TRANSITION, &settled).await {
			return Err("Tailscale did not answer in time".to_owned());
		}
		let state = observed.borrow().state();
		if !matches!(state, Some(State::NeedsLogin | State::Expired)) {
			let signed_out = self
				.transition(daemon, Change::LogOut, |state| state == State::NeedsLogin)
				.await;
			if let Outcome::Failed(reason) = signed_out {
				return Err(reason);
			}
		}
		let (links, _unheard) = mpsc::unbounded_channel();
		let outcome = self
			.sign_in(daemon, &Method::AuthKey(key), Some(name), &links)
			.await;
		match outcome {
			Outcome::Failed(reason) => Err(reason),
			Outcome::Done | Outcome::Result(_) => {
				Ok(observed.borrow().state() == Some(State::NeedsApproval))
			}
		}
	}

	/// An act carried out by one call, and done when Tailscale reaches a state `target` accepts.
	async fn transition(
		&self,
		daemon: &Daemon,
		change: Change,
		target: impl Fn(State) -> bool,
	) -> Outcome {
		let mut observed = daemon.observed.clone();
		let what = change.what();
		let called = match change {
			Change::TurnOn => daemon.api.set_running(true).await,
			Change::TurnOff => daemon.api.set_running(false).await,
			Change::LogOut => daemon.api.logout().await,
		};
		if let Err(err) = called {
			return Outcome::Failed(format!("cannot {what}: {err}"));
		}
		match reach(&mut observed, TRANSITION, &target).await {
			true => Outcome::Done,
			false => Outcome::Failed(format!("Tailscale did not {what} in time")),
		}
	}

	async fn sign_in(
		&self,
		daemon: &Daemon,
		method: &Method,
		name: Option<&str>,
		links: &mpsc::UnboundedSender<Message>,
	) -> Outcome {
		let key = match method {
			Method::AuthKey(key) => Some(key.clone()),
			Method::LoginLink => None,
		};
		// Subscribed before anything is asked, so nothing the bus announces in answer is missed.
		let mut events = daemon.events.subscribe();
		let mut observed = daemon.observed.clone();

		let started = async {
			let mut prefs = daemon.api.prefs().await?;
			prefs.insert("WantRunning".to_owned(), true.into());
			prefs.insert("Hostname".to_owned(), name.unwrap_or_default().into());
			daemon.api.start(prefs, key.clone()).await?;
			daemon.api.login_interactive().await
		};
		if let Err(err) = started.await {
			return Outcome::Failed(scrub(&format!("cannot sign in: {err}"), key.as_ref()));
		}

		let joined = |observed: &Observed| {
			matches!(
				observed.state(),
				Some(State::Running | State::NeedsApproval)
			)
		};
		let deadline = tokio::time::sleep(SIGN_IN);
		tokio::pin!(deadline);
		loop {
			tokio::select! {
				reached = observed.wait_for(joined) => {
					if reached.is_err() {
						return Outcome::Failed("Tailscale is not being watched".to_owned());
					}
					break;
				}
				event = events.recv() => match event {
					Ok(notify) => {
						if let Some(url) = notify.browse_to_url
							&& *method == Method::LoginLink
						{
							let _ = links.send(Message::TailscaleLoginLink { url: Secret::new(url) });
						}
						if let Some(error) = notify.err_message {
							return Outcome::Failed(scrub(&error, key.as_ref()));
						}
					}
					Err(broadcast::error::RecvError::Lagged(missed)) => {
						tracing::warn!(missed, "the notification bus ran ahead of a sign-in");
					}
					Err(broadcast::error::RecvError::Closed) => {
						return Outcome::Failed("Tailscale is not being watched".to_owned());
					}
				},
				() = &mut deadline => {
					return Outcome::Failed(match method {
						Method::AuthKey(_) => "Tailscale did not sign in in time".to_owned(),
						Method::LoginLink => "the login link was not approved in time".to_owned(),
					});
				}
			}
		}
		self.hold_to_the_accepted_tailnet(daemon, &mut observed)
			.await
	}

	/// Sign out of a tailnet a sign-in joined that the device does not accept.
	async fn hold_to_the_accepted_tailnet(
		&self,
		daemon: &Daemon,
		observed: &mut watch::Receiver<Observed>,
	) -> Outcome {
		let Some(accepted) = &self.options.tailnet else {
			return Outcome::Done;
		};
		let joined = match &*observed.borrow() {
			Observed::Standing(snapshot) => snapshot.tailnet.clone(),
			Observed::Broken(_) => None,
		};
		// Where the tailnet is not yet known, as it may not be while an approval is awaited, nothing
		// says it is another one.
		if joined.as_deref().is_none_or(|joined| joined == accepted) {
			return Outcome::Done;
		}
		tracing::warn!(%accepted, "signed in to a tailnet the device does not accept; signing out");
		if let Err(err) = daemon.api.logout().await {
			tracing::error!(%err, "cannot sign out of a tailnet the device does not accept");
		} else if !reach(observed, TRANSITION, &|state| state == State::NeedsLogin).await {
			tracing::error!("did not sign out of a tailnet the device does not accept in time");
		}
		Outcome::Failed(format!("this device accepts only the {accepted} tailnet"))
	}

	async fn netcheck(&self, daemon: Option<&Daemon>) -> Outcome {
		if let Some(daemon) = daemon {
			self.names.refresh(daemon.api.as_ref()).await;
		}
		match self
			.netcheck
			.run()
			.await
			.and_then(|text| netcheck::result(&text, &self.names))
		{
			Ok(result) => Outcome::Result(result),
			Err(reason) => Outcome::Failed(reason),
		}
	}

	async fn ping(&self, daemon: &Daemon, peer: Option<&str>) -> Outcome {
		let peer = peer.unwrap_or(&self.options.ping_default);
		match ping::run(daemon.api.as_ref(), peer, &self.names).await {
			Ok(result) => Outcome::Result(result),
			Err(reason) => Outcome::Failed(reason),
		}
	}
}

/// Whether Tailscale reaches a state `target` accepts within `deadline`.
async fn reach(
	observed: &mut watch::Receiver<Observed>,
	deadline: Duration,
	target: &impl Fn(State) -> bool,
) -> bool {
	let reached = tokio::time::timeout(
		deadline,
		observed.wait_for(|observed| observed.state().is_some_and(target)),
	)
	.await;
	matches!(reached, Ok(Ok(_)))
}

/// `text` with `key` taken out of it, where it echoes one.
fn scrub(text: &str, key: Option<&Secret>) -> String {
	match key {
		Some(key) if !key.expose().is_empty() => text.replace(key.expose(), "[key]"),
		_ => text.to_owned(),
	}
}
