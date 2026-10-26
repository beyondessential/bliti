//! What `bestool tamanu status --json` reports, and how it reduces to how the device stands (NFO,
//! "Tamanu").

use std::cmp::Ordering;

use serde::Deserialize;

use super::state::{Observed, ServiceVersion, Snapshot, State};

/// The expectation that is Tamanu's web application, versioned by the frontend's configured version.
const WEB: &str = "tamanu-frontend";

/// The expectation that is Tamanu's facility server.
const FACILITY_SERVER: &str = "tamanu-facility-api";

/// The whole report.
#[derive(Debug, Deserialize)]
pub struct Report {
	/// Whether any expectation is short, or any instance runs a version other than the configured one.
	pub any_short: bool,
	/// Each service Tamanu is expected to run, or to not run.
	pub expectations: Vec<Expectation>,
}

/// One service, against what is expected of it.
#[derive(Debug, Deserialize)]
pub struct Expectation {
	/// The service.
	pub name: String,
	/// `up`, `down` or `unknown`.
	pub expected_state: String,
	/// How many instances run.
	#[serde(default)]
	pub running: usize,
	/// How many must.
	#[serde(default)]
	pub min_count: usize,
	/// `up`, `down`, `short`, `absent`, `forbidden` or `unknown`.
	pub status: String,
	/// The version the service is configured to run.
	#[serde(default)]
	pub expected_version: Option<String>,
	/// Each instance found.
	#[serde(default)]
	pub instances: Vec<Instance>,
}

/// One instance of a service.
#[derive(Debug, Deserialize)]
pub struct Instance {
	/// Whether it runs.
	#[serde(default)]
	pub running: bool,
	/// The version it runs, while it does.
	#[serde(default)]
	pub version_actual: Option<String>,
	/// `match`, `mismatch` or `unknown`.
	#[serde(default)]
	pub version_status: String,
}

impl Observed {
	/// How the device stands, from what the status says.
	pub fn from_report(report: &Report) -> Self {
		let required = || {
			report
				.expectations
				.iter()
				.filter(|expectation| expectation.expected_state == "up")
		};
		let state = if !report.any_short {
			State::Running
		} else if required().next().is_some() && required().all(|e| e.status == "down") {
			State::Stopped
		} else {
			State::Degraded
		};
		let reason = (state != State::Running).then(|| what_is_short(report));
		Self::Standing(Snapshot {
			state,
			reason,
			web: service_version(report, WEB),
			facility_server: service_version(report, FACILITY_SERVER),
		})
	}
}

/// What each expectation that is short says is wrong, in one sentence.
fn what_is_short(report: &Report) -> String {
	let mut shortfalls = Vec::new();
	for expectation in &report.expectations {
		let name = &expectation.name;
		match expectation.status.as_str() {
			"down" => shortfalls.push(format!("{name} is not running")),
			"short" => shortfalls.push(format!(
				"{name} has {} of {} instances running",
				expectation.running, expectation.min_count
			)),
			"forbidden" => shortfalls.push(format!("{name} is present and must not be")),
			_ => {}
		}
		if let Some(drift) = drift(expectation) {
			shortfalls.push(format!("{name} {drift}"));
		}
	}
	if shortfalls.is_empty() {
		"Tamanu is not as configured".to_owned()
	} else {
		shortfalls.join("; ")
	}
}

/// Say how an expectation's running instances differ from the version configured, or from each other.
fn drift(expectation: &Expectation) -> Option<String> {
	let running: Vec<&str> = running_versions(expectation);
	let mut distinct = running.clone();
	distinct.sort_by(|a, b| compare(a, b));
	distinct.dedup();
	if distinct.len() > 1 {
		return Some(format!(
			"runs different versions, {}",
			distinct.join(" and ")
		));
	}
	let mismatched = expectation
		.instances
		.iter()
		.any(|instance| instance.running && instance.version_status == "mismatch");
	if !mismatched {
		return None;
	}
	Some(match (running.first(), &expectation.expected_version) {
		(Some(running), Some(configured)) => {
			format!("runs {running} where {configured} is configured")
		}
		_ => "does not run the version it is configured to".to_owned(),
	})
}

fn running_versions(expectation: &Expectation) -> Vec<&str> {
	expectation
		.instances
		.iter()
		.filter(|instance| instance.running)
		.filter_map(|instance| instance.version_actual.as_deref())
		.collect()
}

fn service_version(report: &Report, name: &str) -> ServiceVersion {
	let Some(expectation) = report.expectations.iter().find(|e| e.name == name) else {
		return ServiceVersion::default();
	};
	let running = running_versions(expectation)
		.into_iter()
		.max_by(|a, b| compare(a, b))
		.map(ToOwned::to_owned);
	ServiceVersion {
		reported: true,
		up: expectation
			.instances
			.iter()
			.any(|instance| instance.running),
		running,
		configured: expectation.expected_version.clone(),
		warning: drift(expectation),
	}
}

/// Order two versions, as versions where both read as one and as text where they do not.
fn compare(a: &str, b: &str) -> Ordering {
	let parse = |version: &str| semver::Version::parse(version.trim_start_matches('v')).ok();
	match (parse(a), parse(b)) {
		(Some(a), Some(b)) => a.cmp(&b),
		_ => a.cmp(b),
	}
}

#[cfg(test)]
mod tests;
