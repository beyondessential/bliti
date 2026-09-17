//! Generating a QR code: the QR code, and the human-readable rendering printed beneath it.
//!
//! Behaviour is specified in QR. The payload for a board is fixed, and no record of what was
//! issued is kept or needed: a damaged QR code is replaced by printing the same payload again.

use bliti_core::qr::QrPayload;
use qrcode::{EcLevel, QrCode, render::unicode};

/// A QR code ready to print.
pub struct Printable {
	/// The URL the QR code encodes.
	pub url: String,
	/// The human-readable rendering printed beneath the code.
	pub human: String,
	/// The QR code itself.
	code: QrCode,
}

impl Printable {
	/// Build a QR code for a payload.
	///
	/// The error correction is high, because a QR code on an enclosure gets scuffed and a code that
	/// still scans after damage is the difference between reprinting and not.
	pub fn new(payload: &QrPayload) -> Result<Self, QrError> {
		let url = payload.to_url();
		let code = QrCode::with_error_correction_level(&url, EcLevel::H)
			.map_err(|err| QrError::Encode(err.to_string()))?;
		Ok(Self {
			url,
			human: payload.to_human(),
			code,
		})
	}

	/// The code rendered for a terminal, for generating a QR code with the board to hand.
	pub fn to_terminal(&self) -> String {
		self.code
			.render::<unicode::Dense1x2>()
			.quiet_zone(true)
			.build()
	}

	/// The code as an SVG, for sending to a printer.
	pub fn to_svg(&self) -> String {
		self.code
			.render::<qrcode::render::svg::Color<'_>>()
			.min_dimensions(256, 256)
			.quiet_zone(true)
			.build()
	}
}

/// A failure generating a QR code.
#[derive(Debug, thiserror::Error)]
pub enum QrError {
	/// The payload could not be encoded as a QR code.
	#[error("encoding the QR code: {0}")]
	Encode(String),
}

#[cfg(test)]
mod tests {
	use bliti_core::key_schedule::PresenceToken;

	use super::*;

	fn payload(byte: u8) -> QrPayload {
		QrPayload::new(PresenceToken::from_bytes([byte; 32]))
	}

	#[test]
	fn a_board_produces_the_same_code_every_time() {
		// The payload for a board is fixed, so a reprint is byte-identical with no record consulted.
		let a = Printable::new(&payload(0x5a)).unwrap();
		let b = Printable::new(&payload(0x5a)).unwrap();
		assert_eq!(a.url, b.url);
		assert_eq!(a.human, b.human);
		assert_eq!(a.to_svg(), b.to_svg());
	}

	#[test]
	fn the_code_and_the_rendering_carry_the_same_payload() {
		let original = payload(0x31);
		let code = Printable::new(&original).unwrap();
		// Scanning the code yields the payload, and so does reading the rendering beneath it.
		assert_eq!(QrPayload::from_url(&code.url).unwrap(), original);
		assert_eq!(QrPayload::from_human(&code.human).unwrap(), original);
	}

	#[test]
	fn renderings_are_produced() {
		let code = Printable::new(&payload(0x01)).unwrap();
		assert!(code.to_svg().contains("<svg"));
		assert!(!code.to_terminal().is_empty());
	}
}
