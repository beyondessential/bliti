//! How the device stands in Tamanu (NFO, "Tamanu").

use bliti_core::channel::readings::{Entry, kind};

use crate::tamanu::state::{Observed, ServiceVersion, State, View};

const NAMES: [&str; 3] = [
	"tamanu-state",
	"tamanu-web-version",
	"tamanu-facility-server-version",
];

/// The three Tamanu facts.
pub fn entries(at: u64, view: &View) -> Vec<Entry> {
	let (web, facility_server) = match &view.observed {
		Observed::Standing(snapshot) => (&snapshot.web, &snapshot.facility_server),
		Observed::Absent | Observed::Broken(_) => {
			(&ServiceVersion::default(), &ServiceVersion::default())
		}
	};
	let absent = matches!(view.observed, Observed::Absent);
	let broken = |name| match &view.observed {
		Observed::Broken(reason) => Some(Entry::broken(at, name, kind::TEXT, reason.clone())),
		_ => None,
	};
	vec![
		match view.state() {
			Some(state) => state_entry(at, view, state),
			None => broken(NAMES[0]).expect("a state is missing only where observing is broken"),
		},
		broken(NAMES[1]).unwrap_or_else(|| version(at, NAMES[1], web, absent)),
		broken(NAMES[2]).unwrap_or_else(|| version(at, NAMES[2], facility_server, absent)),
	]
}

fn state_entry(at: u64, view: &View, state: State) -> Entry {
	let entry = Entry::text(at, NAMES[0], state.name());
	match (&view.observed, state) {
		(_, State::Running) => entry,
		(Observed::Standing(snapshot), State::Degraded) => entry.warning(
			snapshot
				.reason
				.clone()
				.unwrap_or_else(|| "Tamanu is not as configured".to_owned()),
		),
		(_, State::Degraded) => entry.warning("Tamanu is not as configured"),
		(_, State::Stopped) => entry.failed("Tamanu is stopped"),
		(_, State::Absent) => entry.failed("Tamanu is not installed"),
		(_, State::Starting) => entry.warning("Tamanu is starting"),
		(_, State::Restarting) => entry.warning("Tamanu is restarting"),
	}
}

fn version(at: u64, name: &str, service: &ServiceVersion, absent: bool) -> Entry {
	if absent {
		return Entry::skipped(at, name, kind::TEXT, "Tamanu is not installed");
	}
	match (&service.running, &service.configured) {
		(Some(running), _) => {
			let entry = Entry::text(at, name, running.clone());
			match &service.warning {
				Some(warning) => entry.warning(warning.clone()),
				None => entry,
			}
		}
		(None, Some(configured)) if service.up => Entry::text(at, name, configured.clone())
			.warning("the version running could not be read"),
		(None, Some(configured)) => {
			Entry::text(at, name, configured.clone()).failed("the service is not running")
		}
		(None, None) => Entry::skipped(at, name, kind::TEXT, "no version is known for the service"),
	}
}

#[cfg(test)]
mod tests;
