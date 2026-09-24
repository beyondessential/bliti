//! The QR code payload: what the QR code carries, and how it is rendered and read back.
//!
//! Behaviour is specified in `.workhorse/specs/qr-code.md` (QR). The payload carries the version
//! marker, the presence token and the device static public key, and nothing else. The board ID is
//! deliberately absent: putting it here would hand the board ID to anyone who photographs a QR code,
//! and with it the device static private key.

use data_encoding::BASE32_NOPAD;
use qrcode::{EcLevel, QrCode, render::svg};

use crate::key_schedule::{
	DEVICE_KEY_LEN, DevicePublicKey, PRESENCE_TOKEN_LEN, PresenceToken, VERSION,
};

/// The URL the QR code encodes. The payload rides in the fragment, which a browser never sends to a
/// server, so the presence token stays on the device that scanned it. A generic phone camera opens this
/// page; a native application can claim the link.
///
/// Kept lower case. The scheme and host are case insensitive to a browser, and upper casing them
/// would shrink the code further, but a native application claims a link by matching the scheme and
/// host literally, so the saving would cost the property above.
pub const QR_URL_BASE: &str = "https://bliti.tamanu.app/";

/// The number of characters per group in the human-readable rendering.
const HUMAN_GROUP: usize = 4;

/// The length of the payload in bytes: the version marker, the presence token, and the device static
/// public key.
pub const PAYLOAD_LEN: usize = 1 + PRESENCE_TOKEN_LEN + DEVICE_KEY_LEN;

/// The decoded contents of a QR code: the version marker, the presence token, and the device static
/// public key.
#[derive(Clone, PartialEq, Eq)]
pub struct QrPayload {
	version: u8,
	presence_token: PresenceToken,
	device_public_key: DevicePublicKey,
}

impl QrPayload {
	/// A payload for the current version.
	pub fn new(presence_token: PresenceToken, device_public_key: DevicePublicKey) -> Self {
		Self {
			version: VERSION,
			presence_token,
			device_public_key,
		}
	}

	/// The version marker read from the payload.
	pub fn version(&self) -> u8 {
		self.version
	}

	/// The presence token.
	pub fn presence_token(&self) -> &PresenceToken {
		&self.presence_token
	}

	/// The device static public key.
	pub fn device_public_key(&self) -> &DevicePublicKey {
		&self.device_public_key
	}

	/// The raw payload bytes: the version marker, the presence token, then the device static public
	/// key.
	pub fn to_bytes(&self) -> Vec<u8> {
		let mut bytes = Vec::with_capacity(PAYLOAD_LEN);
		bytes.push(self.version);
		bytes.extend_from_slice(self.presence_token.as_bytes());
		bytes.extend_from_slice(self.device_public_key.as_bytes());
		bytes
	}

	/// The fragment the payload rides in: the raw bytes as unpadded base32, the same characters the
	/// human-readable rendering carries, ungrouped.
	///
	/// Base32 rather than base64url because of what it costs to print. A QR code encodes digits and
	/// upper-case letters at five and a half bits a character, and anything else at eight, so the
	/// longer base32 rendering occupies fewer bits than the shorter mixed-case one and the printed
	/// code comes out measurably coarser. A QR code is read off an
	/// enclosure by a phone, so that is worth more than a shorter URL.
	pub fn to_fragment(&self) -> String {
		BASE32_NOPAD.encode(&self.to_bytes())
	}

	/// The full URL the QR code encodes.
	pub fn to_url(&self) -> String {
		format!("{QR_URL_BASE}#{}", self.to_fragment())
	}

	/// The QR code itself, encoding [`to_url`](Self::to_url).
	///
	/// At error correction level H, the most tolerant of damage, because a code on an enclosure gets
	/// scuffed. Every rendering is drawn from this, so the terminal, the printer and the browser all
	/// show the same code.
	pub fn to_qr_code(&self) -> QrCode {
		QrCode::with_error_correction_level(self.to_url(), EcLevel::H)
			.expect("a 130-byte URL fits a QR code at level H")
	}

