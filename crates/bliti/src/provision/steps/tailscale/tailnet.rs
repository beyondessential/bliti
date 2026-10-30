//! The one tailnet a system accepts, where it is configured with one (TSC, "Signing in").
//!
//! The Iti's own service takes its options from `BLITI_DAEMON_ARGS` in an environment file the
//! deployment installs, so the step reads them the way the service would, rather than keeping its
//! own idea of what they mean.

use clap::Parser as _;

use crate::{Cli, Command};

/// Where the environment file is, under the root filesystem.
pub const PATH: &str = "etc/bliti/daemon.env";

const VARIABLE: &str = "BLITI_DAEMON_ARGS";

/// The tailnet the daemon options in `env`, the text of an environment file, accept, if any.
pub fn accepted(env: &str) -> Result<Option<String>, String> {
	let Some(args) = arguments(env) else {
		return Ok(None);
	};
	let words = ["bliti", "daemon"]
		.into_iter()
		.chain(args.split_whitespace());
	match Cli::try_parse_from(words) {
		Ok(Cli {
			command: Command::Daemon { tailnet, .. },
			..
		}) => Ok(tailnet),
		Ok(_) => Err("the daemon options name another command".to_owned()),
		Err(err) => Err(format!(
			"the daemon options of the system are not readable: {}",
			err.kind()
		)),
	}
}

/// The last value `BLITI_DAEMON_ARGS` is given, as systemd reads an environment file: one
/// `NAME=value` a line, comments starting `#` or `;`, a value optionally in matching quotes.
fn arguments(env: &str) -> Option<String> {
	env.lines()
		.map(str::trim)
		.filter(|line| !line.starts_with(['#', ';']))
		.filter_map(|line| line.strip_prefix(VARIABLE)?.trim_start().strip_prefix('='))
		.map(|value| {
			let value = value.trim();
			['"', '\'']
				.into_iter()
				.find_map(|quote| value.strip_prefix(quote)?.strip_suffix(quote))
				.unwrap_or(value)
				.to_owned()
		})
		.next_back()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn no_option_accepts_any_tailnet() {
		assert_eq!(accepted(""), Ok(None));
		assert_eq!(accepted("OTHER=1\n"), Ok(None));
		assert_eq!(
			accepted("BLITI_DAEMON_ARGS=\"--ping-default x\"\n"),
			Ok(None)
		);
	}

	#[test]
	fn the_option_names_the_tailnet() {
		assert_eq!(
			accepted("BLITI_DAEMON_ARGS=\"--tailnet fleet.example --ping-default canopy\"\n"),
			Ok(Some("fleet.example".to_owned()))
		);
		assert_eq!(
			accepted("BLITI_DAEMON_ARGS=--tailnet=fleet.example"),
			Ok(Some("fleet.example".to_owned()))
		);
		assert_eq!(
			accepted("BLITI_DAEMON_ARGS='--tailnet fleet.example'"),
			Ok(Some("fleet.example".to_owned()))
		);
	}

	#[test]
	fn comments_are_skipped_and_the_last_assignment_wins() {
		let env = "# BLITI_DAEMON_ARGS=\"--tailnet old.example\"\nBLITI_DAEMON_ARGS=\"--tailnet a.example\"\nBLITI_DAEMON_ARGS=\"--tailnet b.example\"\n";
		assert_eq!(accepted(env), Ok(Some("b.example".to_owned())));
	}

	#[test]
	fn options_that_cannot_be_read_are_an_error_and_not_no_restriction() {
		assert!(accepted("BLITI_DAEMON_ARGS=\"--no-such-option\"").is_err());
	}
}
