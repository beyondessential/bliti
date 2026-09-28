//! The QR code a phone joins a device's hotspot by (VIEW, "Joining the hotspot").
//!
//! It carries the ZXing form of the Wi-Fi network URI, which is the one phone cameras parse: Android
//! writes it when sharing a network and reads it back. The WPA3 Specification's percent-encoded
//! form of the same URI is not what they were written against.

use qrcode::{EcLevel, QrCode, render::svg, types::QrError};

/// The Wi-Fi network URI for a hotspot. `T:WPA` covers the WPA2/WPA3 transitional mode every
/// hotspot runs in, and leaves which of the two to the phone.
pub fn uri(ssid: &str, passphrase: &str) -> String {
	format!("WIFI:T:WPA;S:{};P:{};;", escape(ssid), escape(passphrase))
}

/// The code as an SVG image for inlining in a page. The dark modules are drawn in `currentColor`
/// over nothing, so the page colours the code and its quiet zone alike, in any theme.
pub fn svg(ssid: &str, passphrase: &str) -> Result<String, QrError> {
	let svg = QrCode::with_error_correction_level(uri(ssid, passphrase), EcLevel::M)?
		.render::<svg::Color<'_>>()
		.dark_color(svg::Color("currentColor"))
		.light_color(svg::Color("none"))
		.min_dimensions(256, 256)
		.quiet_zone(true)
		.build();
	// The XML declaration belongs to a file, and is noise inside a document.
	Ok(match svg.split_once("?>") {
		Some((_, rest)) => rest.to_owned(),
		None => svg,
	})
}

/// Backslash-escape what the URI gives meaning to. Nothing is quoted: Android keeps quotes as part of
/// the value, so a quoted SSID would name a different network.
fn escape(value: &str) -> String {
	let mut escaped = String::with_capacity(value.len());
	for c in value.chars() {
		if matches!(c, '\\' | ';' | ',' | '"' | ':') {
			escaped.push('\\');
		}
		escaped.push(c);
	}
	escaped
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::scan_svg;

	#[test]
	fn a_plain_hotspot_is_its_ssid_and_passphrase() {
		assert_eq!(
			uri("iti-setup", "harbour-kettle-47"),
			"WIFI:T:WPA;S:iti-setup;P:harbour-kettle-47;;"
		);
	}

	#[test]
	fn every_character_the_uri_gives_meaning_to_is_escaped() {
		assert_eq!(
			uri(r#"a;b,c"d:e\f"#, r#"p;q,r"s:t\u"#),
			r#"WIFI:T:WPA;S:a\;b\,c\"d\:e\\f;P:p\;q\,r\"s\:t\\u;;"#
		);
	}

	#[test]
	fn a_value_that_looks_like_hexadecimal_is_not_quoted() {
		assert_eq!(
			uri("cafe", "0123456789"),
			"WIFI:T:WPA;S:cafe;P:0123456789;;"
		);
	}

	#[test]
	fn the_svg_scans_to_the_uri() {
		let (ssid, passphrase) = ("clinic; ward 3", r#"read "me" aloud\"#);
		assert_eq!(
			scan_svg(&svg(ssid, passphrase).unwrap()),
			uri(ssid, passphrase)
		);
	}

	#[test]
	fn the_svg_takes_its_colours_from_the_page() {
		let svg = svg("iti-setup", "harbour-kettle-47").unwrap();
		assert!(svg.starts_with("<svg"), "{svg}");
		assert!(svg.contains(r#"fill="currentColor""#));
		assert!(svg.contains(r#"fill="none""#));
		assert!(!svg.contains("#fff") && !svg.contains("#000"));
	}

	#[test]
	fn the_svg_carries_neither_the_ssid_nor_the_passphrase_as_text() {
		let svg = svg("iti-setup", "harbour-kettle-47").unwrap();
		assert!(!svg.contains("iti-setup") && !svg.contains("harbour-kettle-47"));
	}
}
