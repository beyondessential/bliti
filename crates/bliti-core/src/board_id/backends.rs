//! Board ID backends that read real firmware, gated behind the `backends` feature so a wasm build,
//! which has no filesystem and never reads a board ID, does not pull them.
//!
//! The two platform-serial backends live here; the stronger two have a module each, because each
//! reaches hardware in its own way. [`OneTimeProgrammableSource`] reads the kernel's nvmem
//! interface, and [`TpmEndorsementKeySource`] talks to the TPM software stack, which carries a C
//! library and so sits behind the `tpm` feature.
//!
//! Which backends a build registers decides which source wins the precedence, and so which QR code
//! secret a board derives. A binary that can derive a QR code therefore registers every backend the
//! platform could offer rather than a subset.

use std::{fs, path::PathBuf};

use super::{BoardIdError, BoardIdSource, Presence, SourceKind, is_sentinel};

mod otp;
pub use otp::OneTimeProgrammableSource;

#[cfg(feature = "tpm")]
mod tpm;
#[cfg(feature = "tpm")]
pub use tpm::TpmEndorsementKeySource;

/// The Raspberry Pi device-tree serial, a 64-bit value the board exposes as sixteen hexadecimal
/// characters. The board ID is the eight bytes those characters denote, most significant first, not
/// the characters themselves (KEY, "What is derived from").
#[derive(Debug, Clone)]
pub struct RaspberryPiSerialSource {
	path: PathBuf,
}

impl Default for RaspberryPiSerialSource {
	fn default() -> Self {
		Self::new()
	}
}

impl RaspberryPiSerialSource {
	/// Read from the standard device-tree location.
	pub fn new() -> Self {
		Self::at("/proc/device-tree/serial-number")
	}

	/// Read from a given path, for testing against a fixture on hardware that has no such serial.
	pub fn at(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}

	fn read_bytes(&self) -> Result<Option<[u8; 8]>, BoardIdError> {
		let raw = match fs::read(&self.path) {
			Ok(raw) => raw,
			Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
			Err(err) => return Err(self.backend_error(err)),
		};
		// The device-tree property is a NUL-terminated string of hex digits.
		let text = String::from_utf8_lossy(&raw);
		let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
		if text.is_empty() {
			return Ok(None);
		}
		let value = u64::from_str_radix(text, 16).map_err(|err| {
			self.backend_message(format!("serial {text:?} is not hexadecimal: {err}"))
		})?;
		Ok(Some(value.to_be_bytes()))
	}

	fn backend_error(&self, err: std::io::Error) -> BoardIdError {
		self.backend_message(err.to_string())
	}

	fn backend_message(&self, message: impl Into<String>) -> BoardIdError {
		BoardIdError::Backend {
			kind: SourceKind::RaspberryPiSerial,
			message: message.into(),
		}
	}
}

impl BoardIdSource for RaspberryPiSerialSource {
	fn kind(&self) -> SourceKind {
		SourceKind::RaspberryPiSerial
	}

	fn probe(&self) -> Result<Presence, BoardIdError> {
		match self.read_bytes()? {
			None => Ok(Presence::Absent),
			Some(bytes) if is_sentinel(&bytes) => Ok(Presence::Placeholder),
			Some(_) => Ok(Presence::Present),
		}
	}

	fn read(&self) -> Result<Vec<u8>, BoardIdError> {
		match self.read_bytes()? {
			Some(bytes) => Ok(bytes.to_vec()),
			None => Err(self.backend_message("serial disappeared between probe and read")),
		}
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use std::{
		path::PathBuf,
		sync::atomic::{AtomicU32, Ordering},
	};

	use super::*;

	/// A unique scratch path in the temp dir, cleaned up on drop.
	pub(crate) struct Fixture(PathBuf);

	impl Fixture {
		pub(crate) fn new(contents: &[u8]) -> Self {
			static COUNTER: AtomicU32 = AtomicU32::new(0);
			let n = COUNTER.fetch_add(1, Ordering::Relaxed);
			let path =
				std::env::temp_dir().join(format!("bliti-fixture-{}-{n}", std::process::id()));
			fs::write(&path, contents).unwrap();
			Self(path)
		}

		pub(crate) fn path(&self) -> &PathBuf {
			&self.0
		}
	}

	impl Drop for Fixture {
		fn drop(&mut self) {
			let _ = fs::remove_file(&self.0);
		}
	}

	#[test]
	fn raspberry_pi_serial_reads_the_bytes_the_hex_denotes() {
		// The real device-tree property: a NUL-terminated string of hex digits (verified on a Pi 5).
		let f = Fixture::new(b"f3756510f632cfad\0");
		let source = RaspberryPiSerialSource::at(&f.0);
		assert_eq!(source.probe().unwrap(), Presence::Present);
		assert_eq!(
			source.read().unwrap(),
			vec![0xf3, 0x75, 0x65, 0x10, 0xf6, 0x32, 0xcf, 0xad]
		);
	}

	#[test]
	fn raspberry_pi_serial_absent_when_missing() {
		let source = RaspberryPiSerialSource::at("/nonexistent/bliti/serial-number");
		assert_eq!(source.probe().unwrap(), Presence::Absent);
	}

	#[test]
	fn raspberry_pi_serial_placeholder_when_all_zero() {
		let f = Fixture::new(b"0000000000000000\0");
		let source = RaspberryPiSerialSource::at(&f.0);
		assert_eq!(source.probe().unwrap(), Presence::Placeholder);
	}

	/// Evaluates the real precedence against the machine this runs on, exercising the backends
	/// together rather than one at a time, and reports what it found so it doubles as a diagnostic.
	/// Ignored: reads real hardware and needs privilege for the TPM.
	#[test]
	#[ignore = "reads the real board; needs privilege for the TPM"]
	fn selects_the_strongest_source_on_this_board() {
		use crate::board_id::{BoardIdSource, select, strongest_present};

		let otp = OneTimeProgrammableSource::new();
		let rpi = RaspberryPiSerialSource::new();
		#[cfg(feature = "tpm")]
		let tpm = TpmEndorsementKeySource::new();

		#[cfg_attr(
			not(feature = "tpm"),
			expect(
				unused_mut,
				reason = "the TPM source is pushed only when that feature is on"
			)
		)]
		let mut sources: Vec<&dyn BoardIdSource> = vec![&otp, &rpi];
		#[cfg(feature = "tpm")]
		sources.push(&tpm);

		for source in &sources {
			println!(
				"{:>28}: {:?}",
				source.kind().to_string(),
				source.probe().unwrap()
			);
		}

		// The cheap probe and the full selection must agree on which source wins, since the cache
		// check in KEY relies on the probe alone to decide whether a rederivation is needed.
		let strongest = strongest_present(&sources).unwrap();
		let board_id = select(&sources).expect("this board offers a usable source");
		println!(
			"selected {} ({} bytes)",
			board_id.kind(),
			board_id.raw().len()
		);
		assert_eq!(Some(board_id.kind()), strongest);
	}
}
