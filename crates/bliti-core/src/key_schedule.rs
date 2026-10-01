//! The key schedule: one memory-hard derivation from a board ID to a root, and the cheap ones that
//! descend from it to the presence token, the pre-shared key, the device keys and their
//! fingerprint, and the advertised handle.
//!
//! Behaviour is specified in `.workhorse/specs/key-schedule.md` (KEY). Every constant and context
//! string here is public: they are compiled into the device, the QR code generator, and every
//! client, and publishing them weakens nothing because no derivation runs backwards. What they
//! provide is domain separation.
//!
//! Everything here is covered by the payload version of VER ([`crate::version::PAYLOAD_VERSION`]):
//! the constants, the argon2id parameters, the source precedence and the encoding below, the pinned
//! Endorsement Key template, and every length. Any of them changing is a new payload version,
//! because any of them changing changes what a QR code carries.

use curve25519_dalek::montgomery::MontgomeryPoint;
use ml_kem::{KeyExport, MlKem768, Seed};

use crate::board_id::SourceKind;

/// The fixed argon2id salt for the root derivation. Public and versioned. Only the memory-hard
/// derivation uses it, so it is gated with that.
#[cfg(feature = "derive")]
const ROOT_SALT: [u8; 16] = [
	0x3e, 0xdf, 0xe9, 0x5c, 0xeb, 0x86, 0xfa, 0xdd, 0x23, 0xd4, 0x6a, 0x87, 0x34, 0xc7, 0xeb, 0x13,
];

const PRESENCE_TOKEN_CONTEXT: &str = "bliti presence token";
const PRE_SHARED_KEY_CONTEXT: &str = "bliti pre-shared key";
const DEVICE_STATIC_KEY_CONTEXT: &str = "bliti device static key";
const DEVICE_KEM_SEED_CONTEXT: &str = "bliti device kem seed";
const KEM_KEY_DIGEST_CONTEXT: &str = "bliti device kem key digest";
const KEY_FINGERPRINT_CONTEXT: &str = "bliti device key fingerprint";
const HANDLE_CONTEXT: &str = "bliti advertised handle";

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

/// The length of a presence token in bytes: sixteen, which is beyond guessing online or offline.
pub const PRESENCE_TOKEN_LEN: usize = 16;

/// The length of the pre-shared key in bytes, which the handshake of CHN takes.
pub const PRE_SHARED_KEY_LEN: usize = 32;

/// The length of a device static key, private or public, in bytes.
pub const DEVICE_KEY_LEN: usize = 32;

/// The length of a KEM key digest in bytes.
pub const KEM_KEY_DIGEST_LEN: usize = 32;

/// The length of a key fingerprint in bytes: eighteen, which puts a key matching a given fingerprint
/// out of reach and makes the QR payload a whole number of base32 groups.
pub const KEY_FINGERPRINT_LEN: usize = 18;

/// The length of an advertised handle in bytes: eight, which makes a collision between two devices
/// at one site implausible and fits the advertising budget in ADV.
pub const HANDLE_LEN: usize = 8;

/// A root: the output of the memory-hard derivation, from which everything else in the key schedule
/// descends (KEY, "The root"). It never leaves the device that derived it, other than into the QR
/// code generator's own derivation of the same board.
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

	/// The presence token (KEY, "Presence token"): the first sixteen bytes of the root's derivation.
	pub fn presence_token(&self) -> PresenceToken {
		let derived = blake3::derive_key(PRESENCE_TOKEN_CONTEXT, &self.0);
		PresenceToken(prefix(&derived))
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

	/// The device KEM key (KEY, "Device KEM key"): the ML-KEM-768 key pair FIPS 203 generates from 64
	/// bytes of the root's derivation, read through BLAKE3's extendable output.
	pub fn device_kem_key(&self) -> DeviceKemKey {
		let mut seed = Seed::default();
		blake3::Hasher::new_derive_key(DEVICE_KEM_SEED_CONTEXT)
			.update(&self.0)
			.finalize_xof()
			.fill(&mut seed);
		DeviceKemKey::from_seed(seed)
	}

	/// Everything a device holds to be reached, derived together.
	pub fn device_keys(&self) -> DeviceKeys {
		let presence_token = self.presence_token();
		DeviceKeys {
			pre_shared_key: presence_token.pre_shared_key(),
			presence_token,
			static_key: self.device_static_key(),
			kem_key_digest: self.device_kem_key().digest(),
		}
	}
}

impl core::fmt::Debug for Root {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("Root(..)")
	}
}

/// The first `N` bytes of a 32-byte derivation.
fn prefix<const N: usize>(derived: &[u8; 32]) -> [u8; N] {
	derived[..N].try_into().expect("N is at most 32")
}

/// What a device holds to be reached: the presence token a client proves it read, by way of the
/// pre-shared key, and the keys the device proves it holds (CHN, "Authentication").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceKeys {
	/// The presence token, which the device advertises a handle for.
	pub presence_token: PresenceToken,
	/// The pre-shared key of the handshake.
	pub pre_shared_key: PreSharedKey,
	/// The device static key.
	pub static_key: DeviceStaticKey,
	/// The digest of the device KEM key, which the device sends in the handshake.
	pub kem_key_digest: KemKeyDigest,
}

impl DeviceKeys {
	/// The key fingerprint these keys carry in the device's QR code.
	pub fn fingerprint(&self) -> KeyFingerprint {
		KeyFingerprint::of(&self.static_key.public_key(), &self.kem_key_digest)
	}
}

