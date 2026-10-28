//! Which product the device is, as NFO "Product" has it.
//!
//! The generations share one image, so the software cannot say which hardware it runs on and the
//! hardware has to. A board that carries a HAT EEPROM says so itself, as HAT has it; the third
//! generation carries none and is known by what it lacks.

use std::{fmt, path::Path};

use bliti_core::channel::readings::{Entry, kind};

use super::read_trimmed;

/// The name every generation of the product shares.
const PRODUCT: &str = "Tamanu Iti";

/// The firmware publishes a programmed HAT EEPROM here, and nothing where it is blank or absent.
const HAT: &str = "/proc/device-tree/hat";

/// The product UUID of the Tamanu Iti, which every one of its HAT EEPROMs carries (HAT).
const ITI_UUID: &str = "7c3b8a88-ad89-4d48-b709-2bd445729867";

/// The TPM's device nodes, as the board ID looks for them.
const TPM_NODES: [&str; 2] = ["/dev/tpmrm0", "/dev/tpm0"];

/// What the board's HAT EEPROM says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hat {
	/// Nothing is published: no EEPROM is fitted, or it holds nothing.
	None,
	/// An EEPROM identifying something other than a Tamanu Iti.
	Other,
	/// An EEPROM identifying a Tamanu Iti of this version.
	Iti(Version),
}

/// A Tamanu Iti's version, which `product_id` and `product_ver` of its EEPROM carry (HAT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Version {
	major: u16,
	minor: u16,
}

impl fmt::Display for Version {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}.{}", self.major, self.minor)
	}
}

/// What the device can see of its hardware.
#[derive(Debug, Clone, Copy)]
struct Hardware {
	hat: Hat,
	tpm: bool,
	/// Whether a backup supply is fitted, which every generation carries.
	backup: bool,
}

/// The `product` fact, from what the hardware shows. `backup` is whether a backup supply is fitted,
/// which only the supply's own look at its gauge can say without disturbing it.
pub fn entry(at: u64, backup: bool) -> Option<Entry> {
	decide(
		at,
		Hardware {
			hat: hat(Path::new(HAT)),
			tpm: TPM_NODES.iter().any(|node| Path::new(node).exists()),
			backup,
		},
	)
}

fn decide(at: u64, hardware: Hardware) -> Option<Entry> {
	let Hardware { hat, tpm, backup } = hardware;
	match hat {
		Hat::Iti(version) => Some(Entry::text(at, "product", format!("{PRODUCT} v{version}"))),
		_ if !backup => None,
		_ if tpm => Some(Entry::broken(
			at,
			"product",
			kind::TEXT,
			"this board carries a TPM but no HAT EEPROM identifying it as a Tamanu Iti",
		)),
		Hat::None => Some(Entry::text(at, "product", format!("{PRODUCT} v3"))),
		Hat::Other => None,
	}
}

/// What the firmware published of the HAT EEPROM under `root`.
///
/// Only the product UUID and the version are read; the vendor and product strings are for people
/// (HAT).
fn hat(root: &Path) -> Hat {
	let property = |name: &str| read_trimmed(&root.join(name).to_string_lossy());
	let Some(uuid) = property("uuid") else {
		return Hat::None;
	};
	if !uuid.eq_ignore_ascii_case(ITI_UUID) {
		return Hat::Other;
	}
	let version = property("product_id")
		.zip(property("product_ver"))
		.and_then(|(major, minor)| {
			Some(Version {
				major: hex(&major)?,
				minor: hex(&minor)?,
			})
		});
	version.map_or(Hat::Other, Hat::Iti)
}

