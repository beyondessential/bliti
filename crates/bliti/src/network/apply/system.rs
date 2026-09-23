//! The device's [`System`]: systemd and networkd over D-Bus, and the radio through [`iw`].

use std::{
	collections::HashMap,
	sync::{Arc, Mutex, PoisonError},
	time::{Duration, Instant},
};

use anyhow::{Context as _, bail};
use dbus::{Path as ObjectPath, blocking::Connection, message::MatchRule};

use super::{Hostapd, System, iw};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";
const SYSTEMD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const NETWORKD: &str = "org.freedesktop.network1";
const NETWORKD_PATH: &str = "/org/freedesktop/network1";
const NETWORKD_MANAGER: &str = "org.freedesktop.network1.Manager";

/// iwd's unit, as the iwd package ships it.
const IWD: &str = "iwd.service";

/// The unit running hostapd on bliti's configuration, from `services/`.
const HOSTAPD: &str = "bliti-hostapd.service";

/// The unit running systemd-resolved, which rereads its DNS delegates on reload.
const RESOLVED: &str = "systemd-resolved.service";

/// How long a D-Bus call waits for its reply.
const CALL: Duration = Duration::from_secs(25);

/// How long a unit's job may take, beyond systemd's own default start timeout of 90 seconds.
const JOB: Duration = Duration::from_secs(120);

/// The device's stack, over the system bus.
pub struct Linux {
	bus: Connection,
}

#[expect(dead_code, reason = "wired in by the backend that applies selections")]
impl Linux {
	/// Connect to the system bus.
	pub fn new() -> anyhow::Result<Self> {
		let bus = Connection::new_system().context("cannot connect to the system bus")?;
		// systemd sends job signals only once some client has subscribed.
		bus.with_proxy(SYSTEMD, SYSTEMD_PATH, CALL)
			.method_call::<(), _, _, _>(SYSTEMD_MANAGER, "Subscribe", ())
			.context("cannot subscribe to systemd")?;
		Ok(Self { bus })
	}
}

impl Linux {
	/// Call `method` on `unit` and wait for the job it queues to finish.
	fn unit(&self, method: &str, unit: &str) -> anyhow::Result<()> {
		let finished: Arc<Mutex<HashMap<ObjectPath<'static>, String>>> = Arc::default();
		let sink = Arc::clone(&finished);
		// Matched before the call, so a job that finishes before its reply is read is not missed.
		let token = self.bus.add_match(
			MatchRule::new_signal(SYSTEMD_MANAGER, "JobRemoved")
				.with_path(SYSTEMD_PATH)
				.static_clone(),
			move |(_, job, _, result): (u32, ObjectPath<'static>, String, String), _, _| {
				sink.lock()
					.unwrap_or_else(PoisonError::into_inner)
					.insert(job, result);
				true
			},
		)?;
		let waited = self.wait(method, unit, &finished);
		let _ = self.bus.remove_match(token);
		waited
	}

	fn wait(
		&self,
		method: &str,
		unit: &str,
		finished: &Mutex<HashMap<ObjectPath<'static>, String>>,
	) -> anyhow::Result<()> {
		let (job,): (ObjectPath<'static>,) = self
			.bus
			.with_proxy(SYSTEMD, SYSTEMD_PATH, CALL)
			.method_call(SYSTEMD_MANAGER, method, (unit, "replace"))
			.with_context(|| format!("systemd refused {method} {unit}"))?;
		let deadline = Instant::now() + JOB;
		loop {
			let result = finished
				.lock()
				.unwrap_or_else(PoisonError::into_inner)
				.remove(&job);
			match result.as_deref() {
				Some("done") => return Ok(()),
				Some(result) => bail!("{method} {unit} ended {result:?}; see its journal"),
				None => {}
			}
			let left = deadline.saturating_duration_since(Instant::now());
			if left.is_zero() {
				bail!("{method} {unit} did not finish within {JOB:?}");
			}
			self.bus.process(left)?;
		}
	}
}

impl System for Linux {
	fn set_regulatory_domain(&mut self, domain: &str) -> anyhow::Result<()> {
		iw::set_regulatory_domain(domain)
	}

	fn create_access_point(&mut self, radio: &str, interface: &str) -> anyhow::Result<()> {
		iw::create_access_point(radio, interface)
	}

	fn delete_access_point(&mut self, interface: &str) -> anyhow::Result<()> {
		iw::delete_access_point(interface)
	}

	fn hostapd(&mut self, action: Hostapd) -> anyhow::Result<()> {
		let method = match action {
			Hostapd::Start => "StartUnit",
			Hostapd::Restart => "RestartUnit",
			Hostapd::Stop => "StopUnit",
		};
		self.unit(method, HOSTAPD)
	}

	fn restart_iwd(&mut self) -> anyhow::Result<()> {
		self.unit("RestartUnit", IWD)
	}

	fn reload_networkd(&mut self) -> anyhow::Result<()> {
		self.bus
			.with_proxy(NETWORKD, NETWORKD_PATH, CALL)
			.method_call::<(), _, _, _>(NETWORKD_MANAGER, "Reload", ())
			.context("networkd refused to reload")
	}

	fn reload_resolved(&mut self) -> anyhow::Result<()> {
		self.unit("ReloadUnit", RESOLVED)
	}
}
