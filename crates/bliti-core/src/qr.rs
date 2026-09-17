//! The QR code payload: what the QR code carries, and how it is rendered and read back.
//!
//! Behaviour is specified in `.workhorse/specs/QR code.md` (QR). The payload carries the
//! presence token and the version marker, and nothing else. The board ID is deliberately absent:
//! putting it here would hand the board ID to anyone who photographs a QR code, which is the
//! property the derivation exists to provide.

use data_encoding::BASE32_NOPAD;

use crate::key_schedule::{PRESENCE_TOKEN_LEN, PresenceToken, VERSION};

/// The URL the QR code encodes. The payload rides in the fragment, which a browser never sends to a
/// server, so the secret stays on the device that scanned it. A generic phone camera opens this
/// page; a native application can claim the link.
///
/// Kept lower case. The scheme and host are case insensitive to a browser, and upper casing them
/// would shrink the code further, but a native application claims a link by matching the scheme and
/// host literally, so the saving would cost the property above.
pub const QR_URL_BASE: &str = "https://bliti.tamanu.app/";

/// The number of characters per group in the human-readable rendering.
const HUMAN_GROUP: usize = 4;

/// The decoded contents of a QR code: the version marker and the presence token.
#[derive(Clone, PartialEq, Eq)]
pub struct QrPayload {
	version: u8,
	secret: PresenceToken,
}

impl QrPayload {
	/// A payload for the current version.
	pub fn new(secret: PresenceToken) -> Self {
		Self {
			version: VERSION,
			secret,
		}
	}

	/// The version marker read from the payload.
	pub fn version(&self) -> u8 {
		self.version
	}

	/// The presence token.
	pub fn secret(&self) -> &PresenceToken {
		&self.secret
	}

	/// The raw payload bytes: the version marker followed by the secret.
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(1 + PRESENCE_TOKEN_LEN);
		bytes.push(self.version);
		bytes.extend_from_slice(self.secret.as_bytes());
		bytes
	}

	/// The fragment the payload rides in: the raw bytes as unpadded base32, the same characters the
	/// human-readable rendering carries, ungrouped.
	///
	/// Base32 rather than base64url because of what it costs to print. A QR code encodes digits and
	/// upper-case letters at five and a half bits a character, and anything else at eight, so the
	/// longer base32 rendering occupies fewer bits than the shorter mixed-case one and the printed
	/// code comes out measurably coarser: 45 modules a side against 49 at the same error correction,
	/// which is a fifth more area per module for a camera to resolve. A QR code is read off an
	/// enclosure by a phone, so that is worth more than a shorter URL.
	pub fn to_fragment(&self) -> String {
		BASE32_NOPAD.encode(&self.to_bytes())
	}

	/// The full URL the QR code encodes.
	pub fn to_url(&self) -> String {
		format!("{QR_URL_BASE}#{}", self.to_fragment())
	}

	/// The human-readable rendering printed beneath the code, so a device whose code is scuffed
	/// remains usable. It is unpadded base32 (no ambiguous 0/1/8/9), grouped for legibility, and
	/// carries the same payload as the fragment.
	pub fn to_human(&self) -> String {
		let encoded = BASE32_NOPAD.encode(&self.to_bytes());
		encoded
			.as_bytes()
			.chunks(HUMAN_GROUP)
			.map(|chunk| std::str::from_utf8(chunk).expect("base32 is ascii"))
			.collect::<Vec<_>>()
			.join("-")
	}

	/// Read a QR code however it was given: the URL a code encodes, the fragment alone, or the
	/// rendering printed beneath the code.
	///
	/// A QR code that parses but carries a version this build does not support is reported as that,
	/// not as an unreadable one (WEB): the forms are tried in turn, and a version complaint from
	/// any of them outranks the failures of the others, which would otherwise bury it. Reading the
	/// three forms in one place is also what keeps every client agreeing on what a QR code is.
	pub fn read(text: &str) -> Result<Self, QrError> {
		let text = text.trim();
		let mut unsupported = None;
		for form in [Self::from_url, Self::from_fragment, Self::from_human] {
			match form(text) {
				Ok(payload) => return Ok(payload),
				Err(QrError::UnsupportedVersion(version)) => unsupported = Some(version),
				Err(QrError::Malformed) => {}
			}
		}
		Err(unsupported.map_or(QrError::Malformed, QrError::UnsupportedVersion))
	}

	/// Read a payload from raw bytes: the version marker followed by the secret.
	///
	/// A version the current build does not support is reported as such, distinctly from a payload
	/// that does not parse at all, so a client can tell "a device at a version I do not support" from
	/// "not a bliti QR code" (WEB).
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, QrError> {
		let (&version, rest) = bytes.split_first().ok_or(QrError::Malformed)?;
		if version != VERSION {
			return Err(QrError::UnsupportedVersion(version));
		}
		let secret: [u8; PRESENCE_TOKEN_LEN] = rest.try_into().map_err(|_| QrError::Malformed)?;
		Ok(Self {
			version,
			secret: PresenceToken::from_bytes(secret),
		})
	}

	/// Read a payload from a fragment, as delivered by following the link.
	///
	/// The fragment carries the same characters as the rendering printed beneath the code, so this is
	/// the same reading: grouping and case are ignored either way.
	pub fn from_fragment(fragment: &str) -> Result<Self, QrError> {
		Self::from_human(fragment.strip_prefix('#').unwrap_or(fragment))
	}

	/// Read a payload from a full QR code URL, taking the payload from its fragment. Both this and
	/// [`from_fragment`](Self::from_fragment) yield the same payload as reading the QR code with a
	/// camera: the URL carries the payload rather than forming part of it.
	pub fn from_url(url: &str) -> Result<Self, QrError> {
		let fragment = url
			.split_once('#')
			.map(|(_, f)| f)
			.ok_or(QrError::Malformed)?;
		Self::from_fragment(fragment)
	}

	/// Read a payload from the human-readable rendering, used when the code itself cannot be scanned.
	/// Grouping and case are ignored.
	pub fn from_human(human: &str) -> Result<Self, QrError> {
		let cleaned: String = human
			.chars()
			.filter(|c| !c.is_whitespace() && *c != '-')
			.flat_map(char::to_uppercase)
			.collect();
		let bytes = BASE32_NOPAD
			.decode(cleaned.as_bytes())
			.map_err(|_| QrError::Malformed)?;
		Self::from_bytes(&bytes)
	}
}

