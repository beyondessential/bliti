//! The QR code payload: what the QR code carries, and how it is drawn and read back.
//!
//! Behaviour is specified in `.workhorse/specs/qr-code.md` (QR). The payload carries the payload
//! version, the presence token and the key fingerprint, and nothing else. The board ID is
//! deliberately absent: putting it here would hand the board ID to anyone who photographs a QR code,
//! and with it the device static private key.

use data_encoding::BASE32_NOPAD;
use qrcode::{EcLevel, QrCode, Version, bits::Bits, render::svg};

use crate::{
	key_schedule::{KEY_FINGERPRINT_LEN, KeyFingerprint, PRESENCE_TOKEN_LEN, PresenceToken},
	version::{PAYLOAD_VERSION, reads_payload},
};

/// The text the code carries ahead of the payload. A carrier rather than payload (VER).
pub const PREFIX: &str = "BLITI:";

/// The length of the payload in bytes: the payload version, the presence token, and the key
/// fingerprint.
pub const PAYLOAD_LEN: usize = 1 + PRESENCE_TOKEN_LEN + KEY_FINGERPRINT_LEN;

/// The length of the encoded payload: unpadded base32 of [`PAYLOAD_LEN`] bytes.
pub const ENCODED_LEN: usize = (PAYLOAD_LEN * 8).div_ceil(5);

/// How many characters at the end of the encoded payload name a device.
const SUFFIX_LEN: usize = 4;

const _: () = {
	// A whole number of base32 blocks, so there are no padding bits.
	assert!(PAYLOAD_LEN % 5 == 0);
	// The characters that name a device lie wholly within the key fingerprint.
	assert!((ENCODED_LEN - SUFFIX_LEN) * 5 >= (1 + PRESENCE_TOKEN_LEN) * 8);
};

/// The decoded contents of a QR code: the payload version, the presence token, and the key
/// fingerprint.
#[derive(Clone, PartialEq, Eq)]
pub struct QrPayload {
	version: u8,
	presence_token: PresenceToken,
	fingerprint: KeyFingerprint,
}

impl QrPayload {
	/// A payload at the payload version this build writes.
	pub fn new(presence_token: PresenceToken, fingerprint: KeyFingerprint) -> Self {
		Self {
			version: PAYLOAD_VERSION,
			presence_token,
			fingerprint,
		}
	}

	/// The payload version read from the payload.
	pub fn version(&self) -> u8 {
		self.version
	}

	/// The presence token.
	pub fn presence_token(&self) -> &PresenceToken {
		&self.presence_token
	}

	/// The key fingerprint.
	pub fn fingerprint(&self) -> &KeyFingerprint {
		&self.fingerprint
	}

	/// The raw payload bytes: the payload version, the presence token, then the key fingerprint.
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(PAYLOAD_LEN);
		bytes.push(self.version);
		bytes.extend_from_slice(self.presence_token.as_bytes());
		bytes.extend_from_slice(self.fingerprint.as_bytes());
		bytes
	}

	/// The encoded payload: the raw bytes as unpadded base32.
	///
	/// Base32 rather than anything denser because every character of it, and of the prefix, lies in
	/// the symbology's alphanumeric mode, which packs them tighter than any mode a denser alphabet
	/// would need, and a coarser code is what a phone reads off an enclosure.
	pub fn encoded(&self) -> String {
		BASE32_NOPAD.encode(&self.to_bytes())
	}

	/// The last characters of the encoded payload, which name a device without giving away anything
	/// secret: they lie within the key fingerprint.
	pub fn suffix(&self) -> String {
		let encoded = self.encoded();
		encoded[encoded.len() - SUFFIX_LEN..].to_owned()
	}

	/// The text the QR code encodes: the prefix, then the encoded payload.
	pub fn to_text(&self) -> String {
		format!("{PREFIX}{}", self.encoded())
	}

	/// The QR code itself, encoding [`to_text`](Self::to_text).
	///
	/// One alphanumeric segment at error correction level H, at the smallest version that holds it.
	/// Built from that segment by hand rather than by the encoder's optimiser, which splits base32's
	/// runs of digits differently from one payload to the next: every board's code then comes out
	/// the same size. Every rendering is drawn from this, so the terminal, the printer and the
	/// browser all show the same code.
	pub fn to_qr_code(&self) -> QrCode {
		let text = self.to_text();
		(1..=40)
			.find_map(|version| {
				let mut bits = Bits::new(Version::Normal(version));
				bits.push_alphanumeric_data(text.as_bytes()).ok()?;
				bits.push_terminator(EcLevel::H).ok()?;
				QrCode::with_bits(bits, EcLevel::H).ok()
			})
			.expect("the code text fits a QR code at level H")
	}

	/// The QR code as an SVG image for sending to a printer: the code alone, with its quiet zone.
	pub fn to_svg(&self) -> String {
		self.to_qr_code()
			.render::<svg::Color<'_>>()
			.min_dimensions(256, 256)
			.quiet_zone(true)
			.build()
	}

	/// Read a payload from the text of a QR code, whether captured from the code or typed in by a
	/// person (QR, "Reading").
	///
	/// Everything up to the last `:` is discarded, so the prefix is optional and any other a person or
	/// an application put there is passed over, as are dashes and whitespace. Case is ignored, and
	/// `0`, `1` and `8`, which base32 never uses, are read as the letters they are mistaken for.
	pub fn read(text: &str) -> Result<Self, QrError> {
		let payload = text.rsplit_once(':').map_or(text, |(_, payload)| payload);
		let cleaned: String = payload
			.chars()
			.filter(|c| !c.is_whitespace() && *c != '-')
			.map(|c| match c.to_ascii_uppercase() {
				'0' => 'O',
				'1' => 'I',
				'8' => 'B',
				c => c,
			})
			.collect();
		let bytes = BASE32_NOPAD
			.decode(cleaned.as_bytes())
			.map_err(|_| QrError::Malformed)?;
		Self::from_bytes(&bytes)
	}

	/// Read a payload from raw bytes: the payload version, the presence token, then the key
	/// fingerprint.
	///
	/// The payload version is read before anything else, so a payload at a version this build does
	/// not read is reported as such however that version lays out the rest, distinctly from bytes
	/// that are not a payload at all.
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, QrError> {
		let (&version, rest) = bytes.split_first().ok_or(QrError::Malformed)?;
		if !reads_payload(version) {
			return Err(QrError::UnsupportedVersion(version));
		}
		if rest.len() != PAYLOAD_LEN - 1 {
			return Err(QrError::Malformed);
		}
		let (token, fingerprint) = rest.split_at(PRESENCE_TOKEN_LEN);
		Ok(Self {
			version,
			presence_token: PresenceToken::from_bytes(
				token.try_into().expect("length checked above"),
			),
			fingerprint: KeyFingerprint::from_bytes(
				fingerprint.try_into().expect("length checked above"),
			),
		})
	}
}

impl core::fmt::Debug for QrPayload {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		// The payload holds a credential; the presence token is not rendered.
		f.debug_struct("QrPayload")
			.field("version", &self.version)
			.field("presence_token", &self.presence_token)
			.field("fingerprint", &self.fingerprint)
			.finish()
	}
}

/// A failure reading a QR code payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QrError {
	/// The text does not hold a bliti QR code payload.
	#[error("not a valid bliti QR code payload")]
	Malformed,

	/// The payload is at a payload version this build does not read.
	#[error("QR code carries unsupported payload version {0}")]
	UnsupportedVersion(u8),
}

#[cfg(test)]
mod tests;
