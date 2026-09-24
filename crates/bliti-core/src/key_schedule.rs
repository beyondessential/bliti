//! The key schedule: one memory-hard derivation from a board ID to a root, and the cheap ones that
//! descend from it to the presence token, the device static key, and the advertised handle.
//!
//! Behaviour is specified in `.workhorse/specs/key-schedule.md` (KEY). Every constant and context
//! string here is public: they are compiled into the device, the QR code generator, and every
//! client, and publishing them weakens nothing because no derivation runs backwards. What they
//! provide is domain separation.
//!
//! Everything a QR code depends on is versioned together under [`VERSION`]. The constants, the
//! argon2id parameters, the source precedence and the encoding below, the pinned Endorsement Key
//! template, and the handle length are all covered by it; any of them changing is a new version,
//! because any of them changing changes what a QR code carries.

use curve25519_dalek::montgomery::MontgomeryPoint;

use crate::board_id::SourceKind;

/// The version marker carried in the QR payload (QR) and in the advertisement (ADV).
///
/// It covers everything a QR code depends on. A change that leaves the QR payload identical does not
/// move it, because moving it orphans every QR code already fixed to an enclosure.
pub const VERSION: u8 = 1;

/// The fixed argon2id salt for the root derivation. Public and versioned. Only the memory-hard
/// derivation uses it, so it is gated with that.
#[cfg(feature = "derive")]
const ROOT_SALT: [u8; 16] = [
	0x3e, 0xdf, 0xe9, 0x5c, 0xeb, 0x86, 0xfa, 0xdd, 0x23, 0xd4, 0x6a, 0x87, 0x34, 0xc7, 0xeb, 0x13,
];

/// The key derivation context for the presence token. Public and versioned.
const PRESENCE_TOKEN_CONTEXT: &str = "bliti presence token";

/// The key derivation context for the device static private key. Public and versioned.
const DEVICE_STATIC_KEY_CONTEXT: &str = "bliti device static key";

/// The fixed domain-separation constant for the handle derivation. Public and versioned.
const HANDLE_CONSTANT: [u8; 16] = [
	0x15, 0x9f, 0x0a, 0x92, 0x9c, 0x9d, 0x0b, 0x80, 0x41, 0x7e, 0x9b, 0x87, 0x75, 0xbb, 0x18, 0x39,
];

/// The argon2id memory parameter, in kibibytes: 2 GiB. Part of the derivation, not a tuning choice.
pub const ROOT_MEMORY_KIB: u32 = 2 * 1024 * 1024;

/// The argon2id memory parameter in bytes, for a device to check against its free memory before
/// beginning (KEY, "Deriving on the device").
pub const ROOT_MEMORY_BYTES: u64 = (ROOT_MEMORY_KIB as u64) * 1024;

/// The argon2id pass count. Part of the derivation.
pub const ROOT_PASSES: u32 = 1;

/// The argon2id lane count. Part of the derivation. Whether the lanes are computed concurrently or
/// in sequence does not change the result.
pub const ROOT_LANES: u32 = 2;

/// The length of a root in bytes.
pub const ROOT_LEN: usize = 32;

/// The length of a presence token in bytes.
pub const PRESENCE_TOKEN_LEN: usize = 32;

/// The length of a device static key, private or public, in bytes.
pub const DEVICE_KEY_LEN: usize = 32;

/// The length of an advertised handle in bytes: eight, which makes a collision between two devices
/// at one site implausible and fits the advertising budget in ADV.
pub const HANDLE_LEN: usize = 8;

/// The length of the rotation salt in bytes.
pub const ROTATION_SALT_LEN: usize = 4;

/// A root: the output of the memory-hard derivation, from which the presence token and the device
/// static key descend (KEY, "The root"). It never leaves the device that derived it, other than into
/// the QR code generator's own derivation of the same board.
#[derive(Clone, PartialEq, Eq)]
pub struct Root([u8; ROOT_LEN]);