impl core::fmt::Debug for QrPayload {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		// The payload holds a credential; render the version only.
		f.debug_struct("QrPayload")
			.field("version", &self.version)
			.field("secret", &"..")
			.finish()
	}
}

/// A failure reading a QR code payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrError {
	/// The payload does not parse as a bliti QR code.
	#[error("not a valid bliti QR code payload")]
	Malformed,

	/// The payload parses but carries a version this build does not support.
	#[error("QR code carries unsupported version {0}")]
	UnsupportedVersion(u8),
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample() -> QrPayload {
		let mut secret = [0u8; PRESENCE_TOKEN_LEN];
		for (i, b) in secret.iter_mut().enumerate() {
			*b = i as u8;
		}
		QrPayload::new(PresenceToken::from_bytes(secret))
	}

	#[test]
	fn payload_is_version_then_secret_and_nothing_else() {
		let payload = sample();
		let bytes = payload.to_bytes();
		// The board ID is not carried: the payload is exactly the version and the 32-byte secret.
		assert_eq!(bytes.len(), 1 + PRESENCE_TOKEN_LEN);
		assert_eq!(bytes[0], VERSION);
		assert_eq!(&bytes[1..], payload.secret().as_bytes());
	}

	#[test]
	fn url_round_trips_through_the_fragment() {
		let payload = sample();
		let url = payload.to_url();
		assert!(url.starts_with(QR_URL_BASE));
		assert!(url.contains('#'));
		let back = QrPayload::from_url(&url).unwrap();
		assert_eq!(back, payload);
	}

	#[test]
	fn fragment_round_trips_with_or_without_hash() {
		let payload = sample();
		let fragment = payload.to_fragment();
		assert_eq!(QrPayload::from_fragment(&fragment).unwrap(), payload);
		assert_eq!(
			QrPayload::from_fragment(&format!("#{fragment}")).unwrap(),
			payload
		);
	}

	#[test]
	fn human_rendering_round_trips_regardless_of_case_and_grouping() {
		let payload = sample();
		let human = payload.to_human();
		assert!(human.contains('-'));
		assert_eq!(QrPayload::from_human(&human).unwrap(), payload);
		// Usable even if retyped in lower case or with the grouping lost.
		assert_eq!(
			QrPayload::from_human(&human.to_lowercase()).unwrap(),
			payload
		);
		let ungrouped = human.replace('-', "");
		assert_eq!(QrPayload::from_human(&ungrouped).unwrap(), payload);
	}

	#[test]
	fn fragment_and_human_carry_the_same_payload() {
		let payload = sample();
		assert_eq!(
			QrPayload::from_fragment(&payload.to_fragment()).unwrap(),
			QrPayload::from_human(&payload.to_human()).unwrap()
		);
	}

	#[test]
	fn read_takes_every_form_a_code_arrives_in() {
		let payload = sample();
		for form in [
			payload.to_url(),
			payload.to_fragment(),
			payload.to_human(),
			format!("  {}  ", payload.to_human()),
		] {
			assert_eq!(QrPayload::read(&form).unwrap(), payload);
		}
	}

	#[test]
	fn read_reports_an_unsupported_version_rather_than_an_unreadable_code() {
		// A QR code at a version this build does not hold parses as one of the forms and fails the
		// others; the version complaint has to survive that, or a client cannot tell a device it
		// cannot speak to from a code that is not a QR code at all (WEB).
		let mut bytes = sample().to_bytes();
		bytes[0] = 2;
		let fragment = BASE32_NOPAD.encode(&bytes);
		let url = format!("{QR_URL_BASE}#{fragment}");
		for form in [url, fragment] {
			assert_eq!(QrPayload::read(&form), Err(QrError::UnsupportedVersion(2)));
		}
		// And something that is not a QR code at all still says so.
		assert_eq!(QrPayload::read("not a code!"), Err(QrError::Malformed));
	}

	#[test]
	fn unsupported_version_is_reported_distinctly() {
		let mut bytes = sample().to_bytes();
		bytes[0] = 2;
		let fragment = BASE32_NOPAD.encode(&bytes);
		assert_eq!(
			QrPayload::from_fragment(&fragment),
			Err(QrError::UnsupportedVersion(2))
		);
	}

	#[test]
	fn malformed_payloads_are_reported() {
		assert_eq!(
			QrPayload::from_fragment("not valid base64!!!"),
			Err(QrError::Malformed)
		);
		assert_eq!(QrPayload::from_fragment(""), Err(QrError::Malformed));
		// A valid encoding of the wrong length is malformed, not a version error.
		let short = BASE32_NOPAD.encode(&[VERSION, 0, 0]);
		assert_eq!(QrPayload::from_fragment(&short), Err(QrError::Malformed));
		// A URL with no fragment.
		assert_eq!(
			QrPayload::from_url("https://bliti.tamanu.app/"),
			Err(QrError::Malformed)
		);
	}
}