/// The device static private key, clamped. Held only by the device.
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceStaticKey([u8; DEVICE_KEY_LEN]);

impl DeviceStaticKey {
	/// The raw bytes of the private key.
	pub fn as_bytes(&self) -> &[u8; DEVICE_KEY_LEN] {
		&self.0
	}

	/// The X25519 public key for this private key, which the device sends in the handshake.
	pub fn public_key(&self) -> DevicePublicKey {
		DevicePublicKey(MontgomeryPoint::mul_base_clamped(self.0).to_bytes())
	}
}

impl core::fmt::Debug for DeviceStaticKey {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("DeviceStaticKey(..)")
	}
}

/// The device static public key, which a client receives in the handshake and checks against the
/// key fingerprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevicePublicKey([u8; DEVICE_KEY_LEN]);

impl DevicePublicKey {
	/// Wrap raw bytes as a device public key, as received in the handshake.
	pub fn from_bytes(bytes: [u8; DEVICE_KEY_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the public key.
	pub fn as_bytes(&self) -> &[u8; DEVICE_KEY_LEN] {
		&self.0
	}
}

/// The device KEM key: an ML-KEM-768 key pair. Nothing is encapsulated to it yet; the key
/// fingerprint commits to it so that a later handshake can authenticate the device by it.
pub struct DeviceKemKey(ml_kem::DecapsulationKey<MlKem768>);

impl DeviceKemKey {
	/// The key pair FIPS 203 generates from a 64-byte seed `d ‖ z`.
	pub fn from_seed(seed: Seed) -> Self {
		Self(ml_kem::DecapsulationKey::from_seed(seed))
	}

	/// The encapsulation key, the public half, in its FIPS 203 encoding.
	pub fn encapsulation_key(&self) -> Vec<u8> {
		self.0.encapsulation_key().to_bytes().to_vec()
	}

	/// The KEM key digest (KEY, "Device KEM key").
	pub fn digest(&self) -> KemKeyDigest {
		KemKeyDigest(blake3::derive_key(
			KEM_KEY_DIGEST_CONTEXT,
			&self.encapsulation_key(),
		))
	}
}

impl core::fmt::Debug for DeviceKemKey {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("DeviceKemKey(..)")
	}
}

/// The digest of the device KEM key's encapsulation key, which the device sends in the handshake
/// and the key fingerprint commits to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KemKeyDigest([u8; KEM_KEY_DIGEST_LEN]);

impl KemKeyDigest {
	/// Wrap raw bytes as a KEM key digest, as received in the handshake.
	pub fn from_bytes(bytes: [u8; KEM_KEY_DIGEST_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the digest.
	pub fn as_bytes(&self) -> &[u8; KEM_KEY_DIGEST_LEN] {
		&self.0
	}
}

/// The key fingerprint: the digest of a device's keys carried in its QR code, against which a client
/// authenticates the device (KEY, "Key fingerprint").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyFingerprint([u8; KEY_FINGERPRINT_LEN]);

impl KeyFingerprint {
	/// The fingerprint of a device static public key and a KEM key digest.
	pub fn of(public_key: &DevicePublicKey, kem_key_digest: &KemKeyDigest) -> Self {
		let mut material = [0u8; DEVICE_KEY_LEN + KEM_KEY_DIGEST_LEN];
		material[..DEVICE_KEY_LEN].copy_from_slice(public_key.as_bytes());
		material[DEVICE_KEY_LEN..].copy_from_slice(kem_key_digest.as_bytes());
		Self(prefix(&blake3::derive_key(
			KEY_FINGERPRINT_CONTEXT,
			&material,
		)))
	}

	/// Wrap raw bytes as a key fingerprint, as read from a QR payload by a client.
	pub fn from_bytes(bytes: [u8; KEY_FINGERPRINT_LEN]) -> Self {
		Self(bytes)
	}

	/// The raw bytes of the fingerprint.
	pub fn as_bytes(&self) -> &[u8; KEY_FINGERPRINT_LEN] {
		&self.0
	}
}

/// A presence token: the credential printed in the QR code, which a client proves it holds.
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

	/// The pre-shared key of the handshake (KEY, "Pre-shared key").
	pub fn pre_shared_key(&self) -> PreSharedKey {
		PreSharedKey(blake3::derive_key(PRE_SHARED_KEY_CONTEXT, &self.0))
	}

	/// Derive the advertised handle (KEY, "Advertised handle"), fixed for the life of the token.
	///
	/// Deliberately cheap: a client computes it for every QR code it holds. It runs in the browser,
	/// where the memory-hard derivation never does.
	pub fn handle(&self) -> Handle {
		Handle(prefix(&blake3::derive_key(HANDLE_CONTEXT, &self.0)))
	}
}

impl core::fmt::Debug for PresenceToken {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		// A presence token is a credential; never render it in a debug log.
		f.write_str("PresenceToken(..)")
	}
}

/// The pre-shared key of the handshake, derived from the presence token.
#[derive(Clone, PartialEq, Eq)]
pub struct PreSharedKey([u8; PRE_SHARED_KEY_LEN]);

impl PreSharedKey {
	/// The raw bytes of the key.
	pub fn as_bytes(&self) -> &[u8; PRE_SHARED_KEY_LEN] {
		&self.0
	}
}

impl core::fmt::Debug for PreSharedKey {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.write_str("PreSharedKey(..)")
	}
}

/// An advertised handle: the eight-byte value a device broadcasts and a client computes to match.
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
mod tests;
