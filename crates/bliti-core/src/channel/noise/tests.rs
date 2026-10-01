use super::*;
use crate::key_schedule::{DeviceKeys, Root};

fn keys(byte: u8) -> DeviceKeys {
	Root::from_bytes([byte; 32]).device_keys()
}

/// What a client holds from a device's QR code.
struct Code {
	psk: PreSharedKey,
	fingerprint: KeyFingerprint,
}

fn code_of(device: &DeviceKeys) -> Code {
	Code {
		psk: device.pre_shared_key.clone(),
		fingerprint: device.fingerprint(),
	}
}

/// Drive a full handshake between a client holding a code and a device holding its keys, returning
/// their transports if it completes.
fn run(code: &Code, device: &DeviceKeys) -> Result<(Transport, Transport), ChannelError> {
	let mut client = Handshake::initiator(&code.psk, &code.fingerprint)?;
	let mut device = Handshake::responder(
		&device.pre_shared_key,
		&device.static_key,
		&device.kem_key_digest,
	)?;

	let msg1 = client.write_message()?;
	device.read_message(&msg1)?;
	let msg2 = device.write_message()?;
	client.read_message(&msg2)?;

	assert!(client.is_finished());
	assert!(device.is_finished());
	Ok((client.into_transport()?, device.into_transport()?))
}

/// A device answering with `static_key` and `kem_key_digest` in place of its own, to a client that
/// read the real device's code. Returns the client's handshake after it has read the second
/// message, and what reading it gave.
fn against_impostor(
	real: &DeviceKeys,
	static_key: &DeviceStaticKey,
	kem_key_digest: &KemKeyDigest,
) -> (Handshake, Result<(), ChannelError>) {
	let code = code_of(real);
	let mut client = Handshake::initiator(&code.psk, &code.fingerprint).unwrap();
	let mut device =
		Handshake::responder(&real.pre_shared_key, static_key, kem_key_digest).unwrap();
	device
		.read_message(&client.write_message().unwrap())
		.unwrap();
	let read = client.read_message(&device.write_message().unwrap());
	(client, read)
}

#[test]
fn protocol_name_is_nxpsk0() {
	assert_eq!(NOISE_PARAMS, "Noise_NXpsk0_25519_ChaChaPoly_BLAKE2s");
}

#[test]
fn matching_credentials_complete_and_carry_messages() {
	let device = keys(0xab);
	let (mut client, mut device) = run(&code_of(&device), &device).unwrap();

	// Both directions carry traffic under the session key.
	let ct = client.encrypt(b"hello device").unwrap();
	assert_ne!(ct, b"hello device");
	assert_eq!(device.decrypt(&ct).unwrap(), b"hello device");

	let ct = device.encrypt(b"hello client").unwrap();
	assert_eq!(client.decrypt(&ct).unwrap(), b"hello client");
}

#[test]
fn the_messages_are_the_sizes_chn_allows_for() {
	// The first an ephemeral and a tag; the second an ephemeral, the static key and its tag, and
	// the digest and its tag.
	let device = keys(0x21);
	let code = code_of(&device);
	let mut client = Handshake::initiator(&code.psk, &code.fingerprint).unwrap();
	let mut responder = Handshake::responder(
		&device.pre_shared_key,
		&device.static_key,
		&device.kem_key_digest,
	)
	.unwrap();
	let msg1 = client.write_message().unwrap();
	assert_eq!(msg1.len(), 48);
	responder.read_message(&msg1).unwrap();
	assert_eq!(responder.write_message().unwrap().len(), 128);
}

#[test]
fn a_wrong_token_fails_at_the_first_message() {
	// A client that scanned a different QR code is turned away before the device runs any
	// Diffie-Hellman of its own or sends its static key.
	let device = keys(0x02);
	let code = Code {
		psk: keys(0x01).pre_shared_key,
		fingerprint: device.fingerprint(),
	};
	let mut client = Handshake::initiator(&code.psk, &code.fingerprint).unwrap();
	let mut responder = Handshake::responder(
		&device.pre_shared_key,
		&device.static_key,
		&device.kem_key_digest,
	)
	.unwrap();
	let read = responder.read_message(&client.write_message().unwrap());
	assert!(matches!(read, Err(ChannelError::Handshake(_))));
	assert!(!responder.is_finished());
}

#[test]
fn a_device_whose_static_key_does_not_match_the_fingerprint_fails() {
	// Something holding a photograph of the QR code has the presence token but not the static
	// private key, so it cannot answer as the device (SEC, "A photograph does not permit
	// impersonation").
	let real = keys(0x10);
	let (client, read) = against_impostor(&real, &keys(0x11).static_key, &real.kem_key_digest);
	assert!(matches!(read, Err(ChannelError::Handshake(_))));
	assert!(!client.is_finished());
	assert!(matches!(
		client.into_transport(),
		Err(ChannelError::Handshake(_))
	));
}

#[test]
fn a_device_whose_kem_key_digest_does_not_match_the_fingerprint_fails() {
	let real = keys(0x10);
	let (client, read) = against_impostor(&real, &real.static_key, &keys(0x11).kem_key_digest);
	assert!(matches!(read, Err(ChannelError::Handshake(_))));
	assert!(!client.is_finished());
	assert!(client.into_transport().is_err());
}

#[test]
fn replayed_transport_message_is_rejected() {
	let device = keys(0x7c);
	let (mut client, mut device) = run(&code_of(&device), &device).unwrap();
	let first = client.encrypt(b"one").unwrap();
	let second = client.encrypt(b"two").unwrap();
	assert_eq!(device.decrypt(&first).unwrap(), b"one");
	// Replaying the first message out of order fails the nonce-bound authentication.
	assert!(device.decrypt(&first).is_err());
	// And the legitimate next message still decrypts, proving the failure is the replay.
	assert_eq!(device.decrypt(&second).unwrap(), b"two");
}

#[test]
fn tampered_message_is_rejected() {
	let device = keys(0x33);
	let (mut client, mut device) = run(&code_of(&device), &device).unwrap();
	let mut ct = client.encrypt(b"authentic").unwrap();
	let last = ct.len() - 1;
	ct[last] ^= 0x01;
	assert!(device.decrypt(&ct).is_err());
}
