//! Provisioning mode: a device booted from a provisioning medium, set up by an operator at the device
//! through a client or the console.
//!
//! Behaviour is specified in `.workhorse/specs/provisioning/`: PRV for the mode, RUN for runs, PRS
//! for the stream a client drives them over, PCON for the console, PLED for the status LED, and PHW and
//! PEEP for the steps `hardware` and `eeprom`.
//!
//! One [`Provisioner`] owns the run and serves every front end, so a client and the console are
//! looking at the same one.

/// Where the daemon listens for the console, in the runtime directory systemd makes for it.
pub const SOCKET: &str = "/run/bliti/provision.sock";

pub mod button;
#[cfg(unix)]
pub mod console;
pub mod context;
#[cfg(target_os = "linux")]
pub mod daemon;
#[cfg(test)]
mod fake;
pub mod led;
mod model;
pub mod name;
pub mod run;
pub mod session;
#[cfg(unix)]
pub mod socket;
pub mod step;
pub mod steps;
pub mod store;
pub mod stream;
pub mod window;

pub use run::{Act, CannotAct, Provisioner, Snapshot};
