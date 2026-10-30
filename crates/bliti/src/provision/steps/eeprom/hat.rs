//! Describing what a HAT EEPROM holds, from its bytes (PEEP, "What the EEPROM holds").
//!
//! Only the header and the vendor-information atom are read, and the atoms' checksums are not
//! checked: this tells an operator what is on a board and decides whether to ask before writing over
//! it, and the step checks what it writes by reading it back.

use std::fmt;

use winnow::{
	Parser, Result,
	binary::{le_u8, le_u16, le_u32},
	token::{literal, take},
};

use crate::facts::product::{ITI_UUID, Version};

const SIGNATURE: &[u8] = b"R-Pi";
const HAT_PLUS: u8 = 0x02;
const VENDOR_ATOM: u16 = 0x0001;

/// What an EEPROM holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
	/// The image for the board's version.
	Image,
	/// Nothing: every byte reads `0xff`.
	Blank,
	/// HAT+ vendor information with the Tamanu Iti's product UUID, and the version it names.
	Iti(Version),
	/// Vendor information of some other HAT, by its strings.
	Other { vendor: String, product: String },
	/// Anything else.
	Unrecognised,
}

impl fmt::Display for Held {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Image => f.write_str("the image"),
			Self::Blank => f.write_str("nothing, being blank"),
			Self::Iti(version) => write!(f, "a Tamanu Iti v{version}"),
			Self::Other { vendor, product } => {
				write!(f, "another HAT, {product:?} from {vendor:?}")
			}
			Self::Unrecognised => f.write_str("contents that are not a HAT image"),
		}
	}
}

/// Describe `bytes`, which are the EEPROM from its first byte, as the first of the kinds of
/// [`Held`] other than the image that applies.
// spec: PEEP
pub fn describe(bytes: &[u8]) -> Held {
	if !bytes.is_empty() && bytes.iter().all(|byte| *byte == 0xff) {
		return Held::Blank;
	}
	let Some((hat_plus, vendor)) = vendor_information(bytes) else {
		return Held::Unrecognised;
	};
	if hat_plus && vendor.uuid().eq_ignore_ascii_case(ITI_UUID) {
		Held::Iti(Version {
			major: vendor.product_id,
			minor: vendor.product_ver,
		})
	} else {
		Held::Other {
			vendor: printable(vendor.vendor),
			product: printable(vendor.product),
		}
	}
}

struct Header {
	version: u8,
	atoms: u16,
}

struct Vendor<'i> {
	/// As the EEPROM holds them: four little-endian words, the least significant first, which is the
	/// UUID's bytes in reverse.
	uuid: &'i [u8],
	product_id: u16,
	product_ver: u16,
	vendor: &'i [u8],
	product: &'i [u8],
}

impl Vendor<'_> {
	/// The UUID as text, hyphenated.
	fn uuid(&self) -> String {
		let hex: String = self
			.uuid
			.iter()
			.rev()
			.map(|byte| format!("{byte:02x}"))
			.collect();
		format!(
			"{}-{}-{}-{}-{}",
			&hex[..8],
			&hex[8..12],
			&hex[12..16],
			&hex[16..20],
			&hex[20..]
		)
	}
}

/// The vendor-information atom of a HAT image, and whether the image is HAT+.
fn vendor_information(bytes: &[u8]) -> Option<(bool, Vendor<'_>)> {
	let mut input = bytes;
	let header = header.parse_next(&mut input).ok()?;
	for _ in 0..header.atoms {
		let (kind, data) = atom.parse_next(&mut input).ok()?;
		if kind == VENDOR_ATOM {
			let mut data = data;
			let vendor = vendor.parse_next(&mut data).ok()?;
			return Some((header.version == HAT_PLUS, vendor));
		}
	}
	None
}

fn header(input: &mut &[u8]) -> Result<Header> {
	literal(SIGNATURE).parse_next(input)?;
	let version = le_u8.parse_next(input)?;
	let _reserved = le_u8.parse_next(input)?;
	let atoms = le_u16.parse_next(input)?;
	let _length = le_u32.parse_next(input)?;
	Ok(Header { version, atoms })
}