	/// The QR code as an SVG image for sending to a printer: the code alone, with its quiet zone.
	pub fn to_svg(&self) -> String {
		self.to_qr_code()
			.render::<svg::Color<'_>>()
			.min_dimensions(256, 256)
			.quiet_zone(true)
			.build()
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

	/// Read a payload from raw bytes: the version marker, the presence token, then the device static
	/// public key.
	///
	/// A version the current build does not support is reported as such, distinctly from a payload
	/// that does not parse at all, so a client can tell "a device at a version I do not support" from
	/// "not a bliti QR code" (WEB).
	pub fn from_bytes(bytes: &[u8]) -> Result<Self, QrError> {
		let (&version, rest) = bytes.split_first().ok_or(QrError::Malformed)?;
		if version != VERSION {
			return Err(QrError::UnsupportedVersion(version));
		}
		if rest.len() != PAYLOAD_LEN - 1 {
			return Err(QrError::Malformed);
		}
		let (token, public_key) = rest.split_at(PRESENCE_TOKEN_LEN);
		Ok(Self {
			version,
			presence_token: PresenceToken::from_bytes(
				token.try_into().expect("length checked above"),
			),
			device_public_key: DevicePublicKey::from_bytes(
				public_key.try_into().expect("length checked above"),
			),
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
		// The payload holds a credential; the presence token is not rendered.
		f.debug_struct("QrPayload")
			.field("version", &self.version)
			.field("presence_token", &self.presence_token)
			.field("device_public_key", &self.device_public_key)
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
		let mut token = [0u8; PRESENCE_TOKEN_LEN];
		for (i, b) in token.iter_mut().enumerate() {
			*b = i as u8;
		}
		let mut public_key = [0u8; DEVICE_KEY_LEN];
		for (i, b) in public_key.iter_mut().enumerate() {
			*b = 0x80 | i as u8;
		}
		QrPayload::new(
			PresenceToken::from_bytes(token),
			DevicePublicKey::from_bytes(public_key),
		)
	}

	#[test]
	fn payload_is_version_then_token_then_public_key_and_nothing_else() {
		let payload = sample();
		let bytes = payload.to_bytes();
		// The board ID is not carried: the payload is exactly these 65 bytes.
		assert_eq!(bytes.len(), 65);
		assert_eq!(bytes[0], VERSION);
		assert_eq!(&bytes[1..33], payload.presence_token().as_bytes());
		assert_eq!(&bytes[33..], payload.device_public_key().as_bytes());
		assert_eq!(QrPayload::from_bytes(&bytes).unwrap(), payload);
	}

	#[test]
	fn fragment_is_104_characters_without_padding() {
		let fragment = sample().to_fragment();
		assert_eq!(fragment.len(), 104);
		assert!(!fragment.contains('='));
		assert!(
			fragment
				.chars()
				.all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
		);
	}

	#[test]
	fn fragment_known_answer() {
		// Pins the field order and the encoding end to end.
		assert_eq!(
			sample().to_fragment(),
			"AEAACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB7AEBQKBYJBMGQ6EITCULRSGY5D4QSGJJHFEVS2LZRGM2TOOJ3HU7"
		);
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

	/// Scan an SVG image as a camera would: rasterise the dark path and decode whatever code is found.
	fn scan_svg(svg: &str) -> String {
		let attr = |name: &str| -> usize {
			let start = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
			let len = svg[start..].find('"').unwrap();
			svg[start..start + len].parse().unwrap()
		};
		let (width, height) = (attr("width"), attr("height"));
		let mut dark = vec![false; width * height];
		// The dark path is a run of rectangles, each `M{left} {top}h{width}v{height}H{left}V{top}`.
		let path = svg.rsplit(" d=\"").next().unwrap();
		let path = &path[..path.find('"').unwrap()];
		for rect in path.split('M').filter(|rect| !rect.is_empty()) {
			let numbers: Vec<usize> = rect
				.split(|c: char| !c.is_ascii_digit())
				.filter(|n| !n.is_empty())
				.map(|n| n.parse().unwrap())
				.collect();
			let [left, top, w, h, ..] = numbers[..] else {
				panic!("not a rectangle: {rect}");
			};
			for y in top..top + h {
				dark[y * width + left..y * width + left + w].fill(true);
			}
		}
		let mut image = rqrr::PreparedImage::prepare_from_greyscale(width, height, |x, y| {
			if dark[y * width + x] { 0 } else { 255 }
		});
		let grids = image.detect_grids();
		assert_eq!(grids.len(), 1, "exactly one code in the image");
		grids[0].decode().unwrap().1
	}

	#[test]
	fn the_svg_scans_to_the_url() {
		let payload = sample();
		let svg = payload.to_svg();
		assert!(svg.contains("<svg"));
		assert_eq!(scan_svg(&svg), payload.to_url());
		assert_eq!(QrPayload::read(&scan_svg(&svg)).unwrap(), payload);
	}

	#[test]
	fn the_svg_carries_the_code_alone() {
		// The rendering is printed separately, not drawn into the image (QR).
		let payload = sample();
		let svg = payload.to_svg();
		assert!(!svg.contains("<text"));
		assert!(!svg.contains(&payload.to_fragment()));
		assert!(!svg.contains(&payload.to_human()));
	}

	#[test]
	fn the_code_is_at_level_h() {
		assert_eq!(sample().to_qr_code().error_correction_level(), EcLevel::H);
	}

	#[test]
	fn a_payload_produces_the_same_svg_every_time() {
		// A reprint is byte-identical with no record consulted, wherever it is produced.
		assert_eq!(sample().to_svg(), sample().to_svg());
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
		// Including a payload carrying the token alone, without the public key.
		let token_only = &sample().to_bytes()[..1 + PRESENCE_TOKEN_LEN];
		assert_eq!(QrPayload::from_bytes(token_only), Err(QrError::Malformed));
		let mut long = sample().to_bytes();
		long.push(0);
		assert_eq!(QrPayload::from_bytes(&long), Err(QrError::Malformed));
		// A URL with no fragment.
		assert_eq!(
			QrPayload::from_url("https://bliti.tamanu.app/"),
			Err(QrError::Malformed)
		);
	}
}
