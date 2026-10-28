//! Provisioning mode: a device booted from a provisioning medium, set up by an operator at the device
//! through a client or the console.
//!
//! Behaviour is specified in `.workhorse/specs/provisioning/`: PRV for the mode, RUN for runs, PRS
//! for the stream a client drives them over, PCON for the console and PLED for the status LED.
//!
//! One [`Provisioner`] owns the run and serves every front end, so a client and the console are
//! looking at the same one.

/// Where the daemon listens for the console, in the runtime directory systemd makes for it.
pub const SOCKET: &str = "/run/bliti/provision.sock";

pub mod button;
#[cfg(unix)]
pub mod console;
#[cfg_attr(
	not(test),
	expect(
		dead_code,
		reason = "steps use this, and the registry is empty until a card adds one"
	)
)]
pub mod context;
#[cfg(target_os = "linux")]
pub mod daemon;
#[cfg(test)]
mod fake;
pub mod led;
#[cfg_attr(
	not(test),
	expect(
		dead_code,
		reason = "the context of a step uses the prompts, and no step exists yet"
	)
)]
mod model;
pub mod name;
pub mod run;
pub mod session;
#[cfg(unix)]
pub mod socket;
pub mod step;
pub mod store;
pub mod stream;
pub mod window;

pub use run::{Act, CannotAct, Provisioner, Snapshot};
