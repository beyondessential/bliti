//! The history of the case's openings and closings, one JSON object a line (NFO, "The history").
//!
//! Only ever appended to. A line torn by a power cut is passed over when reading, and the next
//! append starts on a line of its own rather than rewriting it.

use std::{
	fs::{self, File, OpenOptions},
	io::{self, Read as _, Seek as _, SeekFrom, Write as _},
	path::Path,
};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

/// Where the history is kept.
pub const DEFAULT_PATH: &str = "/var/lib/bliti/case-history.jsonl";

/// One line of the history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
	pub event: Event,
	pub time: Timestamp,
	pub boot: Boot,
	/// Whether the device found the case so when it started rather than saw it move.
	pub found: bool,
}

/// Which way the case moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Event {
	Opened,
	Closed,
}

impl Event {
	/// The event that leaves the case in this position.
	pub fn to(closed: bool) -> Self {
		if closed { Self::Closed } else { Self::Opened }
	}
}

/// The boot something happened in, and how far into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boot {
	/// The kernel's identifier for the boot.
	pub id: String,
	/// Milliseconds into the boot.
	pub offset: u64,
}

/// What the device needs of the history when it starts: the latest entry, to compare the switch
/// with, and the latest opening, to report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Latest {
	pub entry: Option<Entry>,
	pub opened: Option<Entry>,
}

/// Read the history at `path`. A history not yet written holds nothing.
pub fn read(path: &Path) -> io::Result<Latest> {
	let raw = match fs::read_to_string(path) {
		Ok(raw) => raw,
		Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Latest::default()),
		Err(err) => return Err(err),
	};
	Ok(latest(&raw))
}

/// The latest entry and the latest opening in the text of a history, passing over any line that is
/// not an entry, as the one a power cut tore is not.
fn latest(raw: &str) -> Latest {
	let mut latest = Latest::default();
	for line in raw.lines() {
		let Ok(entry) = serde_json::from_str::<Entry>(line) else {
			if !line.trim().is_empty() {
				tracing::warn!(%line, "passing over a line of the case history that is not an entry");
			}
			continue;
		};
		if entry.event == Event::Opened {
			latest.opened = Some(entry.clone());
		}
		latest.entry = Some(entry);
	}
	latest
}

/// Add an entry to the end of the history at `path`, on storage before this returns.
pub fn append(path: &Path, entry: &Entry) -> io::Result<()> {
	let directory = path.parent().unwrap_or(Path::new("."));
	fs::create_dir_all(directory)?;
	let created = !path.exists();
	let mut file = OpenOptions::new()
		.read(true)
		.append(true)
		.create(true)
		.open(path)?;

	let mut line = serde_json::to_vec(entry).map_err(io::Error::other)?;
	line.push(b'\n');
	if !ends_a_line(&mut file)? {
		line.insert(0, b'\n');
	}
	file.write_all(&line)?;
	file.sync_all()?;
	if created {
		File::open(directory)?.sync_all()?;
	}
	Ok(())
}

/// Whether the file is empty or ends with a newline, so the next line starts on a line of its own.
fn ends_a_line(file: &mut File) -> io::Result<bool> {
	if file.seek(SeekFrom::End(0))? == 0 {
		return Ok(true);
	}
	file.seek(SeekFrom::End(-1))?;
	let mut last = [0u8];
	file.read_exact(&mut last)?;
	Ok(last[0] == b'\n')
}

#[cfg(test)]
mod tests {
	use super::*;

	fn entry(event: Event, second: i64) -> Entry {
		Entry {
			event,
			time: Timestamp::from_second(1_790_000_000 + second).unwrap(),
			boot: Boot {
				id: "0f6b5b4e-3a52-4ad6-9d1c-2c3e4f5a6b7c".to_owned(),
				offset: 12_345,
			},
			found: false,
		}
	}

	// spec: NFO
	#[test]
	fn an_entry_is_one_line_of_the_documented_members() {
		let line = serde_json::to_value(entry(Event::Opened, 0)).unwrap();
		assert_eq!(
			line,
			serde_json::json!({
				"event": "opened",
				"time": "2026-09-21T14:13:20Z",
				"boot": {"id": "0f6b5b4e-3a52-4ad6-9d1c-2c3e4f5a6b7c", "offset": 12_345},
				"found": false,
			})
		);
		let back: Entry = serde_json::from_value(line).unwrap();
		assert_eq!(back, entry(Event::Opened, 0));
	}

	#[test]
	fn a_history_not_yet_written_holds_nothing() {
		let dir = tempfile::tempdir().unwrap();
		let read = read(&dir.path().join("case-history.jsonl")).unwrap();
		assert_eq!(read, Latest::default());
	}

	// spec: NFO
	#[test]
	fn the_latest_entry_and_opening_are_found() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("case-history.jsonl");
		for (event, second) in [
			(Event::Opened, 0),
			(Event::Closed, 1),
			(Event::Opened, 2),
			(Event::Closed, 3),
		] {
			append(&path, &entry(event, second)).unwrap();
		}
		let read = read(&path).unwrap();
		assert_eq!(read.entry, Some(entry(Event::Closed, 3)));
		assert_eq!(read.opened, Some(entry(Event::Opened, 2)));
	}

	#[test]
	fn a_history_with_no_opening_reports_none() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("case-history.jsonl");
		append(&path, &entry(Event::Closed, 0)).unwrap();
		assert_eq!(read(&path).unwrap().opened, None);
	}

	// spec: NFO
	#[test]
	fn a_torn_final_line_is_passed_over() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("case-history.jsonl");
		append(&path, &entry(Event::Opened, 0)).unwrap();
		let mut file = OpenOptions::new().append(true).open(&path).unwrap();
		file.write_all(br#"{"event":"closed","ti"#).unwrap();

		let read = read(&path).unwrap();
		assert_eq!(read.entry, Some(entry(Event::Opened, 0)));
	}

	// spec: NFO
	#[test]
	fn an_append_after_a_torn_line_starts_a_line_of_its_own() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("case-history.jsonl");
		append(&path, &entry(Event::Opened, 0)).unwrap();
		let mut file = OpenOptions::new().append(true).open(&path).unwrap();
		file.write_all(br#"{"event":"closed","ti"#).unwrap();
		drop(file);
		let before = fs::read(&path).unwrap();

		append(&path, &entry(Event::Closed, 5)).unwrap();

		let after = fs::read(&path).unwrap();
		assert!(after.starts_with(&before), "nothing earlier is rewritten");
		assert_eq!(after[before.len()], b'\n');
		assert_eq!(read(&path).unwrap().entry, Some(entry(Event::Closed, 5)));
	}

	// spec: NFO
	#[test]
	fn earlier_lines_are_left_as_they_were() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("case-history.jsonl");
		append(&path, &entry(Event::Opened, 0)).unwrap();
		append(&path, &entry(Event::Closed, 1)).unwrap();
		let before = fs::read(&path).unwrap();

		append(&path, &entry(Event::Opened, 2)).unwrap();

		let after = fs::read(&path).unwrap();
		assert!(after.starts_with(&before));
		assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 3);
	}

	#[test]
	fn the_directory_is_made_where_there_is_none() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("lib/bliti/case-history.jsonl");
		append(&path, &entry(Event::Opened, 0)).unwrap();
		assert_eq!(read(&path).unwrap().entry, Some(entry(Event::Opened, 0)));
	}
}
