//! The authenticated channel: the layers that sit above GATT carrying reliable, ordered bytes.
//!
//! Behaviour is specified in `.workhorse/specs/channel.md` (CHN). Once a client has
//! matched a device by its handle, the two authenticate with a Noise `NNpsk0` handshake keyed by
//! the presence token, then carry application messages over the channel it establishes.
//!
//! The layers, each depending only on the one beneath it carrying bytes reliably and in order:
//!
//! | layer | module |
//! | --- | --- |
//! | framing | [`framing`] — message boundaries across the negotiated attribute size |
//! | Noise `NNpsk0` | [`noise`] — mutual authentication, encryption, a session key |
//! | compression | [`compress`] — one zlib stream per direction, spanning the connection |
//! | stream multiplexing | (yamux, wired in with the daemon's async transport) |
//! | JSON | [`messages`] — application messages |
//!
//! This module carries the transport-agnostic pieces. The daemon binds them to `bluer`'s GATT and
//! the web application to Web Bluetooth; the pieces themselves neither know nor care which.

pub mod compress;
pub mod envelope;
pub mod framing;
#[cfg(feature = "generate")]
pub mod generate;
pub mod messages;
pub mod noise;
pub mod readings;
pub mod stream;
pub mod write_backlog;

/// A failure in the channel below the application layer.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
	/// The handshake could not be built or driven: a wrong presence token, a replayed or spoofed
	/// handshake, or a peer that does not hold the secret all surface here, because none can complete
	/// the `NNpsk0` handshake.
	#[error("handshake failed: {0}")]
	Handshake(String),

	/// The peer's compressed stream could not be decompressed (CHN, "Compression"). The context is
	/// shared by every stream and is unrecoverable once it has diverged, so this costs the connection.
	#[error("decompression failed: {0}")]
	Decompress(String),
}
