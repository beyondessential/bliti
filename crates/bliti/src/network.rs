//! Network configuration: the device half of NET and its siblings.
//!
//! [`session`] serves the configuration session of CFG. [`render`] turns a configuration document
//! into the files iwd, hostapd and systemd-networkd read, which bliti owns outright.

mod render;
pub mod session;
