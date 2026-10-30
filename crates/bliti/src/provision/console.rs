//! The provisioning console command: a run driven from the device's own console with text prompts
//! (PCON).
//!
//! It speaks the provisioning stream to the daemon over the daemon's socket, as a client does over
//! the channel, so what it shows and what it can ask for are the stream's. It prints a line for each
//! change and never redraws, since the console may be a serial line. The run belongs to the device:
//! leaving the console, or losing it, closes the connection and nothing else.
//!
//! The decisions are in [`selection`], [`question`] and [`report`], which are plain functions of the
//! state; this module holds the loop that joins them to the stream and the terminal.

mod question;
mod report;
mod selection;
mod terminal;

use std::{io, path::Path, pin::pin};

use anyhow::{Context as _, Result, anyhow, bail};
use bliti_core::{
	channel::{
		envelope::{Reading, read},
		messages::Message,
	},
	provisioning::report::Answer,
};
use futures::{AsyncRead, AsyncWrite, StreamExt as _};
use tokio::net::UnixStream;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};

use self::{
	question::{Handled, Question, Typed, Verb, next, read_answer, read_menu},
	report::Reporter,
	selection::{Pick, Selection},
	terminal::{Lines, Reader, Tty},
};
use super::Snapshot;
use crate::carrier::{Carrier, incoming};

/// What the console names itself as to the daemon, for its log.
const NAME: &str = "bliti-console";
const VERSION: &str = env!("BLITI_VERSION");

/// Drive a run through the daemon listening on `socket`.
pub async fn run(socket: &Path) -> Result<()> {
	let stream = connect(socket).await?;
	let (reader, writer) = stream.into_split();
	attach(reader.compat(), writer.compat_write(), Tty, io::stdout()).await
}

async fn connect(socket: &Path) -> Result<UnixStream> {
	UnixStream::connect(socket).await.map_err(|err| match err.kind() {
		io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => anyhow!(
			"no provisioning daemon answers at {}: this command runs only on a device booted in provisioning mode",
			socket.display()
		),
		io::ErrorKind::PermissionDenied => anyhow!(
			"not allowed to reach the provisioning daemon at {}: run this command as root",
			socket.display()
		),
		_ => anyhow::Error::new(err)
			.context(format!("reaching the provisioning daemon at {}", socket.display())),
	})
}

/// Open the provisioning stream on a connection to the daemon, and drive it from `lines`, showing
/// everything on `out`, until the operator leaves or the connection ends.
async fn attach<R, W, L, O>(reader: R, mut writer: W, lines: L, out: O) -> Result<()>
where
	R: AsyncRead + Unpin,
	W: AsyncWrite + Unpin,
	L: Lines,
	O: io::Write + Send,
{
	let mut incoming = pin!(incoming(reader, Carrier::Socket));
	let hello = Message::Hello {
		name: NAME.to_owned(),
		version: VERSION.to_owned(),
	};
	for message in [hello, Message::Provision] {
		Carrier::Socket
			.write(&mut writer, &message)
			.await
			.context("writing to the daemon")?;
	}

	let mut input = Reader::spawn(lines).context("starting to read the console")?;
	let mut console = Console::new(out);
	loop {
		console.ask(&input)?;
		let reply = tokio::select! {
			raw = incoming.next() => {
				let raw = raw
					.ok_or_else(|| anyhow!("the provisioning daemon closed the connection"))?
					.context("reading from the daemon")?;
				match read::<Message>(&raw) {
					Ok(Reading::Message(message)) => console.hear(&message)?,
					Ok(Reading::Skipped(_) | Reading::Refused(_)) => {}
					Err(fault) => bail!("the daemon sent something that is not a message: {fault}"),
				}
				if console.leaving {
					return Ok(());
				}
				continue;
			}
			line = input.reply() => console.typed(line)?,
		};
		match reply {
			Reply::Nothing => {}
			Reply::Send(message) => Carrier::Socket
				.write(&mut writer, &message)
				.await
				.context("writing to the daemon")?,
			Reply::Leave => return Ok(()),
		}
	}
}

/// What typing a line comes to.
enum Reply {
	Nothing,
	Send(Message),
	Leave,
}

/// An act sent, held against asking anything more until the device has acted on it.
struct Pending {
	/// The state the act was sent against.
	sent_at: Snapshot,
	/// Whether the device has accepted it. The state that follows comes after, on the same stream.
	accepted: bool,
}

