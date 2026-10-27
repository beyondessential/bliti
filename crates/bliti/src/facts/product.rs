//! Which product the device is, as NFO "Product" has it.
//!
//! The generations share one image, so the software cannot say which hardware it runs on and the
//! hardware has to. A board that carries a HAT EEPROM says so itself; the third generation carries
//! none and is known by what it lacks.

use std::path::Path;

use bliti_core::channel::readings::{Entry, kind};

use super::read_trimmed;

/// The name every generation of the product shares.
const PRODUCT: &str = "Tamanu Iti";

/// The firmware publishes a programmed HAT EEPROM here, and nothing where it is blank or absent.
const HAT: &str = "/proc/device-tree/hat";

/// The TPM's device nodes, as the board ID looks for them.
const TPM_NODES: [&str; 2] = ["/dev/tpmrm0", "/dev/tpm0"];

/// What the board's HAT EEPROM says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hat {
	/// Nothing is published: no EEPROM is fitted, or it holds nothing.
	None,
	/// An EEPROM identifying something other than a Tamanu Iti.
	Other,
	/// An EEPROM identifying a Tamanu Iti of this generation.
	Iti(u32),
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
		Hat::Iti(generation) => Some(Entry::text(
			at,
			"product",
			format!("{PRODUCT} v{generation}"),
		)),
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
fn hat(root: &Path) -> Hat {
	let property = |name: &str| read_trimmed(&root.join(name).to_string_lossy());
	let (Some(vendor), Some(product)) = (property("vendor"), property("product")) else {
		return Hat::None;
	};
	match generation(&vendor, &product) {
		Some(generation) => Hat::Iti(generation),
		None => Hat::Other,
	}
}

/// The generation a HAT EEPROM records, where it identifies a Tamanu Iti.
///
/// Placeholder: what the EEPROM holds is not yet defined, so none identifies one.
fn generation(_vendor: &str, _product: &str) -> Option<u32> {
	None
}

#[cfg(test)]
mod tests {
	use super::*;

	fn product(hat: Hat, tpm: bool, backup: bool) -> Option<Entry> {
		decide(1, Hardware { hat, tpm, backup })
	}

	fn named(entry: Option<Entry>) -> Option<String> {
		entry?.value?.as_str().map(str::to_owned)
	}

	// spec: NFO
	#[test]
	fn an_identifying_eeprom_names_its_generation() {
		for generation in [3, 4, 5] {
			let entry = product(Hat::Iti(generation), true, true);
			assert_eq!(
				named(entry.clone()),
				Some(format!("Tamanu Iti v{generation}"))
			);
			assert_eq!(entry.unwrap().status(), Some("passed"));
		}
	}

	// spec: NFO
	#[test]
	fn an_identifying_eeprom_wins_whatever_else_is_fitted() {
		for (tpm, backup) in [(false, false), (false, true), (true, false), (true, true)] {
			assert_eq!(
				named(product(Hat::Iti(4), tpm, backup)).as_deref(),
				Some("Tamanu Iti v4"),
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

	#[test]
	fn an_unprogrammed_eeprom_publishes_nothing() {
		assert_eq!(hat(Path::new("/nonexistent/bliti/hat")), Hat::None);
	}

	#[test]
	fn a_published_eeprom_that_names_no_tamanu_iti_is_another() {
		let root = std::env::temp_dir().join(format!("bliti-hat-{}", std::process::id()));
		std::fs::create_dir_all(&root).unwrap();
		std::fs::write(root.join("vendor"), b"Someone Else\0").unwrap();
		std::fs::write(root.join("product"), b"Their HAT\0").unwrap();
		assert_eq!(hat(&root), Hat::Other);
		std::fs::remove_dir_all(&root).unwrap();
	}
}
