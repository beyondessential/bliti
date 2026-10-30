//! The board's record, kept on the provisioning medium (PENC, "The record").

use std::{
	fs::OpenOptions,
	io::{self, Read as _, Seek as _, SeekFrom, Write as _},
	path::Path,
};

use serde_json::{Map, Value as Json, json};

/// The file at the root of the provisioning medium's data partition.
pub const FILE: &str = "records.tsv";

/// The board's record (PRS, "The record").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
	pub suffix: String,
	pub version: Option<String>,
	pub disk: String,
	pub passphrase: String,
	/// When it was saved, as an RFC 3339 date and time in UTC.
	pub at: String,
}

impl Record {
	/// The line of the records file: the fields tab-separated, and a line feed.
	pub fn line(&self) -> String {
		format!(
			"{}\t{}\t{}\t{}\t{}\n",
			self.suffix,
			self.version.as_deref().unwrap_or(""),
			self.disk,
			self.passphrase,
			self.at
		)
	}

	/// Append the record to the records file under `medium`, making the file where there is none, and
	/// flush it to the medium.
	pub fn save(&self, medium: &Path) -> io::Result<()> {
		let mut file = OpenOptions::new()
			.read(true)
			.append(true)
			.create(true)
			.open(medium.join(FILE))?;
		let length = file.seek(SeekFrom::End(0))?;
		let mut line = self.line();
		if length > 0 {
			file.seek(SeekFrom::Start(length - 1))?;
			let mut last = [0u8];
			file.read_exact(&mut last)?;
			if last[0] != b'\n' {
				line.insert(0, '\n');
			}
		}
		file.write_all(line.as_bytes())?;
		file.sync_all()
	}

	/// The member `record` of a step's result (PRS, "The record").
	pub fn result(&self) -> Map<String, Json> {
		let mut record = Map::new();
		record.insert("suffix".to_owned(), json!(self.suffix));
		if let Some(version) = &self.version {
			record.insert("version".to_owned(), json!(version));
		}
		record.insert("disk".to_owned(), json!(self.disk));
		record.insert("passphrase".to_owned(), json!(self.passphrase));
		record.insert("at".to_owned(), json!(self.at));
		let mut result = Map::new();
		result.insert("record".to_owned(), Json::Object(record));
		result
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn record(suffix: &str) -> Record {
		Record {
			suffix: suffix.to_owned(),
			version: Some("4.0".to_owned()),
			disk: "0a1b".to_owned(),
			passphrase: "one-two".to_owned(),
			at: "2026-10-31T01:02:03Z".to_owned(),
		}
	}

	// spec: PENC#the-record
	#[test]
	fn a_line_is_five_tab_separated_fields_ended_by_a_line_feed() {
		assert_eq!(
			record("K7QA").line(),
			"K7QA\t4.0\t0a1b\tone-two\t2026-10-31T01:02:03Z\n"
		);
	}

	// spec: PENC#the-record
	#[test]
	fn a_version_that_was_not_placed_leaves_its_field_empty() {
		let mut unplaced = record("K7QA");
		unplaced.version = None;
		let line = unplaced.line();
		assert_eq!(line.trim_end().split('\t').count(), 5);
		assert_eq!(line.split('\t').nth(1), Some(""));
		assert!(
			!unplaced.result()["record"]
				.as_object()
				.unwrap()
				.contains_key("version")
		);
	}

	// spec: PENC#the-record
	#[test]
	fn saving_makes_the_file_and_adds_a_line_without_a_header_or_changing_the_others() {
		let dir = tempfile::tempdir().unwrap();
		record("AAAA").save(dir.path()).unwrap();
		record("BBBB").save(dir.path()).unwrap();
		let text = fs::read_to_string(dir.path().join(FILE)).unwrap();
		let lines: Vec<&str> = text.lines().collect();
		assert_eq!(lines.len(), 2);
		assert!(lines[0].starts_with("AAAA\t"));
		assert!(lines[1].starts_with("BBBB\t"));
	}

	// spec: PENC#the-record
	#[test]
	fn a_file_that_does_not_end_in_a_line_feed_does_not_swallow_the_new_line() {
		let dir = tempfile::tempdir().unwrap();
		fs::write(dir.path().join(FILE), "old line").unwrap();
		record("BBBB").save(dir.path()).unwrap();
		let text = fs::read_to_string(dir.path().join(FILE)).unwrap();
		assert_eq!(text.lines().next(), Some("old line"));
		assert!(text.lines().nth(1).unwrap().starts_with("BBBB\t"));
	}

	// spec: PENC#the-record
	#[test]
	fn the_result_carries_every_member_the_record_has() {
		let result = record("K7QA").result();
		let record = result["record"].as_object().unwrap();
		assert_eq!(record["suffix"], "K7QA");
		assert_eq!(record["version"], "4.0");
		assert_eq!(record["disk"], "0a1b");
		assert_eq!(record["passphrase"], "one-two");
		assert_eq!(record["at"], "2026-10-31T01:02:03Z");
	}

	use std::fs;
}
