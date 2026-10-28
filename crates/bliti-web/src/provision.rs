//! What the provisioning client page reads of the protocol: the names a device in provisioning mode
//! advertises, and the revisions and image age a client implements (PRV, PRS).

use bliti_core::{
	provisioning::{self, Name, Suffix},
	version,
};
use wasm_bindgen::prelude::*;

/// What the chooser is filtered on to find a device in provisioning mode: the name up to the suffix,
/// one for each version marker this build implements, highest first (PRV, "The advertised name").
#[wasm_bindgen]
pub fn provisioning_name_prefixes() -> Vec<String> {
	name_prefixes()
}

pub(crate) fn name_prefixes() -> Vec<String> {
	version::markers().map(Name::filter).collect()
}

/// A device in provisioning mode, read from the name it advertises.
#[wasm_bindgen]
pub struct ProvisioningName {
	marker: u8,
	suffix: String,
}

#[wasm_bindgen]
impl ProvisioningName {
	/// The version marker the name carries.
	#[wasm_bindgen(getter)]
	pub fn marker(&self) -> u8 {
		self.marker
	}

	/// Whether this build implements the version marker the name carries.
	#[wasm_bindgen(getter)]
	pub fn supported(&self) -> bool {
		version::implements(self.marker)
	}

	/// What names the board: the suffix of its QR payload, or `FAULT` where its ID could not be
	/// established.
	#[wasm_bindgen(getter)]
	pub fn suffix(&self) -> String {
		self.suffix.clone()
	}
}

/// Read a local name heard over the air, or `undefined` where it is not the name of a device in
/// provisioning mode (PRV, "The advertised name").
#[wasm_bindgen]
pub fn read_provisioning_name(name: &str) -> Option<ProvisioningName> {
	reading(name).map(|(marker, suffix)| ProvisioningName { marker, suffix })
}

pub(crate) fn reading(name: &str) -> Option<(u8, String)> {
	Name::parse(name).map(|name| {
		let suffix = match name.suffix {
			Suffix::Board(suffix) => suffix,
			Suffix::Fault => provisioning::FAULT.to_owned(),
		};
		(name.marker, suffix)
	})
}

/// The lowest and highest revision of the provisioning stream this build implements (PRS, "The
/// revision").
#[wasm_bindgen]
pub fn provisioning_revisions() -> Vec<u32> {
	let revisions = provisioning::CLIENT_REVISIONS;
	vec![*revisions.start(), *revisions.end()]
}

/// How many days before the client's own date an image may have been built and still be provisioned
/// from (PRS, "The image's date").
#[wasm_bindgen]
pub fn provisioning_image_lifetime_days() -> u32 {
	provisioning::IMAGE_LIFETIME_DAYS
}
