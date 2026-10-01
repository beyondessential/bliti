//! The two numbers that version bliti: the payload version a QR code carries, and the version
//! marker a device advertises.
//!
//! Behaviour is specified in `.workhorse/specs/version.md` (VER). The payload version covers the
//! QR payload and the key schedule, everything between a board ID and a QR code; the version marker
//! covers everything two ends must agree on before or during a session, including which payload
//! version it reads. A QR code stays valid under every marker that reads its payload version, so a
//! protocol change that leaves the key schedule alone orphans no printed code.

/// The payload version this build writes into a QR code, and the one its key schedule derives under.
pub const PAYLOAD_VERSION: u8 = 1;

/// The version marker this build's device advertises: the highest it supports.
pub const VERSION_MARKER: u8 = 1;

/// Every version marker this build implements, each with the payload version it reads.
const MARKERS: &[(u8, u8)] = &[(1, 1)];

const _: () = {
	let mut highest = 0;
	let mut i = 0;
	while i < MARKERS.len() {
		if MARKERS[i].0 > highest {
			highest = MARKERS[i].0;
		}
		i += 1;
	}
	assert!(highest == VERSION_MARKER);
};

/// The version markers this build implements that read `payload_version`, highest first. Empty
/// where this build reads no QR code at that payload version.
pub fn markers_reading(payload_version: u8) -> impl Iterator<Item = u8> {
	let mut markers: Vec<u8> = MARKERS
		.iter()
		.filter(|(_, reads)| *reads == payload_version)
		.map(|(marker, _)| *marker)
		.collect();
	markers.sort_unstable_by(|a, b| b.cmp(a));
	markers.into_iter()
}

/// Whether this build implements `marker`.
pub fn implements(marker: u8) -> bool {
	MARKERS
		.iter()
		.any(|(implemented, _)| *implemented == marker)
}

/// Whether this build reads QR codes at `payload_version`.
pub fn reads_payload(payload_version: u8) -> bool {
	markers_reading(payload_version).next().is_some()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_versions_defined() {
		// VER, "The versions defined".
		assert_eq!(PAYLOAD_VERSION, 1);
		assert_eq!(VERSION_MARKER, 1);
		assert_eq!(markers_reading(1).collect::<Vec<_>>(), vec![1]);
	}

	#[test]
	fn a_payload_version_no_marker_reads_has_none() {
		assert_eq!(markers_reading(2).count(), 0);
		assert!(!reads_payload(2));
		assert!(reads_payload(PAYLOAD_VERSION));
	}

	#[test]
	fn the_advertised_marker_reads_the_written_payload_version() {
		assert!(implements(VERSION_MARKER));
		assert!(markers_reading(PAYLOAD_VERSION).any(|marker| marker == VERSION_MARKER));
		assert!(!implements(0));
		assert!(!implements(99));
	}
}
