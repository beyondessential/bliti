//! The manifest of the provisioning image, as the medium's system carries it (PMED, "The manifest").

use std::{fs, io, path::Path};

/// What the steps that write the system need of the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
	/// `PI_IMAGE_VERSION`: the version of the `linux-images` release the payload comes from.
	pub version: String,
	/// `PI_IMAGE_SIZE`: the payload's size in bytes.
	pub size: u64,
	/// `PI_IMAGE_SHA256`: the digest of the payload, in lowercase hexadecimal.
	pub sha256: String,
	/// `PAYLOAD_HASH_OFFSET`: where the payload's hash tree begins on the payload partition.
	pub hash_offset: u64,
	/// `PAYLOAD_ROOT_HASH`: the root hash of the payload's hash tree, in lowercase hexadecimal.
	pub root_hash: String,
}

impl Manifest {
	/// Read the manifest at `path`.
	pub fn read(path: &Path) -> Result<Self, String> {
		let text = fs::read_to_string(path).map_err(|err| match err.kind() {
			io::ErrorKind::NotFound => "the provisioning medium carries no manifest".to_owned(),
			_ => format!("cannot read the provisioning medium's manifest: {err}"),
		})?;
		Self::parse(&text)
	}

	/// Read the manifest text, one `NAME=value` line per entry.
	pub fn parse(text: &str) -> Result<Self, String> {
		let get = |name: &str| {
			text.lines()
				.find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
				.map(str::trim)
				.ok_or_else(|| format!("the provisioning medium's manifest has no {name}"))
		};
		let number = |name: &str| {
			get(name)?
				.parse()
				.map_err(|_| format!("the provisioning medium's manifest has a bad {name}"))
		};
		Ok(Self {
			version: get("PI_IMAGE_VERSION")?.to_owned(),
			size: number("PI_IMAGE_SIZE")?,
			sha256: get("PI_IMAGE_SHA256")?.to_ascii_lowercase(),
			hash_offset: number("PAYLOAD_HASH_OFFSET")?,
			root_hash: get("PAYLOAD_ROOT_HASH")?.to_ascii_lowercase(),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const TEXT: &str = "BLITI_COMMIT=abc\nBUILT=2026-10-29T01:49:00Z\nPI_IMAGE_VERSION=v1.2.3\nPI_IMAGE_SIZE=4096\nPI_IMAGE_SHA256=AB12\nPAYLOAD_HASH_OFFSET=4096\nPAYLOAD_ROOT_HASH=cd34\nBOOTLOADER_VERSION=2026-10-29\n";

	#[test]
	fn reads_what_the_steps_need() {
		let manifest = Manifest::parse(TEXT).unwrap();
		assert_eq!(manifest.version, "v1.2.3");
		assert_eq!(manifest.size, 4096);
		assert_eq!(manifest.sha256, "ab12");
		assert_eq!(manifest.hash_offset, 4096);
		assert_eq!(manifest.root_hash, "cd34");
	}

	#[test]
	fn a_missing_or_bad_entry_is_an_error() {
		assert!(Manifest::parse("PI_IMAGE_VERSION=x\n").is_err());
		let bad = TEXT.replace("PI_IMAGE_SIZE=4096", "PI_IMAGE_SIZE=big");
		assert!(Manifest::parse(&bad).unwrap_err().contains("PI_IMAGE_SIZE"));
	}

	#[test]
	fn a_name_is_not_taken_for_a_longer_one() {
		let text = TEXT.replace("PI_IMAGE_SIZE=", "PI_IMAGE_SIZE_EXTRA=");
		assert!(Manifest::parse(&text).is_err());
	}
}
