//! Generating a QR code: the QR code, and the human-readable rendering printed beneath it.
//!
//! Behaviour is specified in QR. The payload for a board is fixed, and no record of what was
//! issued is kept or needed: a damaged QR code is replaced by printing the same payload again.

use std::io::{self, Write};

use bliti_core::qr::QrPayload;
use qrcode::render::unicode;

/// Write the QR code for a payload, and the rendering printed beneath it.
///
/// Drawn for a terminal, everything goes to `out`. As SVG, `out` gets the image alone, so it can be
/// redirected straight to a file for a printer, and the rendering goes to `err`.
pub fn write(
	payload: &QrPayload,
	svg: bool,
	out: &mut impl Write,
	err: &mut impl Write,
) -> io::Result<()> {
	if svg {
		writeln!(out, "{}", payload.to_svg())?;
		writeln!(err, "{}", payload.to_human())
	} else {
		let code = payload
			.to_qr_code()
			.render::<unicode::Dense1x2>()
			.quiet_zone(true)
			.build();
		writeln!(out, "{code}")?;
		writeln!(out, "{}", payload.to_url())?;
		writeln!(out, "\n{}", payload.to_human())
	}
}

#[cfg(test)]
mod tests {
	use bliti_core::key_schedule::Root;

	use super::*;

	fn payload(byte: u8) -> QrPayload {
		let keys = Root::from_bytes([byte; 32]).device_keys();
		QrPayload::new(keys.presence_token, keys.static_key.public_key())
	}

	fn written(payload: &QrPayload, svg: bool) -> (String, String) {
		let (mut out, mut err) = (Vec::new(), Vec::new());
		write(payload, svg, &mut out, &mut err).unwrap();
		(
			String::from_utf8(out).unwrap(),
			String::from_utf8(err).unwrap(),
		)
	}

	#[test]
	fn svg_output_is_the_image_alone() {
		// Redirecting stdout to a file gives a file a printer can take.
		let code = payload(0x5a);
		let (out, err) = written(&code, true);
		assert_eq!(out, format!("{}\n", code.to_svg()));
		assert_eq!(err, format!("{}\n", code.to_human()));
	}

	#[test]
	fn terminal_output_carries_the_code_the_url_and_the_rendering() {
		let code = payload(0x31);
		let (out, err) = written(&code, false);
		assert!(out.contains(&code.to_url()));
		assert!(out.ends_with(&format!("\n{}\n", code.to_human())));
		assert!(err.is_empty());
	}
}