impl Root {
	/// Wrap raw bytes as a root, as read back from a device's cache.
	pub fn from_bytes(bytes: [u8; ROOT_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the root.
	pub fn as_bytes(&self) -> &[u8; ROOT_LEN] {
		&self.0
	}

	/// The presence token (KEY, "Presence token").
	pub fn presence_token(&self) -> PresenceToken {
		PresenceToken(blake3::derive_key(PRESENCE_TOKEN_CONTEXT, &self.0))
	}

	/// The device static key (KEY, "Device static key"): the key derivation of the root, clamped as
	/// RFC 7748 requires.
	pub fn device_static_key(&self) -> DeviceStaticKey {
		let mut key = blake3::derive_key(DEVICE_STATIC_KEY_CONTEXT, &self.0);
		key[0] &= 0b1111_1000;
		key[31] &= 0b0111_1111;
		key[31] |= 0b0100_0000;
		DeviceStaticKey(key)
	}

	/// Both credentials a device holds, derived together.
	pub fn device_keys(&self) -> DeviceKeys {
		DeviceKeys {
			presence_token: self.presence_token(),
			static_key: self.device_static_key(),
		}
	}
}

impl core::fmt::Debug for Root {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("Root(..)")
	}
}

/// What a device holds to be reached: the presence token a client proves it read, and the static key
/// the device proves it holds (CHN, "Authentication").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKeys {
	/// The presence token.
	pub presence_token: PresenceToken,
	/// The device static key.
	pub static_key: DeviceStaticKey,
}

/// The device static private key, clamped. Held only by the device.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceStaticKey([u8; DEVICE_KEY_LEN]);

impl DeviceStaticKey {
	/// The raw bytes of the private key.
	pub fn as_bytes(&self) -> &[u8; DEVICE_KEY_LEN] {
		&self.0
	}

	/// The X25519 public key for this private key, as carried in the QR code.
	pub fn public_key(&self) -> DevicePublicKey {
		DevicePublicKey(MontgomeryPoint::mul_base_clamped(self.0).to_bytes())
	}
}

impl core::fmt::Debug for DeviceStaticKey {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("DeviceStaticKey(..)")
	}
}

/// The device static public key, carried in the QR code so a client can authenticate the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevicePublicKey([u8; DEVICE_KEY_LEN]);

