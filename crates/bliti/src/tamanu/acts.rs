//! The acts a client can ask of Tamanu, and which are listed while (TMU, "The acts").

use std::fmt;

use bliti_core::channel::config::{Segment, path};

use super::state::State;

/// Something a client can ask a device to do to Tamanu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
	/// Bring Tamanu up.
	Start,
	/// Restart Tamanu.
	Restart,
}

impl Act {
	const ALL: [Self; 2] = [Self::Start, Self::Restart];

	/// The act's wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::Start => "start",
			Self::Restart => "restart",
		}
	}

	/// The act a wire name names, where it names one this build knows.
	pub fn from_name(name: &str) -> Option<Self> {
		Self::ALL.into_iter().find(|act| act.name() == name)
	}

	/// The state reported from the act being accepted until it ends (TMU, "Carrying it out").
	pub fn under_way(self) -> State {
		match self {
			Self::Start => State::Starting,
			Self::Restart => State::Restarting,
		}
	}

	fn doing(self) -> &'static str {
		match self {
			Self::Start => "starting Tamanu",
			Self::Restart => "restarting Tamanu",
		}
	}
}

impl fmt::Display for Act {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(self.name())
	}
}

/// The acts listed while `tamanu-state` is `state`, with none where it is not reported (TMU, the table).
pub fn listed(state: Option<State>) -> Vec<Act> {
	match state {
		Some(State::Stopped) => vec![Act::Start],
		Some(State::Running | State::Degraded) => vec![Act::Restart],
		_ => Vec::new(),
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
	pub(super) fn busy(under_way: Act) -> Self {
		Self {
			at: path(&[]),
			reason: format!("the device is already {}", under_way.doing()),
		}
	}

	pub(super) fn not_listed(act: &str) -> Self {
		Self {
			at: path(&[Segment::Name("act")]),
			reason: format!("this device cannot {act} Tamanu now"),
		}
	}
}
