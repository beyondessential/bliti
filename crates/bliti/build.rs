//! Tags the version with `BLITI_BUILD`, as build metadata, when a development build names itself, so
//! a device running one reports which build it is. Unset, the version is the package's alone.

use std::env;

fn main() {
	println!("cargo:rerun-if-env-changed=BLITI_BUILD");
	let version = env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION");
	let version = match env::var("BLITI_BUILD") {
		Ok(build) if !build.is_empty() => format!("{version}+{build}"),
		_ => version,
	};
	println!("cargo:rustc-env=BLITI_VERSION={version}");
}