/// An atom's type and its data, which ends in the atom's checksum.
fn atom<'i>(input: &mut &'i [u8]) -> Result<(u16, &'i [u8])> {
	let kind = le_u16.parse_next(input)?;
	let _count = le_u16.parse_next(input)?;
	let length = le_u32.parse_next(input)?;
	let data = take(length).parse_next(input)?;
	Ok((kind, data))
}

fn vendor<'i>(input: &mut &'i [u8]) -> Result<Vendor<'i>> {
	let uuid = take(16usize).parse_next(input)?;
	let product_id = le_u16.parse_next(input)?;
	let product_ver = le_u16.parse_next(input)?;
	let vendor_length = le_u8.parse_next(input)?;
	let product_length = le_u8.parse_next(input)?;
	let vendor = take(vendor_length).parse_next(input)?;
	let product = take(product_length).parse_next(input)?;
	Ok(Vendor {
		uuid,
		product_id,
		product_ver,
		vendor,
		product,
	})
}

/// Text with each character that is not printable ASCII replaced, since a stranger's EEPROM may
/// hold anything and an operator reads what is shown.
// spec: PEEP
fn printable(text: &[u8]) -> String {
	String::from_utf8_lossy(text)
		.chars()
		.map(|c| {
			if c.is_ascii_graphic() || c == ' ' {
				c
			} else {
				'?'
			}
		})
		.collect()
}

#[cfg(test)]
pub mod tests {
	use super::*;

	/// A HAT image laid out as `eepmake` writes it, with its checksums left blank, since nothing here
	/// reads them.
	pub fn image(
		version: u8,
		uuid: &str,
		product_id: u16,
		product_ver: u16,
		vendor: &[u8],
		product: &[u8],
		overlay: &str,
	) -> Vec<u8> {
		let mut stored: Vec<u8> = uuid
			.replace('-', "")
			.as_bytes()
			.chunks(2)
			.map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
			.collect();
		stored.reverse();
		let mut vendor_data = stored;
		vendor_data.extend(product_id.to_le_bytes());
		vendor_data.extend(product_ver.to_le_bytes());
		vendor_data.push(vendor.len().try_into().unwrap());
		vendor_data.push(product.len().try_into().unwrap());
		vendor_data.extend(vendor);
		vendor_data.extend(product);
		vendor_data.extend([0, 0]);

		let mut overlay_data = overlay.as_bytes().to_vec();
		overlay_data.extend([0, 0]);

		let atoms = [(1u16, 1u16, vendor_data), (3, 2, overlay_data)];
		let length = 12 + atoms.iter().map(|(.., data)| 8 + data.len()).sum::<usize>();
		let mut bytes = SIGNATURE.to_vec();
		bytes.push(version);
		bytes.push(0);
		bytes.extend(u16::try_from(atoms.len()).unwrap().to_le_bytes());
		bytes.extend(u32::try_from(length).unwrap().to_le_bytes());
		for (kind, count, data) in atoms {
			bytes.extend(kind.to_le_bytes());
			bytes.extend(count.to_le_bytes());
			bytes.extend(u32::try_from(data.len()).unwrap().to_le_bytes());
			bytes.extend(data);
		}
		bytes
	}

	/// The image the `hat/` definition of a Tamanu Iti v4.0 describes.
	pub fn iti_4_0() -> Vec<u8> {
		let definition = include_str!("../../../../../../hat/tamanu-iti-v4.0.txt");
		let field = |name: &str| {
			definition
				.lines()
				.find_map(|line| line.strip_prefix(name)?.strip_prefix(' '))
				.map(|value| value.trim().trim_matches('"'))
				.unwrap_or_else(|| panic!("no {name} in the image definition"))
		};
		let number = |name: &str| u16::from_str_radix(field(name).trim_start_matches("0x"), 16);
		image(
			HAT_PLUS,
			field("product_uuid"),
			number("product_id").unwrap(),
			number("product_ver").unwrap(),
			field("vendor").as_bytes(),
			field("product").as_bytes(),
			field("dt_blob"),
		)
	}

