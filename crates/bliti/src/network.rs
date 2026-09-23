//! Network configuration: the device half of NET and its siblings.
//!
//! [`session`] serves the configuration session of CFG. [`render`] turns a configuration document
//! into the files iwd, hostapd and systemd-networkd read, which bliti owns outright. [`select`]
//! decides which candidates are up (LINK), [`apply`] puts rendered files in place, and [`probe`]
//! asks each radio what it can do.

mod apply;
pub mod probe;
mod render;
mod select;
pub mod session;
pub mod wired;
