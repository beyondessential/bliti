//! Decoding QR codes from camera frames, for a browser with no detector of its own (WEB, "Reading a
//! QR code").
//!
//! The browser's `BarcodeDetector` is on phones and a few desktops only; everywhere else a webcam
//! would be there to read the code with and nothing to decode what it sees.

use wasm_bindgen::prelude::*;

/// The text of every QR code found in a frame, as the RGBA pixels a canvas's `getImageData` gives,
/// `width` by `height`. A code that is found but cannot be decoded, one half out of frame say, is
/// passed over: the next frame is along shortly.
#[wasm_bindgen]
pub fn decode_qr(width: usize, height: usize, rgba: &[u8]) -> Vec<String> {
	if width == 0 || height == 0 || rgba.len() < width * height * 4 {
		return Vec::new();
	}
	let grey: Vec<u8> = rgba[..width * height * 4]
		.chunks_exact(4)
		.map(|pixel| {
			let [r, g, b] = [pixel[0], pixel[1], pixel[2]].map(u32::from);
			((r * 77 + g * 150 + b * 29) >> 8) as u8
		})
		.collect();
	quircs::Quirc::new()
		.identify(width, height, &grey)
		.filter_map(|code| code.ok()?.decode().ok())
		.filter_map(|data| String::from_utf8(data.payload).ok())
		.collect()
}

#[cfg(test)]
mod tests {
	use qrcode::{Color, QrCode};

	use super::*;

	/// A frame with `text` encoded dark-on-light in the middle of it, as a camera held up to a
	/// printed code sees it.
	fn frame(text: &str) -> (usize, usize, Vec<u8>) {
		let code = QrCode::new(text).unwrap();
		let (modules, colours) = (code.width(), code.to_colors());
		let (scale, margin) = (4, 40);
		let side = modules * scale + margin * 2;
		let mut rgba = vec![230; side * side * 4];
		for y in 0..modules * scale {
			for x in 0..modules * scale {
				if colours[(y / scale) * modules + x / scale] == Color::Dark {
					let at = ((y + margin) * side + x + margin) * 4;
					rgba[at..at + 3].fill(20);
				}
			}
		}
		(side, side, rgba)
	}

	#[test]
	fn a_code_in_frame_is_read() {
		let url = "https://bliti.example/#AEAACAQDAQCQMBYIBEFAWDANBYHRAEISCMKBKFQXDAMRUGY4DUPB7";
		let (width, height, rgba) = frame(url);
		assert_eq!(decode_qr(width, height, &rgba), vec![url.to_owned()]);
	}

	#[test]
	fn a_frame_with_no_code_reads_nothing() {
		assert!(decode_qr(64, 48, &[128; 64 * 48 * 4]).is_empty());
	}

	/// A frame not yet sized, as a video that has not started gives, is not an error.
	#[test]
	fn a_frame_short_of_its_size_reads_nothing() {
		assert!(decode_qr(0, 0, &[]).is_empty());
		assert!(decode_qr(64, 48, &[0; 16]).is_empty());
	}
}
