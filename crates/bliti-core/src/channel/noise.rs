//! The Noise `NXpsk0` handshake and the transport it produces.
//!
//! Behaviour is specified in CHN, "Authentication". The client is authenticated by the pre-shared
//! key, which descends from the presence token in the QR code, and the device by its static key,
//! which it sends in the second message and the client checks against the key fingerprint in the
//! QR code. Completing the handshake proves that the device holds the static private key derived
//! from its own board ID and that the client read that device's QR code: a presence token alone, as
//! a photograph of the code yields, does not complete it as the device. The handshake produces a
//! fresh session key and gives the session forward secrecy, so recovering either credential later
//! does not decrypt a recorded session.
//!
//! The fingerprint check is made here rather than by each client, so the command-line client and
//! the web application cannot differ on it.
//!
//! `snow`'s pure-Rust default resolver is used, so this builds for `wasm32-unknown-unknown` and the
//! web application runs the identical handshake.

use snow::{Builder, HandshakeState, TransportState};

use super::ChannelError;
use crate::key_schedule::{
	DEVICE_KEY_LEN, DevicePublicKey, DeviceStaticKey, KEM_KEY_DIGEST_LEN, KemKeyDigest,
	KeyFingerprint, PreSharedKey,
};

/// The Noise protocol: `NXpsk0` over X25519, ChaCha20-Poly1305, and BLAKE2s. The responder sends its
/// static key in the second message, and the PSK sits at position 0, mixed in before the first
/// message. This string is part of the wire contract; changing it is incompatible with deployed
/// peers.
pub const NOISE_PARAMS: &str = "Noise_NXpsk0_25519_ChaChaPoly_BLAKE2s";

/// The largest Noise transport message, including its authentication tag.
const MAX_NOISE_MESSAGE: usize = 65535;

/// The Poly1305 authentication tag length added to every transport message.
const TAG_LEN: usize = 16;

/// The largest plaintext that fits in one transport message.
pub const MAX_PLAINTEXT: usize = MAX_NOISE_MESSAGE - TAG_LEN;

/// What each end brings to the handshake beyond the pre-shared key.
enum Role {
	/// The client, checking the device against the key fingerprint from the QR code.
	Initiator { fingerprint: KeyFingerprint },
	/// The device, sending its KEM key digest in the second message.
	Responder { kem_key_digest: KemKeyDigest },
}

/// An in-progress `NXpsk0` handshake.
///
/// `NXpsk0` is a two-message pattern: the initiator writes the first message, the responder reads it
/// and writes the second, and the initiator reads that. Both ends then move into transport mode.
pub struct Handshake {
	state: HandshakeState,
	role: Role,
	/// Set once the device has failed the fingerprint check, so the handshake can never be taken
	/// into transport mode afterwards.
	refused: bool,
}

impl Handshake {
	/// Build the initiating side, keyed by the pre-shared key and expecting the device whose keys
	/// match `fingerprint`, both from the QR code. The client is the initiator.
	pub fn initiator(
		psk: &PreSharedKey,
		fingerprint: &KeyFingerprint,
	) -> Result<Self, ChannelError> {
		let state = builder(psk)?
			.build_initiator()
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Self {
			state,
			role: Role::Initiator {
				fingerprint: *fingerprint,
			},
			refused: false,
		})
	}

	/// Build the responding side, keyed by the pre-shared key and the device static private key, and
	/// carrying the KEM key digest. The device is the responder.
	pub fn responder(
		psk: &PreSharedKey,
		device_static_key: &DeviceStaticKey,
		kem_key_digest: &KemKeyDigest,
	) -> Result<Self, ChannelError> {
		let state = builder(psk)?
			.local_private_key(device_static_key.as_bytes())
			.and_then(Builder::build_responder)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Self {
			state,
			role: Role::Responder {
				kem_key_digest: *kem_key_digest,
			},
			refused: false,
		})
	}

	/// Write the next handshake message, returning the bytes to send to the peer: the client's
	/// carries an empty payload, the device's the KEM key digest.
	pub fn write_message(&mut self) -> Result<Vec<u8>, ChannelError> {
		let payload: &[u8] = match &self.role {
			Role::Initiator { .. } => &[],
			Role::Responder { kem_key_digest } => kem_key_digest.as_bytes(),
		};
		let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
		let len = self
			.state
			.write_message(payload, &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		buf.truncate(len);
		Ok(buf)
	}

	/// Read a handshake message received from the peer.
	///
	/// On the client this is where the device is authenticated: the static key it sent and the KEM
	/// key digest it carried must give the fingerprint in the QR code, or the handshake fails here,
	/// before it reports finished and before anything further is sent.
	pub fn read_message(&mut self, message: &[u8]) -> Result<(), ChannelError> {
		let mut buf = vec![0u8; MAX_NOISE_MESSAGE];
		let len = self
			.state
			.read_message(message, &mut buf)
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		let payload = &buf[..len];
		match &self.role {
			Role::Responder { .. } if !payload.is_empty() => Err(ChannelError::Handshake(
				"the first message carries a payload".to_owned(),
			)),
			Role::Responder { .. } => Ok(()),
			Role::Initiator { fingerprint } => {
				let sent = self.sent_fingerprint(payload);
				if sent.as_ref() == Some(fingerprint) {
					Ok(())
				} else {
					self.refused = true;
					Err(ChannelError::Handshake(
						"the device's keys do not match its QR code".to_owned(),
					))
				}
			}
		}
	}

	/// The fingerprint of the keys the device sent, where it sent a static key and a digest of the
	/// right size.
	fn sent_fingerprint(&self, payload: &[u8]) -> Option<KeyFingerprint> {
		let static_key: [u8; DEVICE_KEY_LEN] = self.state.get_remote_static()?.try_into().ok()?;
		let digest: [u8; KEM_KEY_DIGEST_LEN] = payload.try_into().ok()?;
		Some(KeyFingerprint::of(
			&DevicePublicKey::from_bytes(static_key),
			&KemKeyDigest::from_bytes(digest),
		))
	}

	/// Whether the handshake has completed and can move into transport mode.
	pub fn is_finished(&self) -> bool {
		!self.refused && self.state.is_handshake_finished()
	}

	/// Move into transport mode once the handshake is finished.
	pub fn into_transport(self) -> Result<Transport, ChannelError> {
		if self.refused {
			return Err(ChannelError::Handshake(
				"the device's keys do not match its QR code".to_owned(),
			));
		}
		let state = self
			.state
			.into_transport_mode()
			.map_err(|err| ChannelError::Handshake(err.to_string()))?;
		Ok(Transport { state })
	}
}

fn builder(psk: &PreSharedKey) -> Result<Builder<'_>, ChannelError> {
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
mod tests;
