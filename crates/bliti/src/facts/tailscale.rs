//! How the device stands in Tailscale (NFO, "Tailscale").

use bliti_core::channel::readings::{Entry, kind};
use jiff::{SignedDuration, Timestamp};

use crate::tailscale::state::{Observed, Snapshot, State};

/// How far ahead of a sign-in lapsing a device warns.
const WARN_WITHIN: SignedDuration = SignedDuration::from_hours(7 * 24);

/// The four Tailscale facts, or none where the device runs no Tailscale.
pub fn entries(at: u64, observed: Option<&Observed>, now: Timestamp) -> Vec<Entry> {
	match observed {
		None => Vec::new(),
		Some(Observed::Broken(reason)) => [
			("tailscale-state", kind::TEXT),
			("tailscale-name", kind::TEXT),
			("tailnet", kind::TEXT),
			("tailscale-key-expiry", kind::DATETIME),
		]
		.into_iter()
		.map(|(name, kind)| Entry::broken(at, name, kind, reason.clone()))
		.collect(),
		Some(Observed::Standing(snapshot)) => vec![
			state(at, snapshot),
			held(at, "tailscale-name", snapshot.name.as_deref()),
			held(at, "tailnet", snapshot.tailnet.as_deref()),
			key_expiry(at, snapshot, now),
		],
	}
}

fn state(at: u64, snapshot: &Snapshot) -> Entry {
	let entry = Entry::text(at, "tailscale-state", snapshot.state.name());
	match snapshot.state {
		State::Running => match &snapshot.warning {
			Some(warning) => entry.warning(warning.clone()),
			None => entry,
		},
		State::Starting => entry.warning("still connecting"),
		State::Stopped => entry.failed("the device is turned off"),
		State::NeedsLogin => entry.failed("the device is not signed in"),
		State::Expired => entry.failed("the device's sign-in has lapsed"),
		State::NeedsApproval => entry.failed("an administrator has yet to approve the device"),
	}
}

fn held(at: u64, name: &str, value: Option<&str>) -> Entry {
	match value {
		Some(value) => Entry::text(at, name, value),
		None => Entry::skipped(at, name, kind::TEXT, "the device is not signed in"),
	}
}

fn key_expiry(at: u64, snapshot: &Snapshot, now: Timestamp) -> Entry {
	let Some(expiry) = snapshot.key_expiry else {
		let reason = if snapshot.state == State::NeedsLogin {
			"the device holds no sign-in"
		} else {
			"the sign-in does not lapse"
		};
		return Entry::skipped(at, "tailscale-key-expiry", kind::DATETIME, reason);
	};
	let entry = Entry::datetime(at, "tailscale-key-expiry", expiry.to_string());
	let left = expiry.duration_since(now);
	if left <= SignedDuration::ZERO {
		entry.failed("the sign-in has lapsed")
	} else if left <= WARN_WITHIN {
		entry.warning(format!("the sign-in lapses in {}", describe(left)))
	} else {
		entry
	}
}

fn describe(left: SignedDuration) -> String {
	let days = left.as_hours() / 24;
	match days {
		0 => format!("{} hours", left.as_hours().max(1)),
		1 => "1 day".to_owned(),
		days => format!("{days} days"),
	}
}

#[cfg(test)]
mod tests;
