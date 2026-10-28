//! The daemon's local socket, through which the console reaches the one run clients also drive
//! (PCON).

use std::{io, path::Path};

use super::Provisioner;
use crate::session::AbortOnDrop;

/// Listen on `path` for the console, serving each connection the provisioning stream on
/// `provisioner`, until the handle returned is dropped.
pub fn listen(_path: &Path, _provisioner: Provisioner) -> io::Result<AbortOnDrop> {
	Err(io::Error::other("not yet"))
}
