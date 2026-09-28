//! Carrying acts out through systemd, over the system bus.

use std::time::Duration;

use anyhow::Context as _;
use dbus::{
	Path as ObjectPath,
	blocking::{Connection, stdintf::org_freedesktop_dbus::Properties},
};

use super::{Act, Power};

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";
const SYSTEMD_MANAGER: &str = "org.freedesktop.systemd1.Manager";
const SYSTEMD_UNIT: &str = "org.freedesktop.systemd1.Unit";

/// How long a D-Bus call waits for its reply.
const CALL: Duration = Duration::from_secs(25);

/// systemd, carrying out what this process may ask of it.
pub struct Systemd {
	/// The service unit this daemon runs as, where it runs as one.
	unit: Option<String>,
	root: bool,
}

impl Systemd {
	/// Find what this process may ask systemd to do: restart it where it runs as a service, and reboot
	/// or power off where it runs as root. Blocking.
	pub fn probe() -> anyhow::Result<Self> {
		let root = rustix::process::geteuid().is_root();
		let bus = Connection::new_system().context("cannot connect to the system bus")?;
		let unit = own_unit(&bus)
			.inspect_err(|err| tracing::debug!(err = format!("{err:#}"), "not a systemd service"))
			.ok()
			.filter(|unit| unit.ends_with(".service"));
		Ok(Self { unit, root })
	}

	/// The acts this process can carry out (CTL, "The acts").
	pub fn acts(&self) -> Vec<Act> {
		let mut acts = Vec::new();
		// Restarting its own unit is something only a service can ask for; one started by hand has
		// nothing that would start it again.
		if self.unit.is_some() && self.root {
			acts.push(Act::Restart);
		}
		if self.root {
			acts.extend([Act::Reboot, Act::PowerOff]);
		}
		acts
	}
}

impl Power for Systemd {
	fn carry_out(&self, act: Act) -> anyhow::Result<()> {
		let bus = Connection::new_system().context("cannot connect to the system bus")?;
		let manager = bus.with_proxy(SYSTEMD, SYSTEMD_PATH, CALL);
		// Queued rather than waited for: every one of these ends this process, and a restart of its own
		// unit waited on would be killed part way through the wait.
		let (unit, mode) = match act {
			Act::Restart => (
				self.unit
					.clone()
					.context("this daemon does not run as a systemd service")?,
				"replace",
			),
			// As `systemctl reboot` and `systemctl poweroff` start them: every service is stopped in
			// turn, and nothing queued later can cancel it.
			Act::Reboot => ("reboot.target".to_owned(), "replace-irreversibly"),
			Act::PowerOff => ("poweroff.target".to_owned(), "replace-irreversibly"),
		};
		let method = match act {
			Act::Restart => "RestartUnit",
			Act::Reboot | Act::PowerOff => "StartUnit",
		};
		let (_job,): (ObjectPath<'static>,) = manager
			.method_call(SYSTEMD_MANAGER, method, (unit.as_str(), mode))
			.with_context(|| format!("systemd refused {method} {unit}"))?;
		Ok(())
	}
}

/// The unit this process runs in.
fn own_unit(bus: &Connection) -> anyhow::Result<String> {
	let (path,): (ObjectPath<'static>,) = bus
		.with_proxy(SYSTEMD, SYSTEMD_PATH, CALL)
		.method_call(SYSTEMD_MANAGER, "GetUnitByPID", (std::process::id(),))
		.context("systemd does not know this process")?;
	bus.with_proxy(SYSTEMD, path, CALL)
		.get::<String>(SYSTEMD_UNIT, "Id")
		.context("cannot read this process's unit")
}
