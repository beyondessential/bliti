//! The acts a client can ask of Tailscale, and which are listed while (TSC, "The acts").

use std::{
	fmt,
	sync::{Arc, Mutex, PoisonError},
};

use bliti_core::channel::{
	config::{Segment, path},
	messages::Secret,
};

use super::state::{Observed, State};

/// Something a client can ask a device to do to Tailscale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
	/// Join a tailnet.
	SignIn,
	/// Connect to the tailnet signed in to.
	TurnOn,
	/// Disconnect, staying signed in.
	TurnOff,
	/// Remove the sign-in.
	LogOut,
	/// Test the network beneath Tailscale.
	Netcheck,
	/// Test the path to another machine.
	Ping,
}

impl Act {
	const ALL: [Self; 6] = [
		Self::SignIn,
		Self::TurnOn,
		Self::TurnOff,
		Self::LogOut,
		Self::Netcheck,
		Self::Ping,
	];

	/// The act's wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::SignIn => "sign-in",
			Self::TurnOn => "turn-on",
			Self::TurnOff => "turn-off",
			Self::LogOut => "log-out",
			Self::Netcheck => "netcheck",
			Self::Ping => "ping",
		}
	}

	/// The act a wire name names, where it names one this build knows.
	pub fn from_name(name: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|act| act.name() == name)
	}

	fn doing(self) -> &'static str {
		match self {
			Self::SignIn => "signing in",
			Self::TurnOn => "turning Tailscale on",
			Self::TurnOff => "turning Tailscale off",
			Self::LogOut => "logging out of Tailscale",
			Self::Netcheck => "running a network check",
			Self::Ping => "pinging",
		}
	}
}

impl fmt::Display for Act {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.name())
	}
}

/// The acts listed while Tailscale is as `observed`, with `None` where the device runs no Tailscale,
/// and `netcheck` where the device can run the test (TSC, the table).
pub fn listed(observed: Option<&Observed>, netcheck: bool) -> Vec<Act> {
	let state = observed.and_then(Observed::state);
	let listed_while = |states: &[State]| state.is_some_and(|state| states.contains(&state));
	Act::ALL
		.into_iter()
		.filter(|act| match act {
			Act::SignIn => listed_while(&[State::NeedsLogin, State::Expired]),
			Act::TurnOn => listed_while(&[State::Stopped]),
			Act::TurnOff => listed_while(&[State::Running, State::Starting]),
			Act::LogOut => listed_while(&[
				State::Running,
				State::Starting,
				State::Stopped,
				State::NeedsApproval,
			]),
			Act::Netcheck => netcheck,
			Act::Ping => listed_while(&[State::Running]),
		})
		.collect()
}

/// How a `sign-in` is done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
	/// With an auth key.
	AuthKey(Secret),
	/// By a login link a person opens.
	LoginLink,
}

/// An act asked for, with the parameters it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
	/// Join a tailnet.
	SignIn {
		/// How.
		method: Method,
		/// The name to register under, where not the device's own hostname.
		name: Option<String>,
	},
	/// Connect.
	TurnOn,
	/// Disconnect.
	TurnOff,
	/// Remove the sign-in.
	LogOut,
	/// Test the network.
	Netcheck,
	/// Test the path to `peer`, or to the device's default where absent.
	Ping {
		/// The machine to test.
		peer: Option<String>,
	},
}

impl Request {
	/// The act this asks for.
	pub fn act(&self) -> Act {
		match self {
			Self::SignIn { .. } => Act::SignIn,
			Self::TurnOn => Act::TurnOn,
			Self::TurnOff => Act::TurnOff,
			Self::LogOut => Act::LogOut,
			Self::Netcheck => Act::Netcheck,
			Self::Ping { .. } => Act::Ping,
		}
	}

	/// Whether the act waits on something outside the device, and so ends with the stream that asked
	/// for it (TSC, "The exchange").
	pub fn ends_with_its_stream(&self) -> bool {
		matches!(
			self,
			Self::Netcheck
				| Self::Ping { .. }
				| Self::SignIn {
					method: Method::LoginLink,
					..
				}
		)
	}

	/// Read the parameters an act of `act` carries, or say which part is at fault (TSC).
	pub(super) fn parse(
		act: Act,
		method: Option<&str>,
		key: Option<Secret>,
		name: Option<String>,
		peer: Option<String>,
	) -> Result<Self, CannotAct> {
		match act {
			Act::SignIn => {
				let method = match method {
					Some("auth-key") => Method::AuthKey(key.ok_or_else(|| {
						CannotAct::at("key", "signing in by auth key needs the key")
					})?),
					Some("login-link") => Method::LoginLink,
					Some(other) => {
						return Err(CannotAct::at(
							"method",
							format!("this device cannot sign in by {other}"),
						));
					}
					None => {
						return Err(CannotAct::at("method", "signing in needs a method"));
					}
				};
				Ok(Self::SignIn { method, name })
			}
			Act::TurnOn => Ok(Self::TurnOn),
			Act::TurnOff => Ok(Self::TurnOff),
			Act::LogOut => Ok(Self::LogOut),
			Act::Netcheck => Ok(Self::Netcheck),
			Act::Ping => Ok(Self::Ping { peer }),
		}
	}
}

/// Why an act asked for is not taken: the part of the message at fault, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CannotAct {
	/// A normalized path rooted at the message.
	pub at: String,
	/// Why.
	pub reason: String,
}

impl CannotAct {
	fn at(member: &'static str, reason: impl Into<String>) -> Self {
		Self {
			at: path(&[Segment::Name(member)]),
			reason: reason.into(),
		}
	}

	pub(super) fn busy(under_way: Act) -> Self {
		Self {
			at: path(&[]),
			reason: format!("the device is already {}", under_way.doing()),
		}
	}

	pub(super) fn not_listed(act: &str) -> Self {
		Self::at("act", format!("this device cannot {act} now"))
	}
}

/// The one act under way, held until it is carried out, has failed or has been ended.
#[derive(Debug)]
pub struct Slot {
	held: Arc<Mutex<Option<Act>>>,
}

impl Slot {
	/// Take the slot for `act`, or return the act holding it.
	pub(super) fn take(held: &Arc<Mutex<Option<Act>>>, act: Act) -> Result<Self, Act> {
		let mut under_way = held.lock().unwrap_or_else(PoisonError::into_inner);
		if let Some(already) = *under_way {
			return Err(already);
		}
		*under_way = Some(act);
		Ok(Self { held: held.clone() })
	}
}

impl Drop for Slot {
	fn drop(&mut self) {
		*self.held.lock().unwrap_or_else(PoisonError::into_inner) = None;
	}
}

#[cfg(test)]
mod tests;
