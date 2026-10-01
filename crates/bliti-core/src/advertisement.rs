//! What a device broadcasts, and how a client reads it back.
//!
//! Behaviour is specified in `.workhorse/specs/discovery.md` (ADV). The advertisement
//! carries the service UUID; the local name carries the version marker and the handle, rendered as
//! base32.
//!
//! This lives in the core rather than in the daemon because both ends need it: the device renders
//! the name and the client parses it, and the client is the web application, which compiles this
//! crate to wasm. One implementation means the two cannot drift.

use data_encoding::BASE32_NOPAD;

use crate::{
	key_schedule::{HANDLE_LEN, Handle, PresenceToken},
	qr::QrPayload,
	version::{VERSION_MARKER, implements, markers_reading},
};

/// A legacy advertising payload carries 31 bytes.
pub const ADVERTISING_BUDGET: usize = 31;

/// Every advertising data element costs a length byte and a type byte before its content.
const AD_HEADER: usize = 2;

/// The mandatory flags element: header plus one byte of flags.
const FLAGS_LEN: usize = AD_HEADER + 1;

/// A 128-bit service UUID element: header plus sixteen bytes.
const SERVICE_UUID_LEN: usize = AD_HEADER + 16;

/// The raw payload: version, then handle.
pub const PAYLOAD_LEN: usize = 1 + HANDLE_LEN;

/// The payload rendered as unpadded base32, which is what the local name holds.
pub const LOCAL_NAME_LEN: usize = (PAYLOAD_LEN * 8).div_ceil(5);

/// The advertising budget is a compile-time guarantee rather than something a test happens to check:
/// the flags and the service UUID must fit one legacy advertisement, and the name element must fit
/// one too, since a host places it in the scan response. Growing the payload past what fits breaks
/// the build rather than a device in the field.
const _: () = {
	assert!(FLAGS_LEN + SERVICE_UUID_LEN <= ADVERTISING_BUDGET);
	assert!(AD_HEADER + LOCAL_NAME_LEN <= ADVERTISING_BUDGET);
};

/// What a device advertises: the version marker and the handle.
///
/// A client reads the version before comparing, so that a device speaking a version the client
/// does not hold is reported as exactly that rather than as silence (ADV, "Matching").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Advertised {
	/// The version marker.
	pub version: u8,
	/// The advertised handle.
	pub handle: Handle,
}

impl Advertised {
	/// What the device holding `token` advertises: the highest version marker this build supports.
	/// Fixed for the life of the token, which is what lets a client compute the name before it
	/// listens.
	pub fn new(token: &PresenceToken) -> Self {
		Self::at(VERSION_MARKER, token)
	}

	/// What the device holding `token` advertises at version marker `version`, for a client
	/// computing the name for each marker it considers.
	pub fn at(version: u8, token: &PresenceToken) -> Self {
		Self {
			version,
			handle: token.handle(),
		}
	}

	/// The raw payload bytes: version, then handle. The version leads so a client can read it however
	/// a later version lays out the rest.
	pub fn to_bytes(self) -> [u8; PAYLOAD_LEN] {
		let mut bytes = [0u8; PAYLOAD_LEN];
		bytes[0] = self.version;
		bytes[1..].copy_from_slice(self.handle.as_bytes());
		bytes
	}

	/// The local name a device advertises: the payload as unpadded base32.
	pub fn to_local_name(self) -> String {
		BASE32_NOPAD.encode(&self.to_bytes())
	}

	/// Read a payload from raw bytes. `None` where it is not the right shape.
	pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
		if bytes.len() != PAYLOAD_LEN {
			return None;
		}
		let handle: [u8; HANDLE_LEN] = bytes[1..].try_into().ok()?;
		Some(Self {
			version: bytes[0],
			handle: Handle::from_bytes(handle),
		})
	}

	/// Read a payload from a local name heard on the air.
	///
	/// `None` where the name is not a bliti payload at all, which is the ordinary case for every other
	/// device in range and is passed over rather than reported.
	pub fn from_local_name(name: &str) -> Option<Self> {
		if name.len() != LOCAL_NAME_LEN {
			return None;
		}
		let bytes = BASE32_NOPAD.decode(name.as_bytes()).ok()?;
		Self::from_bytes(&bytes)
	}

	/// Whether this advertisement belongs to the device holding `token`.
	///
	/// The caller checks the version marker first, because a device at a marker the caller does not
	/// implement is to be reported as that, not as a different device. [`heard_by`](Self::heard_by)
	/// does both.
	pub fn matches(self, token: &PresenceToken) -> bool {
		token.handle() == self.handle
	}

	/// What a client holding `code` makes of this advertisement (ADV, "Matching"; VER, "Acting on the
	/// versions"): the marker is read first, and the handle compared only under a marker the client
	/// considers for the code.
	pub fn heard_by(self, code: &QrPayload) -> Heard {
		if !implements(self.version) {
			Heard::Unsupported(self.version)
		} else if markers_reading(code.version()).any(|marker| marker == self.version)
			&& self.matches(code.presence_token())
		{
			Heard::Matches
		} else {
			Heard::Other
		}
	}
}