impl DevicePublicKey {
	/// Wrap raw bytes as a device public key, as read from a QR payload by a client.
	pub fn from_bytes(bytes: [u8; DEVICE_KEY_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the public key.
	pub fn as_bytes(&self) -> &[u8; DEVICE_KEY_LEN] {
		&self.0
	}
}

/// A presence token: the value printed in the QR code, which a client proves it holds.
#[derive(Clone, PartialEq, Eq)]
pub struct PresenceToken([u8; PRESENCE_TOKEN_LEN]);

impl PresenceToken {
	/// Wrap raw bytes as a presence token, as read from a QR payload by a client.
	pub fn from_bytes(bytes: [u8; PRESENCE_TOKEN_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the token.
	pub fn as_bytes(&self) -> &[u8; PRESENCE_TOKEN_LEN] {
		&self.0
	}

	/// Derive the advertised handle for a given rotation salt (KEY, "Advertised handle").
	///
	/// This is a fast keyed hash, deliberately cheap: a client recomputes it for every advertisement
	/// it hears against every QR code it holds, so a memory-hard function here would be felt during
	/// scanning. It runs in the browser, where the memory-hard derivation never does.
	pub fn handle(&self, salt: RotationSalt) -> Handle {
		let mut data = [0u8; HANDLE_CONSTANT.len() + ROTATION_SALT_LEN];
		data[..HANDLE_CONSTANT.len()].copy_from_slice(&HANDLE_CONSTANT);
		data[HANDLE_CONSTANT.len()..].copy_from_slice(&salt.0);
		let digest = blake3::keyed_hash(&self.0, &data);
		let mut handle = [0u8; HANDLE_LEN];
		handle.copy_from_slice(&digest.as_bytes()[..HANDLE_LEN]);
		Handle(handle)
	}
}

impl core::fmt::Debug for PresenceToken {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		// A presence token is a credential; never render it in a debug log.
		f.write_str("PresenceToken(..)")
	}
}

/// An advertised handle: the eight-byte value a device broadcasts and a client recomputes to match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle([u8; HANDLE_LEN]);

impl Handle {
	/// Wrap raw bytes as a handle, as read from an advertisement by a client.
	pub fn from_bytes(bytes: [u8; HANDLE_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the handle.
	pub fn as_bytes(&self) -> &[u8; HANDLE_LEN] {
		&self.0
	}
}

/// The rotation salt: a short random value advertised in the clear that changes every fifteen
/// minutes (ADV, "Rotation"), so a passive observer cannot follow a device by its handle alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RotationSalt([u8; ROTATION_SALT_LEN]);

impl RotationSalt {
	/// Wrap raw bytes as a rotation salt, as observed in an advertisement.
	pub fn from_bytes(bytes: [u8; ROTATION_SALT_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the salt.
	pub fn as_bytes(&self) -> &[u8; ROTATION_SALT_LEN] {
		&self.0
	}
}

/// The argon2id password for a board ID: a source tag byte followed by the raw bytes of the source
/// value, most significant first (KEY, "The root").
///
/// The tag identifies which kind of source the value came from, so a value byte-identical across two
/// kinds of source still derives differently. Lengths are fixed per kind of source, so the tag
/// leaves the input unambiguous without a length prefix.
pub fn argon2_password(kind: SourceKind, raw: &[u8]) -> Vec<u8> {
	let mut password = Vec::with_capacity(1 + raw.len());
	password.push(kind.tag());
	password.extend_from_slice(raw);
	password
}

/// Whether a device has room to run the root derivation, given the bytes of memory it has
/// available. The derivation needs its full memory parameter at once and is killed by the operating
/// system rather than told the allocation failed, so a device checks this before beginning (KEY,
/// "Deriving on the device").
pub fn check_memory(available_bytes: u64) -> Result<(), KeyError> {
	if available_bytes < ROOT_MEMORY_BYTES {
		return Err(KeyError::InsufficientMemory {
			required: ROOT_MEMORY_BYTES,
			available: available_bytes,
		});
	}
	Ok(())
}

/// Derive the root from a board ID with argon2id under the fixed salt (KEY, "The root").
///
/// This is the memory-hard derivation. It runs on the device and in the QR code generator, never in
/// a client, and is behind the `derive` feature so a wasm build does not pull argon2. A device
/// checks [`check_memory`] before calling this, because the allocation cannot fail gracefully.
#[cfg(feature = "derive")]
pub fn derive_root(board_id: &crate::board_id::BoardId) -> Result<Root, KeyError> {
	let password = argon2_password(board_id.kind(), board_id.raw());
	derive_root_with(ROOT_MEMORY_KIB, ROOT_PASSES, ROOT_LANES, &password)
}

/// Run argon2id over a password with the fixed salt and given parameters. Split out so a
/// known-answer test can pin the wiring, the salt, and the password encoding cheaply with small
/// parameters, while the production parameters are pinned separately by their constants.
#[cfg(feature = "derive")]
fn derive_root_with(
	memory_kib: u32,
	passes: u32,
	lanes: u32,
	password: &[u8],
) -> Result<Root, KeyError> {
	use argon2::{Algorithm, Argon2, Params, Version};

	let params = Params::new(memory_kib, passes, lanes, Some(ROOT_LEN))
		.map_err(|err| KeyError::Parameters(err.to_string()))?;
	let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
	let mut out = [0u8; ROOT_LEN];
	argon2
		.hash_password_into(password, &ROOT_SALT, &mut out)
		.map_err(|err| KeyError::Derivation(err.to_string()))?;
	Ok(Root(out))
}

/// A failure in the key schedule.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
	/// A device does not have room for the memory-hard derivation.
	#[error(
		"insufficient memory for the root derivation: needs {required} bytes, {available} available"
	)]
	InsufficientMemory {
		/// Bytes the derivation needs at once.
		required: u64,
		/// Bytes the device has available.
		available: u64,
	},

	/// The argon2id parameters were rejected.
	#[error("invalid argon2 parameters: {0}")]
	Parameters(String),

	/// The argon2id derivation failed.
	#[error("root derivation failed: {0}")]
	Derivation(String),
}

#[cfg(test)]
mod tests {
	use super::*;

	fn hex(bytes: &[u8]) -> String {
		bytes.iter().map(|b| format!("{b:02x}")).collect()
	}

	#[test]
	fn production_parameters_are_pinned() {
		// A guard that always runs: an accidental edit to the memory-hard parameters, which are
		// versioned into every QR code, fails here without allocating 2 GiB.
		assert_eq!(ROOT_MEMORY_KIB, 2 * 1024 * 1024);
		assert_eq!(ROOT_MEMORY_BYTES, 2 * 1024 * 1024 * 1024);
		assert_eq!(ROOT_PASSES, 1);
		assert_eq!(ROOT_LANES, 2);
		assert_eq!(ROOT_LEN, 32);
		assert_eq!(PRESENCE_TOKEN_LEN, 32);
		assert_eq!(DEVICE_KEY_LEN, 32);
		assert_eq!(HANDLE_LEN, 8);
		assert_eq!(VERSION, 1);
	}

	#[test]
	fn password_is_tag_then_raw_bytes() {
		let password = argon2_password(SourceKind::RaspberryPiSerial, &[0xf3, 0x75]);
		assert_eq!(
			password,
			vec![SourceKind::RaspberryPiSerial.tag(), 0xf3, 0x75]
		);
	}

	#[test]
	fn deriving_from_characters_differs_from_the_bytes_they_denote() {
		// The raw bytes are used, never a text rendering: a serial read as characters gives a
		// different password from the bytes those characters denote.
		let from_chars = argon2_password(SourceKind::RaspberryPiSerial, b"f375");
		let from_bytes = argon2_password(SourceKind::RaspberryPiSerial, &[0xf3, 0x75]);
		assert_ne!(from_chars, from_bytes);
	}

	#[test]
	fn identical_values_from_different_sources_derive_differently() {
		let value = [0x11, 0x22, 0x33, 0x44];
		let a = argon2_password(SourceKind::OneTimeProgrammable, &value);
		let b = argon2_password(SourceKind::RaspberryPiSerial, &value);
		assert_ne!(a, b);
	}

	#[test]
	fn check_memory_reports_insufficient() {
		assert!(matches!(
			check_memory(ROOT_MEMORY_BYTES - 1),
			Err(KeyError::InsufficientMemory { .. })
		));
		assert!(check_memory(ROOT_MEMORY_BYTES).is_ok());
	}

	#[test]
	fn handle_known_answer() {
		// Pins the handle derivation: constant, keying, salt handling, and eight-byte truncation.
		let token = PresenceToken::from_bytes([0x42; PRESENCE_TOKEN_LEN]);
		let salt = RotationSalt::from_bytes([0x01, 0x02, 0x03, 0x04]);
		let handle = token.handle(salt);
		assert_eq!(hex(handle.as_bytes()), "5a22650058575721");
	}

	#[test]
	fn handle_changes_with_the_salt() {
		let token = PresenceToken::from_bytes([0x42; PRESENCE_TOKEN_LEN]);
		let a = token.handle(RotationSalt::from_bytes([0, 0, 0, 0]));
		let b = token.handle(RotationSalt::from_bytes([0, 0, 0, 1]));
		assert_ne!(a, b);
	}

	/// The root the canonical board of `root_production_known_answer` derives. The cheap derivations
	/// below are pinned from it, so they run on every test pass without the 2 GiB derivation.
	const CANONICAL_ROOT: &str = "cb89bf939b867ec6e15530a6b92db98a14f170e1f4c9ff218cbd460e2140ccbd";

	fn canonical_root() -> Root {
		let mut bytes = [0u8; ROOT_LEN];
		for (i, byte) in bytes.iter_mut().enumerate() {
			*byte = u8::from_str_radix(&CANONICAL_ROOT[2 * i..2 * i + 2], 16).unwrap();
		}
		Root::from_bytes(bytes)
	}

	#[test]
	fn presence_token_known_answer() {
		assert_eq!(
			hex(canonical_root().presence_token().as_bytes()),
			"e4602ffee2a5aecac332443a8474650161980709aafdf8e766bae48c81e1e1ef"
		);
	}

	#[test]
	fn device_static_key_known_answer() {
		// Pins the context string, the clamping, and the X25519 public key.
		let key = canonical_root().device_static_key();
		assert_eq!(
			hex(key.as_bytes()),
			"201f75c2f3241873c2ecd0ed15ffb73acc0c93347460039203352fa75ae5db75"
		);
		assert_eq!(
			hex(key.public_key().as_bytes()),
			"5b89128820363945de1431e0e33cae6281a1fb410422a8ba77ebd9b766358f19"
		);
	}

	#[test]
	fn device_static_key_is_clamped() {
		for byte in [0x00, 0x42, 0xff] {
			let key = Root::from_bytes([byte; ROOT_LEN]).device_static_key();
			let key = key.as_bytes();
			assert_eq!(key[0] & 0b0000_0111, 0);
			assert_eq!(key[31] & 0b1100_0000, 0b0100_0000);
		}
	}

	#[test]
	fn token_and_static_key_are_separated() {
		// The two context strings are what keep one from being the other.
		let root = canonical_root();
		assert_ne!(
			root.presence_token().as_bytes(),
			root.device_static_key().as_bytes()
		);
		assert_ne!(root.presence_token().as_bytes(), root.as_bytes());
	}

	#[cfg(feature = "derive")]
	#[test]
	fn root_wiring_known_answer() {
		// A cheap known-answer test with small memory: pins the algorithm, version, salt constant,
		// password encoding, and output length. The memory-hard magnitude is pinned separately by
		// `production_parameters_are_pinned`, so together they cover the whole derivation.
		let password = argon2_password(SourceKind::RaspberryPiSerial, &[0xf3, 0x75, 0x65, 0x10]);
		let root = derive_root_with(32, 1, 2, &password).unwrap();
		assert_eq!(
			hex(root.as_bytes()),
			"6f1389914fdb010c7ed6f41278bf0dd2ee97f0fd692e8bae7c3f581c06ef610c"
		);
	}

	/// The canonical version 1 vector. Measured at 2.2 s on a Raspberry Pi 5, the slowest board in
	/// scope, with the lanes computed concurrently, and 3.1 s with them in sequence.
	///
	/// This one value is what pins the whole memory-hard derivation, and it holds across every way of
	/// computing it: it is identical on x86-64 and aarch64, and identical whether or not the
	/// `parallel` feature is on. Running this test under each of those settings is what verifies that
	/// the device, the QR code generator, and any future implementation agree on a board's root
	/// regardless of the machine they run on or how each chooses to compute it.
	#[cfg(feature = "derive")]
	#[test]
	#[ignore = "allocates 2 GiB and runs the full derivation; run explicitly with --ignored"]
	fn root_production_known_answer() {
		use crate::board_id::BoardId;
		let board_id = BoardId::new(
			SourceKind::RaspberryPiSerial,
			vec![0xf3, 0x75, 0x65, 0x10, 0xf6, 0x32, 0xcf, 0xad],
		)
		.unwrap();
		let root = derive_root(&board_id).unwrap();
		assert_eq!(hex(root.as_bytes()), CANONICAL_ROOT);
	}
}
