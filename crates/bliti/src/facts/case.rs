//! Whether the case is closed, and when it was last opened (NFO, "Case").
//!
//! The switch is watched on a thread of its own for as long as the daemon runs, whether or not the
//! device is sampling, as the backup supply is: a case is seldom opened with anyone connected. That
//! thread keeps the history and holds what the facts report, and wakes a feed the moment the case
//! moves rather than at its next look for changed facts.

use std::{
	fs, io,
	path::{Path, PathBuf},
	sync::Arc,
	thread,
	time::Duration,
};

use bliti_core::channel::readings::{Entry, kind};
use jiff::{Timestamp, Unit};
use serde_json::Value as Json;
use tokio::sync::watch;

use self::history::{Boot, Event};
use super::product;
use crate::gpio;

mod history;

/// The line the case switch is on. Low while the closed lid holds the switch down.
pub(crate) const LINE: &str = "GPIO17";

/// How long the switch must hold a new position before it counts as having moved, which rules out
/// contact bounce.
const DEBOUNCE: Duration = Duration::from_millis(20);

/// How long to wait before trying the switch again once it could not be read.
const RETRY: Duration = Duration::from_secs(10);

/// The kernel's identifier for this boot.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";

/// Only this version of the product has the switch (NFO).
const FITTED: product::Version = product::Version { major: 4, minor: 0 };

const CLOSED: &str = "case-closed";
const LAST_OPENED: &str = "case-last-opened";

/// The case, as the facts report it. Reports nothing where no switch is fitted.
#[derive(Debug, Clone)]
pub struct Case {
	state: Arc<watch::Sender<Option<State>>>,
}

/// What the watching thread last published, both facts taken together so a change lands in one
/// batch.
#[derive(Debug, Clone, PartialEq)]
struct State {
	/// Whether the case is closed, or why the switch could not be read.
	closed: Result<bool, String>,
	/// The latest opening in the history, or why the history could not be read.
	opened: Result<Option<history::Entry>, String>,
}

/// Wakes when the case's facts change.
#[derive(Debug)]
pub struct Changes(watch::Receiver<Option<State>>);

impl Changes {
	/// Wait for the case's facts to change. Never resolves on a device with no switch.
	pub async fn next(&mut self) {
		// The sender lives as long as any `Case`, and this is only ever held beside one.
		if self.0.changed().await.is_err() {
			std::future::pending::<()>().await;
		}
	}
}

impl Case {
	/// A case with no switch, which reports nothing.
	pub fn none() -> Self {
		Self {
			state: Arc::new(watch::Sender::new(None)),
		}
	}

	/// Start watching the switch, where the board is the version that has one (NFO).
	pub fn watch() -> io::Result<Self> {
		if product::iti_version() != Some(FITTED) {
			return Ok(Self::none());
		}
		let boot_id = fs::read_to_string(BOOT_ID)
			.map(|id| id.trim().to_owned())
			.unwrap_or_else(|err| {
				tracing::warn!(%err, "cannot read this boot's identifier; the case history records none");
				String::new()
			});
		Self::start(
			|| gpio::Edges::request(LINE, DEBOUNCE),
			Path::new(history::DEFAULT_PATH).to_owned(),
			boot_id,
			now,
		)
	}

	/// Watch the switch `open` gives, keeping the history at `path`.
	fn start<S: Switch>(
		mut open: impl FnMut() -> io::Result<S> + Send + 'static,
		path: PathBuf,
		boot_id: String,
		clock: fn() -> (Timestamp, u64),
	) -> io::Result<Self> {
		let case = Self::none();
		let mut watcher = Watcher::new(path, boot_id, clock, case.state.clone());
		thread::Builder::new()
			.name("case".to_owned())
			.spawn(move || {
				loop {
					let err = match open() {
						Ok(mut switch) => watcher.follow(&mut switch),
						Err(err) => err,
					};
					watcher.broken(&err);
					thread::sleep(RETRY);
				}
			})?;
		Ok(case)
	}

	/// What wakes when the case's facts change.
	pub fn changes(&self) -> Changes {
		Changes(self.state.subscribe())
	}

	/// `case-closed` and `case-last-opened`, as last seen (NFO).
	pub fn entries(&self, at: u64) -> Vec<Entry> {
		let Some(state) = self.state.borrow().clone() else {
			return Vec::new();
		};
		let mut entries = vec![match state.closed {
			Ok(true) => Entry::boolean(at, CLOSED, true),
			Ok(false) => Entry::boolean(at, CLOSED, false).warning("the case is open"),
			Err(reason) => Entry::broken(at, CLOSED, kind::BOOLEAN, reason),
		}];
		match state.opened {
			Ok(Some(opening)) => entries.push(
				Entry::datetime(at, LAST_OPENED, opening.time.to_string())
					.with_trait(
						"boot",
						serde_json::to_value(&opening.boot).expect("a boot is plain data"),
					)
					.with_trait("found", Json::Bool(opening.found)),
			),
			Ok(None) => {}
			Err(reason) => entries.push(Entry::broken(at, LAST_OPENED, kind::DATETIME, reason)),
		}
		entries
	}
}

