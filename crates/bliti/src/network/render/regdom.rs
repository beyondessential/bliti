//! The regulatory domain the radio operates under (NET).

use bliti_core::channel::config::{Document, Invalid, Segment};

use super::{File, PUBLIC, Paths, header, invalid};

/// The kernel's world domain: what every domain permits, which is where an unset device stays (NET).
const WORLD: &str = "00";

/// The document's regulatory domain, where it sets one.
pub(super) fn domain(document: &Document) -> Result<Option<&str>, Invalid> {
	match document.regulatory_domain.as_deref() {
		None => Ok(None),
		Some(code) if code.len() == 2 && code.bytes().all(|b| b.is_ascii_uppercase()) => {
			Ok(Some(code))
		}
		Some(code) => Err(invalid(
			&[Segment::Name("regulatory-domain")],
			format!("{code:?} is not an ISO 3166-1 alpha-2 code such as \"NZ\""),
		)),
	}
}

/// The domain cfg80211 starts under when the module loads, so the radio is in it from boot.
pub(super) fn modprobe(paths: &Paths, domain: Option<&str>) -> File {
	File {
		path: paths.modprobe.clone(),
		contents: format!(
			"{}options cfg80211 ieee80211_regdom={}\n",
			header("the document"),
			domain.unwrap_or(WORLD)
		),
		mode: PUBLIC,
	}
}
