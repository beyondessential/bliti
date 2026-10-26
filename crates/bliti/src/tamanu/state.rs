//! How the device stands in Tamanu, and the poller that keeps it current (NFO, "Tamanu").

use std::{sync::Arc, time::Duration};

use tokio::{
	sync::{Mutex, watch},
	time::Instant,
};

use super::{
	acts::Act,
	run::{Output, Runner},
	status::Report,
};

/// How long to wait between looks at Tamanu while somebody may be watching.
const POLL: Duration = Duration::from_secs(5);

/// How long a poll waits for a session, when it has stopped for want of one.
const RESUME_CHECK: Duration = Duration::from_secs(1);

/// How the device stands, by the wire names of `tamanu-state` (NFO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
	/// Every required service runs as configured.
	Running,
	/// Some required service runs, and Tamanu is not `running`.
	Degraded,
	/// No required service runs.
	Stopped,
	/// No installation is found.
	Absent,
	/// A start is under way.
	Starting,
	/// A restart is under way.
	Restarting,
}

impl State {
	/// The wire name.
	pub fn name(self) -> &'static str {
		match self {
			Self::Running => "running",
			Self::Degraded => "degraded",
			Self::Stopped => "stopped",
			Self::Absent => "absent",
			Self::Starting => "starting",
			Self::Restarting => "restarting",
		}
	}
}

/// What the device knows of one service's version.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServiceVersion {
	/// Whether Tamanu reports the service at all.
	pub reported: bool,
	/// Whether any instance runs.
	pub up: bool,
	/// The version running, the highest where instances differ.
	pub running: Option<String>,
	/// The version the service is configured to run.
	pub configured: Option<String>,
	/// What is wrong with the version, where something is.
	pub warning: Option<String>,
}

/// How an installation of Tamanu stands, once it has been read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
	/// `running`, `degraded` or `stopped`.
	pub state: State,
	/// What is short, where it is.
	pub reason: Option<String>,
	/// The web application's version.
	pub web: ServiceVersion,
	/// The facility server's version.
	pub facility_server: ServiceVersion,
}

/// What the device observes of Tamanu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
	/// Tamanu has not been looked at yet, or not since the device last stopped looking for want of a
	/// session. Nothing is known, which is not the same as being unable to tell.
	Pending,
	/// No installation is found.
	Absent,
	/// The status was read.
	Standing(Snapshot),
	/// An installation is found, or the program that finds one cannot be run, and how it stands cannot
	/// be told; says why.
	Broken(String),
}

/// What the device reports of Tamanu: what it observes, with an act under way laid over it (TMU).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
	/// What the device last observed.
	pub observed: Observed,
	/// The act under way, whatever the observation says.
	pub acting: Option<Act>,
}

impl View {
	/// `tamanu-state`, or none where the device has not looked or could not tell and no act is under way.
	pub fn state(&self) -> Option<State> {
		if let Some(act) = self.acting {
			return Some(act.under_way());
		}
		match &self.observed {
			Observed::Absent => Some(State::Absent),
			Observed::Standing(snapshot) => Some(snapshot.state),
			Observed::Pending | Observed::Broken(_) => None,
		}
	}
}

impl Observed {
	/// Whether Tamanu runs as configured.
	pub fn is_running(&self) -> bool {
		matches!(self, Self::Standing(snapshot) if snapshot.state == State::Running)
	}

	/// What is short of Tamanu running, in the device's words.
	pub fn shortfall(&self) -> String {
		match self {
			Self::Pending => "Tamanu has not been looked at".to_owned(),
			Self::Absent => "no installation of Tamanu is found".to_owned(),
			Self::Broken(reason) => reason.clone(),
			Self::Standing(snapshot) => snapshot
				.reason
				.clone()
				.unwrap_or_else(|| "Tamanu is running".to_owned()),
		}
	}
}

/// Look at Tamanu once.
///
/// Reads the status first, as an installation that is there answers it. Where it does not, an
/// installation not found at all is `absent`, and anything else that stops the status being read is
/// `broken`.
pub async fn observe(runner: &dyn Runner) -> Observed {
	let Output { stdout, failure } = match runner.status().await {
		Ok(output) => output,
		Err(reason) => return Observed::Broken(reason),
	};
	let unreadable = match serde_json::from_str::<Report>(&stdout) {
		Ok(report) => return Observed::from_report(&report),
		Err(err) => failure.unwrap_or_else(|| format!("the status was unreadable: {err}")),
	};
	match runner.find().await {
		Ok(false) => Observed::Absent,
		Ok(true) => Observed::Broken(format!("Tamanu is installed, but {unreadable}")),
		Err(reason) => Observed::Broken(reason),
	}
}

/// Held while Tamanu is looked at and the sight published, so that a look which began before another
/// is never published after it and does not put an older state over a newer one.
pub type Looking = Arc<Mutex<()>>;

/// Publish what was observed, and say so where it changed.
pub fn publish(view: &watch::Sender<View>, now: Observed) {
	let changed = view.send_if_modified(|held| {
		if held.observed == now {
			return false;
		}
		held.observed = now.clone();
		true
	});
	if changed {
		tracing::info!(state = ?now, "Tamanu changed");
	}
}

/// Keep `view` current, looking while `wanted` says somebody is connected and for `idle_stop` after
/// (NFO, "Sampling"), so a device nobody is talking to does not inspect Tamanu's containers.
///
/// Where it has stopped looking, what it last saw is withdrawn, as it would be out of date by the time a
/// session next opens. Runs until dropped.
pub async fn poll(
	runner: Arc<dyn Runner>,
	view: Arc<watch::Sender<View>>,
	looking: Looking,
	wanted: Arc<dyn Fn() -> bool + Send + Sync>,
	idle_stop: Duration,
) {
	let mut last_wanted = Instant::now();
	loop {
		if wanted() {
			last_wanted = Instant::now();
		} else if last_wanted.elapsed() >= idle_stop {
			tracing::debug!("no session for a while; Tamanu is not looked at until one opens");
			publish(&view, Observed::Pending);
			while !wanted() {
				tokio::time::sleep(RESUME_CHECK).await;
			}
			last_wanted = Instant::now();
		}
		{
			let _looking = looking.lock().await;
			publish(&view, observe(&*runner).await);
		}
		tokio::time::sleep(POLL).await;
	}
}

#[cfg(test)]
mod tests;