/// The instant by the device's clock, and the milliseconds since it booted.
fn now() -> (Timestamp, u64) {
	let time = Timestamp::now();
	(
		time.round(Unit::Millisecond).unwrap_or(time),
		super::Facts::since_boot(),
	)
}

/// The case switch.
trait Switch {
	/// Whether the case is closed now.
	fn closed(&mut self) -> io::Result<bool>;

	/// Wait for the switch to move, returning whether the case is closed after it.
	fn moved(&mut self) -> io::Result<bool>;
}

impl Switch for gpio::Edges {
	fn closed(&mut self) -> io::Result<bool> {
		self.level().map(|high| !high)
	}

	fn moved(&mut self) -> io::Result<bool> {
		self.next().map(|high| !high)
	}
}

/// The watching thread's own state: what it has seen and recorded, and where it publishes.
struct Watcher {
	path: PathBuf,
	boot_id: String,
	clock: fn() -> (Timestamp, u64),
	state: Arc<watch::Sender<Option<State>>>,
	/// The latest entry in the history, and nothing where the history could not be read.
	recorded: Option<Option<Event>>,
	/// The position last read off the switch, and nothing since it last could not be read.
	seen: Option<bool>,
	closed: Result<bool, String>,
	opened: Result<Option<history::Entry>, String>,
}

impl Watcher {
	fn new(
		path: PathBuf,
		boot_id: String,
		clock: fn() -> (Timestamp, u64),
		state: Arc<watch::Sender<Option<State>>>,
	) -> Self {
		let (recorded, opened) = match history::read(&path) {
			Ok(latest) => (
				Some(latest.entry.map(|entry| entry.event)),
				Ok(latest.opened),
			),
			Err(err) => {
				tracing::warn!(%err, path = %path.display(), "cannot read the case history");
				(None, Err(format!("cannot read the case history: {err}")))
			}
		};
		Self {
			path,
			boot_id,
			clock,
			state,
			recorded,
			seen: None,
			closed: Err("the case switch has not been read yet".to_owned()),
			opened,
		}
	}

	/// Follow the switch until it cannot be read, returning why.
	fn follow(&mut self, switch: &mut impl Switch) -> io::Error {
		match switch.closed() {
			Ok(closed) => self.found(closed),
			Err(err) => return err,
		}
		loop {
			match switch.moved() {
				Ok(closed) => self.moved(closed),
				Err(err) => return err,
			}
		}
	}

	/// The switch has been read without being seen to move: record its position where the history
	/// says otherwise, or says nothing and the case is open (NFO).
	fn found(&mut self, closed: bool) {
		self.seen = Some(closed);
		let differs = match self.recorded {
			Some(Some(event)) => event != Event::to(closed),
			Some(None) => !closed,
			None => false,
		};
		if differs {
			self.record(Event::to(closed), true);
		}
		self.closed = Ok(closed);
		self.publish();
	}

	/// The switch has moved.
	fn moved(&mut self, closed: bool) {
		if self.seen == Some(closed) {
			return;
		}
		self.seen = Some(closed);
		self.record(Event::to(closed), false);
		self.closed = Ok(closed);
		self.publish();
	}

	/// The switch could not be read.
	fn broken(&mut self, err: &io::Error) {
		let reason = format!("cannot read the case switch: {err}");
		if self.closed.as_ref().err() != Some(&reason) {
			tracing::warn!(%err, "cannot read the case switch");
		}
		self.seen = None;
		self.closed = Err(reason);
		self.publish();
	}

	/// Add an entry to the history, on storage before anything reports it.
	fn record(&mut self, event: Event, found: bool) {
		let (time, offset) = (self.clock)();
		let entry = history::Entry {
			event,
			time,
			boot: Boot {
				id: self.boot_id.clone(),
				offset,
			},
			found,
		};
		if let Err(err) = history::append(&self.path, &entry) {
			tracing::warn!(%err, ?event, found, path = %self.path.display(), "cannot add to the case history");
			return;
		}
		tracing::info!(?event, found, "the case moved");
		self.recorded = Some(Some(event));
		if event == Event::Opened {
			self.opened = Ok(Some(entry));
		}
	}

	fn publish(&self) {
		let next = State {
			closed: self.closed.clone(),
			opened: self.opened.clone(),
		};
		self.state.send_if_modified(|current| {
			let changed = current.as_ref() != Some(&next);
			*current = Some(next);
			changed
		});
	}
}

/// A case whose switch starts `closed` and moves when the test says, keeping its history at `path`.
#[cfg(test)]
pub fn moved_by_hand(path: &Path, closed: bool) -> (Case, std::sync::mpsc::Sender<bool>) {
	tests::started(path, Ok(closed))
}

#[cfg(test)]
mod tests;
