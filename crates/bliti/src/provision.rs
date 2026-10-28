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
pub mod console;
pub mod context;
pub mod daemon;
#[cfg(test)]
mod fake;
pub mod led;
mod model;
pub mod name;
pub mod run;
pub mod session;
pub mod socket;
pub mod step;
pub mod store;
pub mod stream;
pub mod window;

pub use context::{Context, Notice};
pub use run::{Act, CannotAct, Provisioner, Snapshot};
pub use step::{Done, Step};
