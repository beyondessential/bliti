//! Running the tools the medium's system carries.

use std::process::{Command, Output};

/// Run `command`, which succeeds where it exits with status 0.
pub fn run(command: &mut Command) -> Result<Output, String> {
	let output = finish(command)?;
	if output.status.success() {
		return Ok(output);
	}
	Err(failure(command, &output))
}

/// Run `command` and give its output whatever its status was, failing only where it could not run.
pub fn finish(command: &mut Command) -> Result<Output, String> {
	let name = command.get_program().to_string_lossy().into_owned();
	command
		.output()
		.map_err(|err| format!("cannot run {name}: {err}"))
}

/// Why `command` failed with `output`: its name, its status and the last line it said.
pub fn failure(command: &Command, output: &Output) -> String {
	let name = command.get_program().to_string_lossy();
	let stderr = String::from_utf8_lossy(&output.stderr);
	let said = stderr.lines().rev().find(|line| !line.trim().is_empty());
	format!(
		"{name} failed with {}{}",
		output.status,
		said.map(|said| format!(": {}", said.trim()))
			.unwrap_or_default()
	)
}

/// Where the medium's system keeps what is only for this boot, in memory.
pub const RUN: &str = "/run/bliti-provision";

/// A file in memory holding `content`, readable by its owner alone, gone when dropped. A tool that
/// takes a key from a file is given the file rather than the key on its command line.
pub fn secret(content: &str) -> Result<tempfile::NamedTempFile, String> {
	use std::io::Write as _;
	std::fs::create_dir_all(RUN).map_err(|err| format!("cannot make {RUN}: {err}"))?;
	let mut file = tempfile::Builder::new()
		.prefix("key-")
		.tempfile_in(RUN)
		.map_err(|err| format!("cannot make a file for a key: {err}"))?;
	file.write_all(content.as_bytes())
		.and_then(|()| file.flush())
		.map_err(|err| format!("cannot write a key: {err}"))?;
	Ok(file)
}