struct Console<O> {
	out: O,
	snapshot: Option<Snapshot>,
	reporter: Reporter,
	selection: Option<Selection>,
	handled: Handled,
	/// The question the operator is being asked, with a line asked for and not yet typed.
	asking: Option<Question>,
	pending: Option<Pending>,
	/// What was printed to ask the question being asked, to print again below what comes meanwhile.
	marker: Option<String>,
	/// Whether the operator has been told the question asked has closed.
	told: bool,
	explained: bool,
	/// Set where there is nothing for the console to do.
	leaving: bool,
}

impl<O: io::Write> Console<O> {
	fn new(out: O) -> Self {
		Self {
			out,
			snapshot: None,
			reporter: Reporter::default(),
			selection: None,
			handled: Handled::default(),
			asking: None,
			pending: None,
			marker: None,
			told: false,
			explained: false,
			leaving: false,
		}
	}

	fn say(&mut self, line: &str) -> io::Result<()> {
		self.say_all(&[line.to_owned()])
	}

	/// Print lines, below the marker of a question that is waiting for a line and with that marker
	/// again after, so the question stays last.
	fn say_all(&mut self, lines: &[String]) -> io::Result<()> {
		if lines.is_empty() {
			return Ok(());
		}
		if self.marker.is_some() {
			writeln!(self.out)?;
		}
		for line in lines {
			writeln!(self.out, "{line}")?;
		}
		if let Some(marker) = &self.marker {
			write!(self.out, "{marker}")?;
		}
		self.out.flush()
	}

	/// Take in a message from the device.
	fn hear(&mut self, message: &Message) -> io::Result<()> {
		match message {
			Message::ProvisionState {
				revision,
				built,
				run,
				steps,
				prompts,
			} => {
				let snapshot = Snapshot {
					revision: *revision,
					built: *built,
					run: *run,
					steps: steps.clone(),
					prompts: prompts.clone(),
				};
				if snapshot.steps.is_empty() {
					self.leaving = true;
					return self.say("This device offers no steps to run.");
				}
				self.handled.retain_open(&snapshot);
				let lines = self.reporter.update(&snapshot);
				self.say_all(&lines)?;
				if self
					.pending
					.as_ref()
					.is_some_and(|pending| pending.accepted && pending.sent_at != snapshot)
				{
					self.pending = None;
				}
				let rechecked = self
					.selection
					.as_mut()
					.is_some_and(|selection| selection.refresh(&snapshot.steps));
				if rechecked && self.asking == Some(Question::Choose) {
					self.say("The device has checked the steps again:")?;
					self.show_selection()?;
				}
				self.snapshot = Some(snapshot);
				// A read cannot be taken back, so the line typed next is the last of that question.
				if !self.told
					&& self
						.asking
						.as_ref()
						.is_some_and(|asked| self.want().as_ref() != Some(asked))
				{
					self.told = true;
					self.say("That question has closed; press Enter to go on.")?;
				}
			}
			Message::ProvisionAccepted => {
				if let Some(pending) = &mut self.pending {
					pending.accepted = true;
				}
			}
			// spec: PRS
			Message::CannotAct { act, at, reason } => {
				self.pending = None;
				self.say(&format!("The device refused {act} at {at}: {reason}"))?;
			}
			_ => {}
		}
		Ok(())
	}

	fn want(&self) -> Option<Question> {
		let snapshot = self.snapshot.as_ref()?;
		next(snapshot, &self.handled, self.reporter.all_shown(snapshot))
	}

	fn show_selection(&mut self) -> io::Result<()> {
		let Some(selection) = &self.selection else {
			return Ok(());
		};
		let mut lines = vec!["Steps, in the order they run:".to_owned()];
		lines.extend(selection.lines());
		lines.push(
			"Type step numbers or names to tick or untick them, all, or none. \
			 Enter starts a run of the ticked steps; q leaves without starting."
				.to_owned(),
		);
		self.say_all(&lines)
	}

	/// Ask the operator the next question, unless one is already asked or the device is yet to act on
	/// something sent.
	fn ask(&mut self, input: &Reader) -> io::Result<()> {
		if self.asking.is_some() || self.pending.is_some() {
			return Ok(());
		}
		let question = self.want();
		if question != Some(Question::Choose) {
			self.selection = None;
		}
		let Some(question) = question else {
			return Ok(());
		};
		let marker = match &question {
			Question::Choose => {
				if self.selection.is_none() {
					let steps = self.snapshot.as_ref().map(|s| s.steps.clone());
					self.selection = steps.as_deref().map(Selection::of);
				}
				self.show_selection()?;
				"> ".to_owned()
			}
			Question::Prompt { id, asks, default } => {
				if !self.explained {
					self.explained = true;
					self.say(
						"While answering: :skip leaves the question for later, :stop ends a waiting \
						 or failed run, :quit leaves the console with the run as it is.",
					)?;
				}
				match asks {
					question::Asks::Confirm => format!("{id} [y/n]> "),
					question::Asks::Text => match default {
						Some(default) => format!("{id} [{default}]> "),
						None => format!("{id}> "),
					},
					question::Asks::Secret => format!("{id} (not shown as you type)> "),
				}
			}
			Question::Waiting => "stop to end the run, or q to leave it running> ".to_owned(),
			Question::Failed => "retry, stop to end the run, or q to leave it> ".to_owned(),
			Question::Finished => {
				"Acknowledge the finished run, which discards it? [Y/n]> ".to_owned()
			}
		};
		write!(self.out, "{marker}")?;
		self.out.flush()?;
		self.marker = Some(marker);
		input.ask(question.hidden());
		self.asking = Some(question);
		Ok(())
	}

