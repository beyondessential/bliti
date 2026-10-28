//! What a device in provisioning mode and a client agree on beyond a device's own keys: the
//! provisioning token, the keys a device without an identity brings to the handshake, and the name it
//! advertises.
//!
//! Behaviour is specified in `.workhorse/specs/provisioning/overview.md` (PRV). The browser client
//! compiles this module, so the device and the client cannot disagree on any of it.

use crate::{
	advertisement::{AD_HEADER, ADVERTISING_BUDGET},
	key_schedule::{
		DEVICE_KEY_LEN, DeviceKeys, DeviceStaticKey, KEM_KEY_DIGEST_LEN, KemKeyDigest,
		PRESENCE_TOKEN_LEN, PreSharedKey, PresenceToken,
	},
	qr::SUFFIX_LEN,
};

pub mod report;

/// The revision of the steps these specifications describe, which a device sends in every
/// `provision-state` (PRS, "The revision").
// spec: PRS
pub const REVISION: u32 = 1;

/// The revisions a client built from this crate implements, lowest first and highest last.
pub const CLIENT_REVISIONS: std::ops::RangeInclusive<u32> = 1..=1;

/// How long before a client's own date an image may have been built and still be provisioned from
/// (PRS, "The image's date"), in days.
// spec: PRS
pub const IMAGE_LIFETIME_DAYS: u32 = 30;

/// The provisioning token: the ASCII text `ITI PROVISIONING` (PRV, "The provisioning token").
///
/// Public, and the same on every device. It takes the place of a presence token in the derivation of
/// the pre-shared key, so the handshake keeps a session from a passive observer and proves nothing
/// about either end.
pub const TOKEN: [u8; PRESENCE_TOKEN_LEN] = *b"ITI PROVISIONING";

/// The provisioning token, as the presence token it stands in for.
pub fn token() -> PresenceToken {
	PresenceToken::from_bytes(TOKEN)
}

/// The pre-shared key every provisioning handshake is keyed by.
pub fn pre_shared_key() -> PreSharedKey {
	token().pre_shared_key()
}

impl DeviceKeys {
	/// The keys a device in provisioning mode brings to the handshake (PRV, "The provisioning token").
	///
	/// `static_key` and `kem_key_digest` are bytes the device generated at random when it started; the
	/// static key is clamped as RFC 7748 requires. Nothing here descends from a board ID, so no
	/// client can check it against a key fingerprint and none does.
	pub fn provisioning(
		mut static_key: [u8; DEVICE_KEY_LEN],
		kem_key_digest: [u8; KEM_KEY_DIGEST_LEN],
	) -> Self {
		static_key[0] &= 0b1111_1000;
		static_key[31] &= 0b0111_1111;
		static_key[31] |= 0b0100_0000;
		Self {
			presence_token: token(),
			pre_shared_key: pre_shared_key(),
			static_key: DeviceStaticKey::from_clamped(static_key),
			kem_key_digest: KemKeyDigest::from_bytes(kem_key_digest),
		}
	}
}

/// What a device's name begins with (PRV, "The advertised name").
pub const NAME_PREFIX: &str = "ITI_PROVISION_";

/// The suffix of a device that cannot establish its board ID (PRV, "The advertised name").
pub const FAULT: &str = "FAULT";

/// The longest name: the prefix, a marker of three digits, an underscore, and the longest suffix.
const LONGEST_NAME: usize = NAME_PREFIX.len() + 3 + 1 + FAULT.len();

const _: () = {
	// The name rides in the scan response, whose one element must fit a legacy advertisement.
	assert!(AD_HEADER + LONGEST_NAME <= ADVERTISING_BUDGET);
	assert!(FAULT.len() != SUFFIX_LEN);
};

/// How a device in provisioning mode tells one board from another in its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suffix {
	/// The last four characters of the encoded QR payload of the board's ID.
	Board(String),
	/// The board's ID could not be established.
	Fault,
}

impl Suffix {
	/// The suffix of a board whose QR payload is `payload`.
	pub fn of(payload: &crate::qr::QrPayload) -> Self {
		Self::Board(payload.suffix())
	}

	/// The suffix as it is written in a name.
	pub fn as_str(&self) -> &str {
		match self {
			Self::Board(suffix) => suffix,
			Self::Fault => FAULT,
		}
	}

	fn read(text: &str) -> Option<Self> {
		if text == FAULT {
			return Some(Self::Fault);
		}
		let base32 = |c: char| matches!(c, 'A'..='Z' | '2'..='7');
		(text.len() == SUFFIX_LEN && text.chars().all(base32)).then(|| Self::Board(text.to_owned()))
	}
}

/// The local name a device in provisioning mode advertises (PRV, "The advertised name").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
	/// The version marker of VER.
	pub marker: u8,
	/// What names the board.
	pub suffix: Suffix,
}

impl Name {
	/// The local name: the prefix, the marker in decimal, an underscore, then the suffix.
	pub fn render(&self) -> String {
		format!("{NAME_PREFIX}{}_{}", self.marker, self.suffix.as_str())
	}

	/// What a client filters on to hear every device at `marker`: the name up to the suffix.
	pub fn filter(marker: u8) -> String {
		format!("{NAME_PREFIX}{marker}_")
	}

	/// Read a local name heard on the air.
	///
	/// `None` where it is not the name of a device in provisioning mode, the ordinary case for every
	/// other device in range.
	pub fn parse(name: &str) -> Option<Self> {
		let rest = name.strip_prefix(NAME_PREFIX)?;
		let (marker, suffix) = rest.split_once('_')?;
		let parsed: u8 = marker.parse().ok()?;
		// Only the way a marker is written, so one name is one marker.
		if parsed.to_string() != marker {
			return None;
		}
		Some(Self {
			marker: parsed,
			suffix: Suffix::read(suffix)?,
		})
	}
}

#[cfg(test)]
mod tests;