	fn other(vendor: &str, product: &str) -> Held {
		Held::Other {
			vendor: vendor.to_owned(),
			product: product.to_owned(),
		}
	}

	// spec: PEEP
	#[test]
	fn the_v4_0_image_is_a_tamanu_iti_v4_0() {
		let image = iti_4_0();
		assert_eq!(image.len(), 99, "the size eepmake gives it");
		assert_eq!(describe(&image), Held::Iti(Version { major: 4, minor: 0 }));
	}

	// spec: PEEP
	#[test]
	fn the_version_comes_from_the_product_id_and_version() {
		let image = image(HAT_PLUS, ITI_UUID, 4, 1, b"BES International", b"x", "x");
		assert_eq!(describe(&image), Held::Iti(Version { major: 4, minor: 1 }));
	}

	// spec: PEEP
	#[test]
	fn another_uuid_is_another_hat_whatever_its_strings_say() {
		let image = image(
			HAT_PLUS,
			"00000000-0000-0000-0000-000000000001",
			4,
			0,
			b"BES International",
			b"Tamanu Iti v4.0",
			"x",
		);
		assert_eq!(
			describe(&image),
			other("BES International", "Tamanu Iti v4.0")
		);
	}

	/// A header from before HAT+ is not what a Tamanu Iti carries, whatever UUID it holds.
	// spec: PEEP
	#[test]
	fn a_legacy_header_with_the_uuid_is_another_hat() {
		let image = image(0x01, ITI_UUID, 4, 0, b"Old", b"Board", "x");
		assert_eq!(describe(&image), other("Old", "Board"));
	}

	// spec: PEEP
	#[test]
	fn every_byte_ff_is_blank() {
		assert_eq!(describe(&[0xff; 100]), Held::Blank);
		assert_eq!(describe(&vec![0xff; 32 * 1024]), Held::Blank);
	}

	// spec: PEEP
	#[test]
	fn anything_else_is_unrecognised() {
		let mut almost_blank = vec![0xff; 100];
		almost_blank[50] = 0;
		let mut wrong_signature = iti_4_0();
		wrong_signature[0] = b'X';
		let mut no_vendor = iti_4_0();
		no_vendor[12] = 0x09;
		for bytes in [
			Vec::new(),
			vec![0; 100],
			almost_blank,
			wrong_signature,
			no_vendor,
			b"R-Pi".to_vec(),
			iti_4_0()[..40].to_vec(),
			iti_4_0()[..11].to_vec(),
		] {
			assert_eq!(describe(&bytes), Held::Unrecognised, "{bytes:02x?}");
		}
	}

	/// Atoms are counted by the header, and the walk stops at the last of them.
	#[test]
	fn a_header_that_claims_no_atoms_holds_no_vendor() {
		let mut image = iti_4_0();
		image[6] = 0;
		image[7] = 0;
		assert_eq!(describe(&image), Held::Unrecognised);
	}

	// spec: PEEP
	#[test]
	fn characters_that_are_not_printable_ascii_are_replaced() {
		let image = image(
			HAT_PLUS,
			"00000000-0000-0000-0000-000000000001",
			1,
			0,
			b"Ac\x1b[31mme\n",
			"Bo\u{fc}rd \u{1f600}".as_bytes(),
			"x",
		);
		assert_eq!(describe(&image), other("Ac?[31mme?", "Bo?rd ?"));

		let image = self::image(
			HAT_PLUS,
			"00000000-0000-0000-0000-000000000001",
			1,
			0,
			&[b'A', 0xff, 0xfe, b'B'],
			b"P",
			"x",
		);
		assert_eq!(describe(&image), other("A??B", "P"));
	}

	#[test]
	fn what_is_held_is_worded_for_an_operator() {
		assert_eq!(
			Held::Iti(Version { major: 4, minor: 0 }).to_string(),
			"a Tamanu Iti v4.0"
		);
		assert_eq!(
			other("Acme", "Widget").to_string(),
			"another HAT, \"Widget\" from \"Acme\""
		);
	}
}