	/// Take the line the operator typed, for the question that was asked.
	fn typed(&mut self, line: Option<io::Result<Option<String>>>) -> Result<Reply> {
		self.told = false;
		self.marker = None;
		let line = match line {
			Some(Ok(Some(line))) => line,
			Some(Err(err)) => return Err(err).context("reading the console"),
			Some(Ok(None)) | None => {
				self.say("\nThe console has no more input; leaving the run as it is.")?;
				return Ok(Reply::Leave);
			}
		};
		let Some(asked) = self.asking.take() else {
			return Ok(Reply::Nothing);
		};
		if self.want().as_ref() != Some(&asked) {
			self.say(
				"That question was settled before you answered, so what you typed is ignored.",
			)?;
			return Ok(Reply::Nothing);
		}
		match asked {
			Question::Choose => self.chosen(&line),
			Question::Prompt { id, asks, default } => {
				match read_answer(asks, default.as_deref(), &line) {
					Typed::Answer(value) => {
						self.handled.answered(&id);
						Ok(self.send(answer(&id, value)))
					}
					Typed::Verb(verb) => self.verb(verb, Some(&id)),
					Typed::Bad(why) => self.refuse(&why),
				}
			}
			menu => match read_menu(&menu, &line) {
				Typed::Verb(verb) => self.verb(verb, None),
				Typed::Bad(why) => self.refuse(&why),
				Typed::Answer(_) => unreachable!("a menu takes no answer"),
			},
		}
	}

	fn refuse(&mut self, why: &str) -> Result<Reply> {
		self.say(why)?;
		Ok(Reply::Nothing)
	}

	fn chosen(&mut self, line: &str) -> Result<Reply> {
		let Some(selection) = &mut self.selection else {
			return Ok(Reply::Nothing);
		};
		let pick = match selection.parse(line) {
			Ok(pick) => pick,
			Err(why) => return self.refuse(&why),
		};
		match pick {
			Pick::Quit => {
				self.say("Nothing started.")?;
				Ok(Reply::Leave)
			}
			Pick::Start => {
				// spec: PCON
				let steps = selection.chosen();
				if steps.is_empty() {
					return self.refuse("No step is ticked; tick one, or q to leave.");
				}
				self.say(&format!("Starting: {}", steps.join(", ")))?;
				Ok(self.send(Message::ProvisionAct {
					act: "start".to_owned(),
					steps: Some(steps),
					prompt: None,
					value: None,
				}))
			}
			ticks => {
				selection.apply(&ticks);
				Ok(Reply::Nothing)
			}
		}
	}

	fn verb(&mut self, verb: Verb, prompt: Option<&str>) -> Result<Reply> {
		Ok(match verb {
			Verb::Retry => self.send(act("retry")),
			Verb::Stop => self.send(act("stop")),
			Verb::Acknowledge => self.send(act("acknowledge")),
			Verb::Skip => {
				if let Some(id) = prompt {
					self.handled.skipped(id);
				}
				Reply::Nothing
			}
			Verb::Quit => {
				if self.snapshot.as_ref().is_some_and(|snapshot| {
					snapshot.run != bliti_core::provisioning::report::RunState::Idle
				}) {
					self.say("Leaving; the run carries on at the device.")?;
				}
				Reply::Leave
			}
		})
	}

	fn send(&mut self, message: Message) -> Reply {
		if let Some(sent_at) = self.snapshot.clone() {
			self.pending = Some(Pending {
				sent_at,
				accepted: false,
			});
		}
		Reply::Send(message)
	}
}

fn act(name: &str) -> Message {
	Message::ProvisionAct {
		act: name.to_owned(),
		steps: None,
		prompt: None,
		value: None,
	}
}

fn answer(prompt: &str, value: Answer) -> Message {
	Message::ProvisionAct {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some(prompt.to_owned()),
		value: Some(value),
	}
}

#[cfg(test)]
mod tests;
