//! Generating a QR code.
//!
//! Behaviour is specified in QR. The payload for a board is fixed, and no record of what was
//! issued is kept or needed: a damaged QR code is replaced by printing the same payload again.

use std::io::{self, Write};

use bliti_core::qr::QrPayload;
use qrcode::render::unicode;

/// Write the QR code for a payload.
///
/// Drawn for a terminal, the code is followed by the text it encodes. As SVG, the image alone, so
/// it can be redirected straight to a file for a printer.
pub fn write(payload: &QrPayload, svg: bool, out: &mut impl Write) -> io::Result<()> {
	if svg {
		writeln!(out, "{}", payload.to_svg())
	} else {
		let code = payload
			.to_qr_code()
			.render::<unicode::Dense1x2>()
			.quiet_zone(true)
			.build();
		writeln!(out, "{code}")?;
		writeln!(out, "{}", payload.to_text())
	}
}

#[cfg(test)]
mod tests {
	use bliti_core::key_schedule::Root;

	use super::*;

	fn payload(byte: u8) -> QrPayload {
		let keys = Root::from_bytes([byte; 32]).device_keys();
		QrPayload::new(keys.presence_token.clone(), keys.fingerprint())
	}

	fn written(payload: &QrPayload, svg: bool) -> String {
		let mut out = Vec::new();
		write(payload, svg, &mut out).unwrap();
		String::from_utf8(out).unwrap()
	}

	#[test]
	fn svg_output_is_the_image_alone() {
		// Redirecting stdout to a file gives a file a printer can take.
		let code = payload(0x5a);
		assert_eq!(written(&code, true), format!("{}\n", code.to_svg()));
	}

	#[test]
	fn terminal_output_carries_the_code_and_its_text() {
		let code = payload(0x31);
		let out = written(&code, false);
		assert!(out.ends_with(&format!("\n{}\n", code.to_text())));
	}
}
