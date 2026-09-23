//! The radio settings nl80211 carries, set by running `iw`.
//!
//! Speaking nl80211 over netlink would spare the dependency on the `iw` package; until a crate for
//! it is chosen, these three commands are all of it, kept here so that replacing them touches
//! nothing else.

use std::{path::Path, process::Command};

use anyhow::{Context as _, bail};

/// Put the radio under `domain`, `00` being the world domain.
pub(super) fn set_regulatory_domain(domain: &str) -> anyhow::Result<()> {
	run(&["reg", "set", domain])
}

/// Create the access point interface `interface` on the radio `radio` is on, where it does not
/// exist.
pub(super) fn create_access_point(radio: &str, interface: &str) -> anyhow::Result<()> {
	if exists(interface) {
		return Ok(());
	}
	run(&["dev", radio, "interface", "add", interface, "type", "__ap"])
}

/// Delete the interface `interface`, where it exists.
pub(super) fn delete_access_point(interface: &str) -> anyhow::Result<()> {
	if !exists(interface) {
		return Ok(());
	}
	run(&["dev", interface, "del"])
}

fn exists(interface: &str) -> bool {
	Path::new("/sys/class/net").join(interface).exists()
}

fn run(args: &[&str]) -> anyhow::Result<()> {
	let output = Command::new("iw")
		.args(args)
		.output()
		.context("cannot run iw")?;
	if !output.status.success() {
		bail!(
			"iw {} failed ({}): {}",
			args.join(" "),
			output.status,
			String::from_utf8_lossy(&output.stderr).trim()
		);
	}
	Ok(())
}