/// What a client made of one advertisement, against the QR code it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
	/// The device the QR code belongs to.
	Matches,
	/// Another device this client could speak to.
	Other,
	/// A device at a version marker this client does not implement.
	Unsupported(u8),
}

/// The local names the device whose QR code is `code` may advertise: one for each version marker
/// this build implements that reads the code's payload version, highest first. Known before
/// anything is heard, which is what lets a client filter on them.
pub fn local_names(code: &QrPayload) -> Vec<String> {
	markers_reading(code.version())
		.map(|marker| Advertised::at(marker, code.presence_token()).to_local_name())
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_payload_is_the_stated_shape() {
		// That it fits the budget is asserted at compile time above; these are the sizes the spec
		// states, which a client in another language has to agree with.
		assert_eq!(FLAGS_LEN + SERVICE_UUID_LEN, 21);
		assert_eq!(PAYLOAD_LEN, 9);
		assert_eq!(LOCAL_NAME_LEN, 15);
	}

	#[test]
	fn the_rendering_is_the_stated_length() {
		let name = Advertised::new(&PresenceToken::from_bytes([0x11; 16])).to_local_name();
		assert_eq!(name.len(), LOCAL_NAME_LEN);
	}

	#[test]
	fn the_local_name_known_answer() {
		// Pins the layout: version first, then the handle, as unpadded base32. The handle is the
		// known answer of KEY for this token.
		let advertised = Advertised::new(&PresenceToken::from_bytes([0x42; 16]));
		assert_eq!(
			advertised.to_bytes()[1..],
			[0x5e, 0xf4, 0x15, 0xeb, 0xca, 0x67, 0x35, 0x2c]
		);
		assert_eq!(advertised.to_local_name(), "AFPPIFPLZJTTKLA");
	}

	#[test]
	fn a_local_name_round_trips() {
		let advertised = Advertised::new(&PresenceToken::from_bytes([0xab; 16]));
		assert_eq!(
			Advertised::from_local_name(&advertised.to_local_name()),
			Some(advertised)
		);
	}

	#[test]
	fn the_version_marker_leads_and_is_readable_when_unsupported() {
		let advertised = Advertised::new(&PresenceToken::from_bytes([0x01; 16]));
		assert_eq!(advertised.to_bytes()[0], VERSION_MARKER);

		// A client must be able to read a version it does not hold, which is what separates "a device
		// at an unsupported version" from hearing nothing at all.
		let mut bytes = advertised.to_bytes();
		bytes[0] = 99;
		let foreign = Advertised::from_local_name(&BASE32_NOPAD.encode(&bytes)).unwrap();
		assert_eq!(foreign.version, 99);
		assert_eq!(foreign.handle, advertised.handle);
	}

	#[test]
	fn a_name_that_is_not_a_payload_is_passed_over() {
		// Every other device in range has a name of its own; none of them is a bliti device.
		for name in ["", "ATC_ORANGE", "Aranet4 2E4A5", "athom-co2-sen-b34960"] {
			assert_eq!(Advertised::from_local_name(name), None);
		}
		// The right length but not base32.
		assert_eq!(
			Advertised::from_local_name(&"!".repeat(LOCAL_NAME_LEN)),
			None
		);
	}

	fn code(token: [u8; 16]) -> QrPayload {
		QrPayload::new(
			PresenceToken::from_bytes(token),
			crate::key_schedule::KeyFingerprint::from_bytes([0x07; 18]),
		)
	}

	#[test]
	fn a_client_hears_its_device_another_and_an_unsupported_one_apart() {
		let ours = code([0x5a; 16]);
		let theirs = code([0x5b; 16]);
		let advertised = Advertised::new(ours.presence_token());
		assert_eq!(advertised.heard_by(&ours), Heard::Matches);
		assert_eq!(advertised.heard_by(&theirs), Heard::Other);

		// A marker this build does not implement is reported as that, even with a matching handle.
		let later = Advertised {
			version: 99,
			..advertised
		};
		assert_eq!(later.heard_by(&ours), Heard::Unsupported(99));
	}

	#[test]
	fn the_names_a_code_may_be_advertised_under_are_one_per_marker_considered() {
		let ours = code([0x5a; 16]);
		assert_eq!(
			local_names(&ours),
			vec![Advertised::new(ours.presence_token()).to_local_name()]
		);
	}

	#[test]
	fn a_client_matches_only_the_device_whose_code_it_holds() {
		let ours = PresenceToken::from_bytes([0x5a; 16]);
		let theirs = PresenceToken::from_bytes([0x5b; 16]);

		let advertised = Advertised::new(&ours);
		assert!(advertised.matches(&ours));
		assert!(!advertised.matches(&theirs));
	}
}
