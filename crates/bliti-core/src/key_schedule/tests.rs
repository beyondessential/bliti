use super::*;

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Vec<u8> {
	(0..text.len())
		.step_by(2)
		.map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
		.collect()
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
	assert_eq!(PRESENCE_TOKEN_LEN, 16);
	assert_eq!(PRE_SHARED_KEY_LEN, 32);
	assert_eq!(DEVICE_KEY_LEN, 32);
	assert_eq!(KEM_KEY_DIGEST_LEN, 32);
	assert_eq!(KEY_FINGERPRINT_LEN, 18);
	assert_eq!(HANDLE_LEN, 8);
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
	// Pins the handle derivation: context string and eight-byte truncation.
	let token = PresenceToken::from_bytes([0x42; PRESENCE_TOKEN_LEN]);
	assert_eq!(hex(token.handle().as_bytes()), "5ef415ebca67352c");
}

#[test]
fn the_handle_and_the_pre_shared_key_of_a_token_differ() {
	// Two context strings, so neither says anything about the other.
	let token = Root::from_bytes([0x42; ROOT_LEN]).presence_token();
	let psk = token.pre_shared_key();
	assert_ne!(&psk.as_bytes()[..HANDLE_LEN], token.handle().as_bytes());
	assert_ne!(&psk.as_bytes()[..PRESENCE_TOKEN_LEN], token.as_bytes());
}

/// The root the canonical board of `root_production_known_answer` derives. The cheap derivations
/// below are pinned from it, so they run on every test pass without the 2 GiB derivation.
const CANONICAL_ROOT: &str = "cb89bf939b867ec6e15530a6b92db98a14f170e1f4c9ff218cbd460e2140ccbd";

fn canonical_root() -> Root {
	Root::from_bytes(unhex(CANONICAL_ROOT).try_into().unwrap())
}

#[test]
fn the_whole_chain_from_the_canonical_root_is_pinned() {
	// Every derivation a QR code depends on, from the root to the text the code encodes. Checked
	// against an independent implementation of BLAKE3 and of FIPS 203 when first pinned.
	let root = canonical_root();
	let token = root.presence_token();
	assert_eq!(hex(token.as_bytes()), "e4602ffee2a5aecac332443a84746501");
	assert_eq!(
		hex(token.pre_shared_key().as_bytes()),
		"c194fb3661e35efe6f8f56242a03c61e08725d6469aeb523dd69eb20f2e5dd6d"
	);
	assert_eq!(hex(token.handle().as_bytes()), "cdc57e31a225aea2");

	let static_key = root.device_static_key();
	assert_eq!(
		hex(static_key.as_bytes()),
		"201f75c2f3241873c2ecd0ed15ffb73acc0c93347460039203352fa75ae5db75"
	);
	assert_eq!(
		hex(static_key.public_key().as_bytes()),
		"5b89128820363945de1431e0e33cae6281a1fb410422a8ba77ebd9b766358f19"
	);

	let kem_key = root.device_kem_key();
	let encapsulation_key = kem_key.encapsulation_key();
	assert_eq!(encapsulation_key.len(), 1184);
	assert_eq!(
		hex(&encapsulation_key[..32]),
		"cb13cb39b3964ed33ff43b85e9d191f41214c8d8ccc31cbf9ba8603a452dceda"
	);
	assert_eq!(
		hex(kem_key.digest().as_bytes()),
		"afe3fb9e3f43b87a948298964ba71b9b7676c085be9569b9bd7e052359f03434"
	);

	let keys = root.device_keys();
	assert_eq!(keys.presence_token, token);
	assert_eq!(keys.pre_shared_key, token.pre_shared_key());
	assert_eq!(keys.kem_key_digest, kem_key.digest());
	assert_eq!(
		hex(keys.fingerprint().as_bytes()),
		"11e99becf80f72007416b82a69d975442c19"
	);

	let code = crate::qr::QrPayload::new(token, keys.fingerprint());
	assert_eq!(
		code.to_text(),
		"BLITI:AHSGAL764KS25SWDGJCDVBDUMUARD2M35T4A64QAOQLLQKTJ3F2UILAZ"
	);
}

#[test]
fn ml_kem_768_key_generation_matches_the_acvp_vectors() {
	// The key fingerprint commits to exactly the key FIPS 203 generates from the seed. A release of
	// `ml-kem` that generated anything else would change every fingerprint, and orphan every QR code,
	// without a word: this is what would catch it.
	let vectors: serde_json::Value =
		serde_json::from_str(include_str!("acvp-ml-kem-768-keygen.json")).unwrap();
	assert_eq!(vectors["parameterSet"], "ML-KEM-768");
	let tests = vectors["tests"].as_array().unwrap();
	assert_eq!(tests.len(), 25);
	for test in tests {
		let mut seed = Seed::default();
		seed[..32].copy_from_slice(&unhex(test["d"].as_str().unwrap()));
		seed[32..].copy_from_slice(&unhex(test["z"].as_str().unwrap()));
		assert_eq!(
			hex(&DeviceKemKey::from_seed(seed).encapsulation_key()),
			test["ek"].as_str().unwrap().to_lowercase(),
			"ACVP keyGen tcId {}",
			test["tcId"],
		);
	}
}

#[test]
fn the_fingerprint_commits_to_both_keys() {
	let keys = canonical_root().device_keys();
	let other = Root::from_bytes([0x01; ROOT_LEN]).device_keys();
	let public_key = keys.static_key.public_key();
	assert_ne!(
		KeyFingerprint::of(&other.static_key.public_key(), &keys.kem_key_digest),
		keys.fingerprint()
	);
	assert_ne!(
		KeyFingerprint::of(&public_key, &other.kem_key_digest),
		keys.fingerprint()
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
	// The context strings are what keep one from being the other.
	let root = canonical_root();
	assert_ne!(
		root.presence_token().as_bytes(),
		&root.device_static_key().as_bytes()[..PRESENCE_TOKEN_LEN]
	);
	assert_ne!(
		root.presence_token().as_bytes(),
		&root.as_bytes()[..PRESENCE_TOKEN_LEN]
	);
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

/// The canonical payload version 1 vector. Measured at 2.2 s on a Raspberry Pi 5, the slowest board
/// in scope, with the lanes computed concurrently, and 3.1 s with them in sequence.
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
