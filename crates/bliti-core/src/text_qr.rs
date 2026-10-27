//! A QR code for any text, drawn for inlining in a page: the hotspot's (VIEW) and a Tailscale login
//! link's (TSCR) alike.

use qrcode::{EcLevel, QrCode, render::svg, types::QrError};

/// The code for `text` as an SVG image. The dark modules are drawn in `currentColor` over nothing, so
/// the page colours the code and its quiet zone alike, in any theme.
pub fn svg(text: &str) -> Result<String, QrError> {
	let svg = QrCode::with_error_correction_level(text, EcLevel::M)?
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::scan_svg;

	#[test]
	fn the_svg_scans_to_the_text() {
		let link = "https://login.tailscale.com/a/1a2b3c4d5e6f";
		assert_eq!(scan_svg(&svg(link).unwrap()), link);
	}

	#[test]
	fn the_svg_carries_no_xml_declaration() {
		assert!(svg("x").unwrap().trim_start().starts_with("<svg"));
	}
}
