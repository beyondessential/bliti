//! The Noise `NKpsk0` handshake and the transport it produces.
//!
//! Behaviour is specified in CHN, "Authentication". The device is authenticated by its static key,
//! whose public half the client takes from the QR code, and the client by the presence token, the
//! pre-shared key. Completing the handshake proves that the device holds the static private key
//! derived from its own board ID and that the client read that device's QR code: a presence token
//! alone, as a photograph of the code yields, does not complete it as the device. The handshake
//! produces a fresh session key and gives the session forward secrecy, so recovering either
//! credential later does not decrypt a recorded session.
//!
//! `snow`'s pure-Rust default resolver is used, so this builds for `wasm32-unknown-unknown` and the
//! web application runs the identical handshake.

use snow::{Builder, HandshakeState, TransportState};

use super::ChannelError;
use crate::key_schedule::{DevicePublicKey, DeviceStaticKey, PresenceToken};

/// The Noise protocol: `NKpsk0` over X25519, ChaCha20-Poly1305, and BLAKE2s. The responder's static
/// key is known to the initiator in advance, and the PSK sits at position 0, mixed in before the
/// first message. This string is part of the wire contract; changing it is incompatible with
/// deployed peers.
pub const NOISE_PARAMS: &str = "Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s";

/// The largest Noise transport message, including its authentication tag.
const MAX_NOISE_MESSAGE: usize = 65535;

/// The Poly1305 authentication tag length added to every transport message.
const TAG_LEN: usize = 16;

/// The largest plaintext that fits in one transport message.
pub const MAX_PLAINTEXT: usize = MAX_NOISE_MESSAGE - TAG_LEN;

/// An in-progress `NKpsk0` handshake.
///
/// `NKpsk0` is a two-message pattern: the initiator writes the first message, the responder reads it
/// and writes the second, and the initiator reads that. Both ends then move into transport mode.
pub struct Handshake {
	state: HandshakeState,
}

impl Handshake {
	/// Build the initiating side, keyed by the presence token and the device static public key, both
	/// from the QR code. The client is the initiator.
	pub fn initiator(
		psk: &PresenceToken,
		device_public_key: &DevicePublicKey,
	) -> Result<Self, ChannelError> {
		let state = builder(psk)?
			.remote_public_key(device_public_key.as_bytes())
			.and_then(Builder::build_initiator)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Self { state })
	}

	/// Build the responding side, keyed by the presence token and the device static private key. The
	/// device is the responder.
	pub fn responder(
		psk: &PresenceToken,
		device_static_key: &DeviceStaticKey,
	) -> Result<Self, ChannelError> {
		let state = builder(psk)?
			.local_private_key(device_static_key.as_bytes())
			.and_then(Builder::build_responder)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Self { state })
	}

	/// Write the next handshake message, returning the bytes to send to the peer.
	pub fn write_message(&mut self) -> Result<Vec<u8>, ChannelError> {
		let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
		let len = self
			.state
			.write_message(&[], &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		buf.truncate(len);
		Ok(buf)
	}

	/// Read a handshake message received from the peer.
	pub fn read_message(&mut self, message: &[u8]) -> Result<(), ChannelError> {
		let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
		self.state
			.read_message(message, &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(())
	}

	/// Whether the handshake has completed and can move into transport mode.
	pub fn is_finished(&self) -> bool {
		self.state.is_handshake_finished()
	}

	/// Move into transport mode once the handshake is finished.
	pub fn into_transport(self) -> Result<Transport, ChannelError> {
		let state = self
			.state
			.into_transport_mode()
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Transport { state })
	}
}

fn builder(psk: &PresenceToken) -> Result<Builder<'_>, ChannelError> {
	let params = NOISE_PARAMS
		.parse()
		.map_err(|err| ChannelError::Handshake(format!("invalid Noise parameters: {err}")))?;
	Builder::new(params)
		.psk(0, psk.as_bytes())
		.map_err(|err| ChannelError::Handshake(err.to_string()))
}

/// An established Noise transport: the encrypted, authenticated channel the handshake produces.
///
/// Each call encrypts or decrypts one Noise transport message. Above this sits the stream layer,
/// which chops its byte stream into pieces no larger than [`MAX_PLAINTEXT`].
pub struct Transport {
	state: TransportState,
}