/// A 16-bit value as the firmware publishes it: `0x` and hexadecimal digits.
fn hex(text: &str) -> Option<u16> {
	let digits = text.strip_prefix("0x")?;
	if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
		return None;
	}
	u16::from_str_radix(digits, 16).ok()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn product(hat: Hat, tpm: bool, backup: bool) -> Option<Entry> {
		decide(1, Hardware { hat, tpm, backup })
	}

	const fn iti(major: u16, minor: u16) -> Hat {
		Hat::Iti(Version { major, minor })
	}

	fn named(entry: Option<Entry>) -> Option<String> {
		entry?.value?.as_str().map(str::to_owned)
	}

	// spec: NFO
	#[test]
	fn an_identifying_eeprom_names_its_version() {
		for (major, minor) in [(4, 0), (4, 1), (5, 0)] {
			let entry = product(iti(major, minor), true, true);
			assert_eq!(
				named(entry.clone()),
				Some(format!("Tamanu Iti v{major}.{minor}"))
			);
			assert_eq!(entry.unwrap().status(), Some("passed"));
		}
	}

	// spec: NFO
	#[test]
	fn an_identifying_eeprom_wins_whatever_else_is_fitted() {
		for (tpm, backup) in [(false, false), (false, true), (true, false), (true, true)] {
			assert_eq!(
				named(product(iti(4, 0), tpm, backup)).as_deref(),
				Some("Tamanu Iti v4.0"),
				"tpm {tpm}, backup {backup}"
			);
		}
	}

	// spec: NFO
	#[test]
	fn a_tpm_and_a_backup_supply_without_an_identifying_eeprom_is_broken() {
		for hat in [Hat::None, Hat::Other] {
			let entry = product(hat, true, true).expect("an entry");
			assert_eq!(entry.status(), Some("broken"), "{hat:?}");
			assert!(entry.value.is_none());
			assert!(entry.reason().unwrap().contains("HAT EEPROM"));
		}
	}

	// spec: NFO
	#[test]
	fn a_backup_supply_with_neither_a_tpm_nor_an_eeprom_is_the_third_generation() {
		assert_eq!(
			named(product(Hat::None, false, true)).as_deref(),
			Some("Tamanu Iti v3")
		);
	}

	// spec: NFO
	#[test]
	fn another_eeprom_without_a_tpm_is_no_product() {
		assert!(product(Hat::Other, false, true).is_none());
	}

	// spec: NFO
	#[test]
	fn no_backup_supply_is_no_product() {
		for hat in [Hat::None, Hat::Other] {
			for tpm in [false, true] {
				assert!(product(hat, tpm, false).is_none(), "{hat:?}, tpm {tpm}");
			}
		}
	}

	/// A scratch tree standing in for `/proc/device-tree/hat`, each property written as the
	/// firmware does, with its trailing NUL.
	struct Published(std::path::PathBuf);

	impl Published {
		fn new(name: &str, properties: &[(&str, &str)]) -> Self {
			let root =
				std::env::temp_dir().join(format!("bliti-hat-{name}-{}", std::process::id()));
			std::fs::create_dir_all(&root).unwrap();
			for (property, text) in properties {
				std::fs::write(root.join(property), format!("{text}\0")).unwrap();
			}
			Self(root)
		}

		fn read(&self) -> Hat {
			hat(&self.0)
		}
	}

	impl Drop for Published {
		fn drop(&mut self) {
			std::fs::remove_dir_all(&self.0).ok();
		}
	}

	const ITI: [(&str, &str); 5] = [
		("uuid", ITI_UUID),
		("product_id", "0x0004"),
		("product_ver", "0x0000"),
		("vendor", "BES International"),
		("product", "Tamanu Iti v4.0"),
	];

	#[test]
	fn an_unprogrammed_eeprom_publishes_nothing() {
		assert_eq!(hat(Path::new("/nonexistent/bliti/hat")), Hat::None);
	}

	// spec: HAT
	#[test]
	fn the_iti_product_uuid_identifies_a_tamanu_iti_and_its_version() {
		assert_eq!(Published::new("iti", &ITI).read(), iti(4, 0));

		let mut rev_c = ITI;
		rev_c[2].1 = "0x0001";
		assert_eq!(Published::new("rev-c", &rev_c).read(), iti(4, 1));
	}

	// spec: HAT
	#[test]
	fn the_uuid_is_matched_whatever_its_case() {
		let mut upper = ITI;
		let shouting = ITI_UUID.to_uppercase();
		upper[0].1 = &shouting;
		assert_eq!(Published::new("upper", &upper).read(), iti(4, 0));
	}

	// spec: HAT
	#[test]
	fn another_uuid_is_another_eeprom_whatever_its_strings_and_version_say() {
		let theirs = [
			("uuid", "00000000-0000-0000-0000-000000000001"),
			("product_id", "0x0004"),
			("product_ver", "0x0000"),
			("vendor", "BES International"),
			("product", "Tamanu Iti v4.0"),
		];
		assert_eq!(Published::new("other", &theirs).read(), Hat::Other);
	}

	// spec: HAT
	#[test]
	fn the_strings_do_not_identify_a_tamanu_iti() {
		let mut renamed = ITI;
		renamed[3].1 = "Someone Else";
		renamed[4].1 = "Their HAT";
		assert_eq!(Published::new("renamed", &renamed).read(), iti(4, 0));
	}

	// spec: HAT
	#[test]
	fn a_version_the_firmware_did_not_publish_as_hex_does_not_identify() {
		let wrong = [
			"", "4", "0x", "0xZZ", "0x+4", "0x10000", "0x 4", "-0x4", "04",
		];
		for text in wrong {
			for slot in [1, 2] {
				let mut board = ITI;
				board[slot].1 = text;
				assert_eq!(
					Published::new("wrong", &board).read(),
					Hat::Other,
					"{} {text:?}",
					board[slot].0
				);
			}
		}
	}

	// spec: HAT
	#[test]
	fn a_missing_version_does_not_identify() {
		for missing in ["product_id", "product_ver"] {
			let board: Vec<_> = ITI
				.into_iter()
				.filter(|(name, _)| *name != missing)
				.collect();
			assert_eq!(
				Published::new("missing", &board).read(),
				Hat::Other,
				"{missing}"
			);
		}
	}

	// spec: HAT
	#[test]
	fn the_rev_b_image_definition_identifies_a_tamanu_iti_v4_0() {
		let image = include_str!("../../../../hat/tamanu-iti-v4.0.txt");
		let field = |name: &str| {
			image
				.lines()
				.find_map(|line| line.strip_prefix(name)?.strip_prefix(' '))
				.map(|value| value.trim().trim_matches('"'))
				.unwrap_or_else(|| panic!("no {name} in the image definition"))
		};
		let properties = [
			("uuid", field("product_uuid")),
			("product_id", field("product_id")),
			("product_ver", field("product_ver")),
			("vendor", field("vendor")),
			("product", field("product")),
		];
		assert_eq!(field("vendor"), "BES International");
		assert_eq!(field("product"), "Tamanu Iti v4.0");
		assert_eq!(field("dt_blob"), "tamanu-iti-v4");
		assert_eq!(Published::new("rev-b", &properties).read(), iti(4, 0));
	}

	#[test]
	fn the_largest_sixteen_bit_version_is_read() {
		assert_eq!(hex("0xffff"), Some(u16::MAX));
		assert_eq!(hex("0xFFFF"), Some(u16::MAX));
		assert_eq!(hex("0x0004"), Some(4));
	}
}
