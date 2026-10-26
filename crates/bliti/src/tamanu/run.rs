//! Running `bestool` for what the device reads of Tamanu and does to it.

use std::time::Duration;

use async_trait::async_trait;
use tokio::process::Command as Process;

use super::acts::Act;

/// How long reading the status, or looking for an installation, may take.
const READ: Duration = Duration::from_secs(30);

/// What `status` printed, and how it ended.
#[derive(Debug, Clone, Default)]
pub struct Output {
	/// What it printed, which is the report whether or not it exited well.
	pub stdout: String,
	/// The last thing it said of itself, where it did not exit well.
	pub failure: Option<String>,
}

/// Reads Tamanu and acts on it.
#[async_trait]
pub trait Runner: Send + Sync + 'static {
	/// Read the status, or say why it could not be run.
	async fn status(&self) -> Result<Output, String>;

	/// Whether an installation of Tamanu is found, or why that could not be told.
	async fn find(&self) -> Result<bool, String>;

	/// Carry `act` out, or say why it did not complete. Does not bound itself, which the caller does.
	async fn act(&self, act: Act) -> Result<(), String>;
}

/// The `bestool` command.
#[derive(Debug, Clone)]
pub struct Command {
	program: String,
}

impl Command {
	/// The command named `program`.
	pub fn new(program: impl Into<String>) -> Self {
		Self {
			program: program.into(),
		}
	}

	/// The `bestool` command on the path.
	pub fn on_path() -> Self {
		Self::new("bestool")
	}

	/// Run it, giving up after `limit` where there is one. Killed where it is given up on, or where the
	/// caller stops waiting for it.
	async fn run(
		&self,
		args: &[&str],
		limit: Option<Duration>,
	) -> Result<std::process::Output, String> {
		let running = Process::new(&self.program)
			.args(args)
			.kill_on_drop(true)
			.output();
		let output = match limit {
			Some(limit) => tokio::time::timeout(limit, running)
				.await
				.map_err(|_| format!("`{}` took too long", self.program))?,
			None => running.await,
		};
		output.map_err(|err| format!("cannot run `{}`: {err}", self.program))
	}
}

/// The last line of what a failed command said, kept short.
fn last_line(stderr: &[u8]) -> Option<String> {
	String::from_utf8_lossy(stderr)
		.lines()
		.rev()
		.map(str::trim)
		.find(|line| !line.is_empty())
		.map(|line| line.chars().take(200).collect())
}

#[async_trait]
impl Runner for Command {
	async fn status(&self) -> Result<Output, String> {
		let output = self
			.run(&["tamanu", "status", "--json"], Some(READ))
			.await?;
		Ok(Output {
			stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
			failure: (!output.status.success()).then(|| {
				last_line(&output.stderr)
					.unwrap_or_else(|| "the status did not complete".to_owned())
			}),
		})
	}

	async fn find(&self) -> Result<bool, String> {
		let output = self.run(&["tamanu", "find"], Some(READ)).await?;
		if !output.status.success() {
			return Err(last_line(&output.stderr)
				.unwrap_or_else(|| "looking for Tamanu did not complete".to_owned()));
		}
		Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
	}

	async fn act(&self, act: Act) -> Result<(), String> {
		let output = self.run(&["tamanu", act.name()], None).await?;
		if output.status.success() {
			return Ok(());
		}
		Err(last_line(&output.stderr).unwrap_or_else(|| format!("the {act} did not complete")))
	}
}
