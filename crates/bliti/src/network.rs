//! Network configuration: the device half of NET and its siblings.
//!
//! [`session`] serves the configuration session of CFG. [`render`] turns a configuration document
//! into the files iwd, hostapd and systemd-networkd read, which bliti owns outright. [`select`]
//! decides which candidates are up (LINK), and [`apply`] puts rendered files in place.

mod apply;
mod render;
mod select;
pub mod session;
