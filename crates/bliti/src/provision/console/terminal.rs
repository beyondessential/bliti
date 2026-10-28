//! The operator's typing, read without stopping the run's progress from being shown (PCON).
//!
//! Reading a line blocks, and cannot be taken back once begun, so it is done on a thread of its own
//! that the console asks for one line at a time and hears back from over a channel.

use std::io::{self, IsTerminal};

use tokio::sync::mpsc;

/// Where the operator's lines come from.
pub trait Lines: Send + 'static {
	/// Wait for the operator to end a line. What they type is not echoed where `hidden`. Nothing at
	/// the end of the input.
	fn line(&mut self, hidden: bool) -> io::Result<Option<String>>;
}

/// The console's standard input, with no echo for a hidden line where it is a terminal.
pub struct Tty;

impl Lines for Tty {
	fn line(&mut self, hidden: bool) -> io::Result<Option<String>> {
		let stdin = io::stdin();
		if hidden && stdin.is_terminal() {
			// spec: PCON
			return match rpassword::read_password() {
				Ok(line) => Ok(Some(line)),
				Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => Ok(None),
				Err(err) => Err(err),
			};
		}
		let mut line = String::new();
		if stdin.read_line(&mut line)? == 0 {
			return Ok(None);
		}
		let end = line.trim_end_matches(['\n', '\r']).len();
		line.truncate(end);
		Ok(Some(line))
	}
}

/// A [`Lines`] read on its own thread, one line asked for at a time.
pub struct Reader {
	requests: mpsc::UnboundedSender<bool>,
	replies: mpsc::UnboundedReceiver<io::Result<Option<String>>>,
}

impl Reader {
	pub fn spawn(mut lines: impl Lines) -> io::Result<Self> {
		let (requests, mut asked) = mpsc::unbounded_channel::<bool>();
		let (replying, replies) = mpsc::unbounded_channel();
		// A thread of its own, not a task the runtime waits for at shutdown: a read nobody will answer
		// must not hold the console from exiting.
		std::thread::Builder::new()
			.name("console input".to_owned())
			.spawn(move || {
				while let Some(hidden) = asked.blocking_recv() {
					let line = lines.line(hidden);
					let ended = !matches!(line, Ok(Some(_)));
					if replying.send(line).is_err() || ended {
						break;
					}
				}
			})?;
		Ok(Self { requests, replies })
	}

	/// Ask for a line. The thread reads one at a time, so this is to be called with none outstanding.
	pub fn ask(&self, hidden: bool) {
		// The thread has gone only after the input ended, which the reply says.
		let _ = self.requests.send(hidden);
	}

	/// The line asked for, once typed: `None` where the input has ended. Safe to drop and call again.
	pub async fn reply(&mut self) -> Option<io::Result<Option<String>>> {
		self.replies.recv().await
	}
}