impl Transport {
	/// Encrypt one plaintext into a transport message. The plaintext must be no larger than
	/// [`MAX_PLAINTEXT`].
	pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, ChannelError> {
		let mut buf = vec![0u8; plaintext.len() + TAG_LEN];
		let len = self
			.state
			.write_message(plaintext, &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		buf.truncate(len);
		Ok(buf)
	}

	/// Decrypt one transport message into its plaintext. A message that fails authentication — a
	/// replay, a forgery, or corruption — surfaces as an error rather than plaintext.
	pub fn decrypt(&mut self, message: &[u8]) -> Result<Vec<u8>, ChannelError> {
		let mut buf = vec![0u8; message.len()];
		let len = self
			.state
			.read_message(message, &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		buf.truncate(len);
		Ok(buf)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::key_schedule::{DeviceKeys, Root};

	fn keys(byte: u8) -> DeviceKeys {
		Root::from_bytes([byte; 32]).device_keys()
	}

	/// Drive a full handshake between a client holding a presence token and a device public key, and
	/// a device holding its keys, returning their transports if it completes.
	fn run(
		client_psk: &PresenceToken,
		client_device_public_key: &DevicePublicKey,
		device: &DeviceKeys,
	) -> Result<(Transport, Transport), ChannelError> {
		let mut client = Handshake::initiator(client_psk, client_device_public_key)?;
		let mut device = Handshake::responder(&device.presence_token, &device.static_key)?;

		let msg1 = client.write_message()?;
		device.read_message(&msg1)?;
		let msg2 = device.write_message()?;
		client.read_message(&msg2)?;

		assert!(client.is_finished());
		assert!(device.is_finished());
		Ok((client.into_transport()?, device.into_transport()?))
	}

	/// A handshake from a client that read the device's own QR code.
	fn run_matching(device: &DeviceKeys) -> Result<(Transport, Transport), ChannelError> {
		run(
			&device.presence_token,
			&device.static_key.public_key(),
			device,
		)
	}

	#[test]
	fn protocol_name_is_nkpsk0() {
		assert_eq!(NOISE_PARAMS, "Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s");
	}

	#[test]
	fn matching_credentials_complete_and_carry_messages() {
		let (mut client, mut device) = run_matching(&keys(0xab)).unwrap();

		// Both directions carry traffic under the session key.
		let ct = client.encrypt(b"hello device").unwrap();
		assert_ne!(ct, b"hello device");
		assert_eq!(device.decrypt(&ct).unwrap(), b"hello device");

		let ct = device.encrypt(b"hello client").unwrap();
		assert_eq!(client.decrypt(&ct).unwrap(), b"hello client");
	}

	#[test]
	fn wrong_presence_token_fails_the_handshake() {
		// A client that scanned a different QR code cannot complete the handshake.
		let device = keys(0x02);
		let result = run(
			&keys(0x01).presence_token,
			&device.static_key.public_key(),
			&device,
		);
		assert!(matches!(result, Err(ChannelError::Handshake(_))));
	}

	#[test]
	fn wrong_device_public_key_fails_the_handshake_even_with_the_right_token() {
		// Something holding a photograph of the QR code has the presence token but not the static
		// private key, so it cannot answer as the device: a client expecting the real device's key
		// does not complete a handshake with it (SEC, "A photograph does not permit impersonation").
		let real = keys(0x10);
		let impostor = DeviceKeys {
			presence_token: real.presence_token.clone(),
			static_key: keys(0x11).static_key,
		};
		let result = run(
			&real.presence_token,
			&real.static_key.public_key(),
			&impostor,
		);
		assert!(matches!(result, Err(ChannelError::Handshake(_))));

		// And the same from the other side: a client holding the wrong public key fails against the
		// real device.
		let result = run(
			&real.presence_token,
			&keys(0x11).static_key.public_key(),
			&real,
		);
		assert!(matches!(result, Err(ChannelError::Handshake(_))));
	}

	#[test]
	fn replayed_transport_message_is_rejected() {
		let (mut client, mut device) = run_matching(&keys(0x7c)).unwrap();
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
		let (mut client, mut device) = run_matching(&keys(0x33)).unwrap();
		let mut ct = client.encrypt(b"authentic").unwrap();
		let last = ct.len() - 1;
		ct[last] ^= 0x01;
		assert!(device.decrypt(&ct).is_err());
	}
}
