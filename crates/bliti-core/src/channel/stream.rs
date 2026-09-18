//! The stream layer: an encrypted byte stream over any transport, and yamux multiplexing above it.
//!
//! Behaviour is specified in CHN, "Streams". Above the handshake, either end opens streams,
//! unidirectional or bidirectional, without coordinating identifiers and without asking permission,
//! and several are in flight at once. Closing one leaves the others and the connection alive. This is
//! what lets a device send without being asked.
//!
//! Two pieces live here:
//!
//! - [`NoiseStream`] wraps a byte transport and an established Noise [`Transport`], presenting an
//!   `AsyncRead + AsyncWrite` byte stream: it encrypts each write into one framed Noise message and
//!   reassembles and decrypts on read. This is the encrypted, reliable, ordered channel yamux runs
//!   on.
//! - [`multiplex`] runs yamux over a [`NoiseStream`], returning a [`Streams`] handle for opening and
//!   accepting streams and a driver future the host spawns. Driving is the one runtime-specific part;
//!   the daemon spawns it on tokio and the web application on the browser's executor.

use std::{
	future::poll_fn,
	io,
	pin::Pin,
	task::{Context, Poll},
};

use futures::{
	AsyncRead, AsyncWrite, StreamExt,
	channel::{mpsc, oneshot},
};
use yamux::Connection;

pub use yamux::{ConnectionError, Mode, Stream};

/// Whether a connection ended because the peer is not speaking the protocol, rather than because it
/// went away.
///
/// A decompression failure (CHN, "Compression") and a Noise message that fails authentication both
/// cost the whole connection and are faults in the peer worth reporting. A client walking out of
/// range, or a device restarting, ends the connection just as surely and is nobody's fault.
///
/// The two are told apart by the error a layer of this channel attached, not by its kind: both layers
/// carry a [`ChannelError`] inside the `io::Error` they produce when what the peer sent cannot be
/// read, so a fault is what the chain of causes holds one of. An ordinary ending carries none, and
/// neither does an unrelated transport failure that happens to report the same kind, nor this end's
/// own compressor or cipher failing on the way out. yamux wraps a read failure as a decode error
/// rather than an I/O one, so the whole chain is walked rather than the outermost variant matched.
pub fn is_peer_fault(err: &ConnectionError) -> bool {
	let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(err);
	while let Some(err) = cause {
		// `io::Error`'s own `source` skips past the error it was built from, so read it off directly.
		if let Some(inner) = err.downcast_ref::<io::Error>().and_then(io::Error::get_ref)
			&& inner.downcast_ref::<ChannelError>().is_some()
		{
			return true;
		}
		cause = err.source();
	}
	false
}

use super::{
	ChannelError,
	compress::CompressStream,
	framing::{
		MESSAGE_PREFIX, Reassembler, TRANSPORT_PREFIX, encode_prefix, read_delimited,
		write_delimited,
	},
	noise::{Handshake, MAX_PLAINTEXT, Transport},
	write_backlog::WriteBacklog,
};
use crate::key_schedule::PresenceToken;

/// The size of the buffer used to pull bytes off the inner transport on each read.
const READ_CHUNK: usize = 8192;

fn to_io(err: ChannelError) -> io::Error {
	io::Error::new(io::ErrorKind::InvalidData, err)
}

/// An encrypted, reliable, ordered byte stream: a byte transport plus an established Noise transport.
///
/// Each write is encrypted into one length-framed Noise message; reads reassemble those frames and
/// decrypt them. A message that fails authentication surfaces as an I/O error rather than plaintext,
/// which tears the stream down.
pub struct NoiseStream<S> {
	inner: S,
	transport: Transport,
	reassembler: Reassembler,
	read_plain: Vec<u8>,
	read_consumed: usize,
	read_chunk: Box<[u8]>,
	write: WriteBacklog,
}

impl<S> NoiseStream<S> {
	/// Wrap a byte transport and an established Noise transport.
	pub fn new(inner: S, transport: Transport) -> Self {
		Self {
			inner,
			transport,
			reassembler: Reassembler::new(),
			read_plain: Vec::new(),
			read_consumed: 0,
			read_chunk: vec![0u8; READ_CHUNK].into_boxed_slice(),
			write: WriteBacklog::default(),
		}
	}
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for NoiseStream<S> {
	fn poll_write(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &[u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		if buf.is_empty() {
			return Poll::Ready(Ok(0));
		}
		// One Noise message is in flight at a time: finish sending it before encrypting the next.
		std::task::ready!(this.write.poll_drain(&mut this.inner, cx))?;
		let chunk = &buf[..buf.len().min(MAX_PLAINTEXT)];
		let ciphertext = this.transport.encrypt(chunk).map_err(to_io)?;
		// Prefix then ciphertext into the backlog the drain above just emptied, rather than a fresh
		// buffer holding a copy of both: the backlog keeps its allocation, so a connection settles on
		// one per direction instead of one per Noise message.
		this.write
			.extend(&encode_prefix::<TRANSPORT_PREFIX>(ciphertext.len())?);
		this.write.extend(&ciphertext);
		// Best-effort flush; anything left is drained on the next call or on flush.
		let _ = this.write.poll_drain(&mut this.inner, cx)?;
		Poll::Ready(Ok(chunk.len()))
	}

	fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		let this = self.get_mut();
		std::task::ready!(this.write.poll_drain(&mut this.inner, cx))?;
		Pin::new(&mut this.inner).poll_flush(cx)
	}

	fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
		let this = self.get_mut();
		std::task::ready!(this.write.poll_drain(&mut this.inner, cx))?;
		Pin::new(&mut this.inner).poll_close(cx)
	}
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for NoiseStream<S> {
	fn poll_read(
		self: Pin<&mut Self>,
		cx: &mut Context<'_>,
		buf: &mut [u8],
	) -> Poll<io::Result<usize>> {
		let this = self.get_mut();
		loop {
			// Serve buffered plaintext first.
			if this.read_consumed < this.read_plain.len() {
				let available = &this.read_plain[this.read_consumed..];
				let n = available.len().min(buf.len());
				buf[..n].copy_from_slice(&available[..n]);
				this.read_consumed += n;
				if this.read_consumed == this.read_plain.len() {
					this.read_plain.clear();
					this.read_consumed = 0;
				}
				return Poll::Ready(Ok(n));
			}

			// Decrypt the next reassembled frame, if one is ready.
			if let Some(message) = this.reassembler.take() {
				this.read_plain = this.transport.decrypt(&message).map_err(to_io)?;
				this.read_consumed = 0;
				continue;
			}

			// Otherwise pull more bytes off the inner transport.
			let n =
				std::task::ready!(Pin::new(&mut this.inner).poll_read(cx, &mut this.read_chunk))?;
			if n == 0 {
				return Poll::Ready(Ok(0));
			}
			this.reassembler.push(&this.read_chunk[..n]);
		}
	}
}

/// A handle for opening and accepting streams over a multiplexed connection.
pub struct Streams {
	open: mpsc::UnboundedSender<oneshot::Sender<io::Result<Stream>>>,
	inbound: mpsc::UnboundedReceiver<Stream>,
}

impl Streams {
	/// Open a new outbound stream. Bidirectional; a client that wants a one-way stream simply never
	/// reads or never writes.
	pub async fn open(&mut self) -> io::Result<Stream> {
		let (tx, rx) = oneshot::channel();
		self.open
			.unbounded_send(tx)
			.map_err(|_| io::Error::from(io::ErrorKind::NotConnected))?;
		rx.await
			.map_err(|_| io::Error::from(io::ErrorKind::NotConnected))?
	}

	/// Accept the next inbound stream the peer opens, or `None` once the connection closes.
	pub async fn accept(&mut self) -> Option<Stream> {
		self.inbound.next().await
	}
}

/// Run yamux over an encrypted stream, returning a handle and a driver future.
///
/// The driver must be spawned and polled for anything to progress: it services open requests, surfaces
/// inbound streams, and drives the I/O of every open stream. The client is [`Mode::Client`] and the
/// device [`Mode::Server`]; the two must differ.
///
/// Compression is wired in here rather than left to each caller, so no call site can assemble an
/// uncompressed channel: the always-on property of CHN is structural, not a convention the daemon and
/// the web client each have to remember. yamux runs on the compressed stream, which runs on the
/// encrypted one.
pub fn multiplex<S>(
	socket: NoiseStream<S>,
	mode: Mode,
) -> (
	Streams,
	impl std::future::Future<Output = Result<(), yamux::ConnectionError>>,
)
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	let (open_tx, open_rx) = mpsc::unbounded();
	let (inbound_tx, inbound_rx) = mpsc::unbounded();
	let connection = Connection::new(CompressStream::new(socket), yamux::Config::default(), mode);
	let streams = Streams {
		open: open_tx,
		inbound: inbound_rx,
	};
	(streams, drive(connection, open_rx, inbound_tx))
}

async fn drive<S>(
	mut connection: Connection<CompressStream<NoiseStream<S>>>,
	mut open_rx: mpsc::UnboundedReceiver<oneshot::Sender<io::Result<Stream>>>,
	inbound_tx: mpsc::UnboundedSender<Stream>,
) -> Result<(), yamux::ConnectionError>
where
	S: AsyncRead + AsyncWrite + Unpin,
{
	// One poll_fn owns the connection, so opening, accepting, and driving all streams happen from a
	// single place and never borrow the connection twice.
	let mut pending: std::collections::VecDeque<oneshot::Sender<io::Result<Stream>>> =
		std::collections::VecDeque::new();
	let mut closing = false;

	poll_fn(move |cx| {
		// Collect any new open requests.
		while !closing {
			match open_rx.poll_next_unpin(cx) {
				Poll::Ready(Some(reply)) => pending.push_back(reply),
				// The handle is gone, so nothing can open or accept another stream and the connection has
				// no further purpose. Close it rather than leave it to be dropped: a drop never finishes
				// the compression stream, and a transport that ends before its stream does reads as a
				// truncation at the far end rather than the clean ending it is (CHN).
				Poll::Ready(None) => closing = true,
				Poll::Pending => break,
			}
		}

		if closing {
			// Anything still waiting for a stream will not be getting one.
			for reply in pending.drain(..) {
				let _ = reply.send(Err(io::ErrorKind::NotConnected.into()));
			}
			return connection.poll_close(cx);
		}

		// Service pending open requests.
		while !pending.is_empty() {
			match connection.poll_new_outbound(cx) {
				Poll::Ready(Ok(stream)) => {
					let reply = pending.pop_front().expect("non-empty");
					let _ = reply.send(Ok(stream));
				}
				Poll::Ready(Err(err)) => {
					let reply = pending.pop_front().expect("non-empty");
					let _ = reply.send(Err(io::Error::other(err.to_string())));
				}
				Poll::Pending => break,
			}
		}

		// Drive inbound streams, which also drives the I/O of every open stream.
		loop {
			match connection.poll_next_inbound(cx) {
				Poll::Ready(Some(Ok(stream))) => {
					let _ = inbound_tx.unbounded_send(stream);
				}
				Poll::Ready(Some(Err(err))) => return Poll::Ready(Err(err)),
				Poll::Ready(None) => return Poll::Ready(Ok(())),
				Poll::Pending => return Poll::Pending,
			}
		}
	})
	.await
}

/// Run the `NNpsk0` handshake as the initiator over a byte transport and return the encrypted stream.
/// The client is the initiator. The handshake reads exactly its two framed messages, leaving no bytes
/// buffered, so the returned [`NoiseStream`] takes over a clean transport.
pub async fn connect_initiator<S: AsyncRead + AsyncWrite + Unpin>(
	mut inner: S,
	psk: &PresenceToken,
) -> Result<NoiseStream<S>, ChannelError> {
	let mut handshake = Handshake::initiator(psk)?;
	let msg1 = handshake.write_message()?;
	write_delimited::<TRANSPORT_PREFIX, _>(&mut inner, &msg1)
		.await
		.map_err(|err| ChannelError::Handshake(err.to_string()))?;
	let msg2 = read_delimited::<TRANSPORT_PREFIX, _>(&mut inner)
		.await
		.map_err(|err| ChannelError::Handshake(err.to_string()))?
		.ok_or_else(|| ChannelError::Handshake("peer closed during handshake".to_owned()))?;
	handshake.read_message(&msg2)?;
	Ok(NoiseStream::new(inner, handshake.into_transport()?))
}

/// Run the `NNpsk0` handshake as the responder over a byte transport and return the encrypted stream.
/// The device is the responder.
pub async fn accept_responder<S: AsyncRead + AsyncWrite + Unpin>(
	mut inner: S,
	psk: &PresenceToken,
) -> Result<NoiseStream<S>, ChannelError> {
	let mut handshake = Handshake::responder(psk)?;
	let msg1 = read_delimited::<TRANSPORT_PREFIX, _>(&mut inner)
		.await
		.map_err(|err| ChannelError::Handshake(err.to_string()))?
		.ok_or_else(|| ChannelError::Handshake("peer closed during handshake".to_owned()))?;
	handshake.read_message(&msg1)?;
	let msg2 = handshake.write_message()?;
	write_delimited::<TRANSPORT_PREFIX, _>(&mut inner, &msg2)
		.await
		.map_err(|err| ChannelError::Handshake(err.to_string()))?;
	Ok(NoiseStream::new(inner, handshake.into_transport()?))
}

/// Write a length-delimited application message to a stream. Several messages ride on one stream, so
/// each is delimited by a three-byte length prefix (MSG). That is a different width from the two-byte
/// prefix the transport framing gives a Noise message, so code reading one cannot read the other.
pub async fn write_message<W: AsyncWrite + Unpin>(
	stream: &mut W,
	message: &[u8],
) -> io::Result<()> {
	write_delimited::<MESSAGE_PREFIX, _>(stream, message).await
}

/// Read the next length-delimited application message from a stream, or `None` at end of stream.
///
/// The three-byte prefix cannot express more than a message may be, so there is no ceiling to check
/// and nothing to refuse.
pub async fn read_message<R: AsyncRead + Unpin>(stream: &mut R) -> io::Result<Option<Vec<u8>>> {
	read_delimited::<MESSAGE_PREFIX, _>(stream).await
}

#[cfg(test)]
mod tests {
	use futures::AsyncWriteExt;
	use tokio_util::compat::TokioAsyncReadCompatExt;

	use super::*;
	use crate::channel::{
		envelope::{Reading, read},
		messages::{ClientMessage, DeviceMessage},
	};

	/// Set up a client and device connected over an in-memory duplex: a full `NNpsk0` handshake, then
	/// yamux on both ends with their drivers spawned. No BLE is involved.
	async fn paired() -> (Streams, Streams) {
		let psk = PresenceToken::from_bytes([0x5a; 32]);
		let (a, b) = tokio::io::duplex(1 << 16);
		let (client_ns, device_ns) = tokio::join!(
			connect_initiator(a.compat(), &psk),
			accept_responder(b.compat(), &psk)
		);
		let (client_streams, client_driver) = multiplex(client_ns.unwrap(), Mode::Client);
		let (device_streams, device_driver) = multiplex(device_ns.unwrap(), Mode::Server);
		tokio::spawn(async move {
			let _ = client_driver.await;
		});
		tokio::spawn(async move {
			let _ = device_driver.await;
		});
		(client_streams, device_streams)
	}

	#[tokio::test]
	async fn handshake_then_bidirectional_exchange_on_one_stream() {
		let (mut client, mut device) = paired().await;

		// Client to device: a subscription.
		let mut cs = client.open().await.unwrap();
		let subscribe = ClientMessage::Subscribe {
			topic: "system".to_owned(),
		};
		write_message(&mut cs, &subscribe.to_json()).await.unwrap();

		let mut ds = device.accept().await.unwrap();
		let received = read_message(&mut ds).await.unwrap().unwrap();
		assert_eq!(read(&received).unwrap(), Reading::Message(subscribe));

		// Device to client, on the same stream: a sample for that subscription.
		let sample = DeviceMessage::SystemSample {
			at: 20_308_140,
			readings: Vec::new(),
		};
		write_message(&mut ds, &sample.to_json()).await.unwrap();
		let back = read_message(&mut cs).await.unwrap().unwrap();
		assert_eq!(read(&back).unwrap(), Reading::Message(sample));
	}

	/// Closing a stream is the unsubscribe, and it is a half-close: the peer reads end of stream while
	/// its own write side stays open. A device that did not act on that would go on sending to a
	/// client that has said it is done, which is the case the subscription mechanism exists to
	/// prevent (MSG, "Subscribing").
	#[tokio::test]
	async fn closing_a_stream_is_read_as_end_of_stream_and_leaves_the_peer_writable() {
		let (mut client, mut device) = paired().await;

		let mut cs = client.open().await.unwrap();
		let subscribe = ClientMessage::Subscribe {
			topic: "system".to_owned(),
		};
		write_message(&mut cs, &subscribe.to_json()).await.unwrap();
		let mut ds = device.accept().await.unwrap();
		assert!(read_message(&mut ds).await.unwrap().is_some());

		cs.close().await.unwrap();

		// What the device acts on to drop the subscription.
		assert!(read_message(&mut ds).await.unwrap().is_none());

		// And the half-close leaves the device's own write side open, which is precisely why acting
		// on the end of stream is the device's job rather than something the transport does for it.
		write_message(&mut ds, b"still writable").await.unwrap();
	}

	/// A stream dropped without being closed reaches the peer as a reset rather than a graceful end,
	/// so a subscription ends however its stream ends and no drop guard is needed (MSG).
	#[tokio::test]
	async fn dropping_a_stream_also_ends_it_for_the_peer() {
		let (mut client, mut device) = paired().await;

		let mut cs = client.open().await.unwrap();
		write_message(&mut cs, b"opened").await.unwrap();
		let mut ds = device.accept().await.unwrap();
		assert!(read_message(&mut ds).await.unwrap().is_some());

		drop(cs);

		// Either a clean end or a reset: both tell the device the subscription is over.
		let over = match read_message(&mut ds).await {
			Ok(None) | Err(_) => true,
			Ok(Some(_)) => false,
		};
		assert!(over, "a dropped stream must reach the peer");
	}

	/// Application messages are delimited within a stream by a three-byte big-endian prefix (MSG),
	/// which is one byte wider than the two-byte prefix the link uses for Noise messages a layer down.
	/// A message is reassembled whatever sizes the reads arrive in, and several in one read are
	/// separated.
	#[tokio::test]
	async fn messages_are_delimited_within_a_stream() {
		let (mut client, mut device) = paired().await;

		let mut cs = client.open().await.unwrap();
		// Three messages written back to back, which the peer may read in any grouping.
		for each in [b"one".as_slice(), b"two".as_slice(), b"three".as_slice()] {
			write_message(&mut cs, each).await.unwrap();
		}

		let mut ds = device.accept().await.unwrap();
		assert_eq!(read_message(&mut ds).await.unwrap().unwrap(), b"one");
		assert_eq!(read_message(&mut ds).await.unwrap().unwrap(), b"two");
		assert_eq!(read_message(&mut ds).await.unwrap().unwrap(), b"three");

		// A message far larger than any one read, reassembled byte for byte.
		let long = vec![b'x'; 40_000];
		write_message(&mut cs, &long).await.unwrap();
		assert_eq!(read_message(&mut ds).await.unwrap().unwrap(), long);
	}

	/// A message far past the former 128 KiB ceiling round-trips: the ceiling is gone (MSG), and the
	/// compression layer and yamux flow control carry it. It is also past the 256 KiB receive window,
	/// so it moves only because the receiver credits the window as it consumes, and it is not one
	/// repeated byte, so compression does not shrink it to nothing on the way.
	#[tokio::test]
	async fn a_message_past_the_former_ceiling_round_trips() {
		let (mut client, mut device) = paired().await;

		let big: Vec<u8> = (0..600_000).map(|i| (i % 251) as u8).collect();
		let sent = big.clone();
		let mut cs = client.open().await.unwrap();
		// Write from its own task: past the window, the write parks until the reader credits it, so the
		// read below has to run alongside rather than after.
		let writer = tokio::spawn(async move {
			write_message(&mut cs, &sent).await.unwrap();
			cs
		});

		let mut ds = device.accept().await.unwrap();
		assert_eq!(read_message(&mut ds).await.unwrap().unwrap(), big);
		let _cs = writer.await.unwrap();
	}

	/// A decompression failure is a fault in the peer, and reaches the host as one. yamux wraps a
	/// failure reading the socket as a decode error rather than an I/O one, so the classification has to
	/// look at what the compression layer attached rather than at the outermost variant or its kind.
	#[tokio::test]
	async fn a_decompression_failure_is_classified_as_a_peer_fault() {
		let (a, b) = tokio::io::duplex(1 << 16);
		// Bytes that are not a zlib stream, put where the compression layer will read them.
		let mut peer = b.compat();
		peer.write_all(&[0xff; 64]).await.unwrap();
		peer.flush().await.unwrap();

		let mut connection = Connection::new(
			CompressStream::new(a.compat()),
			yamux::Config::default(),
			Mode::Client,
		);
		let err = poll_fn(|cx| connection.poll_next_inbound(cx))
			.await
			.expect("the connection fails rather than ending")
			.expect_err("a stream cannot be decoded from bytes that are not a zlib stream");
		assert!(
			is_peer_fault(&err),
			"a decompression failure is a fault in the peer, got {err:?}"
		);
	}

	/// A connection whose transport simply went away is nobody's fault, and is not reported as one.
	#[tokio::test]
	async fn a_transport_that_goes_away_is_not_a_peer_fault() {
		let (a, b) = tokio::io::duplex(1 << 16);
		drop(b);

		let mut connection = Connection::new(
			CompressStream::new(a.compat()),
			yamux::Config::default(),
			Mode::Client,
		);
		// Either a clean end or an I/O failure, but never the peer's fault.
		if let Some(Err(err)) = poll_fn(|cx| connection.poll_next_inbound(cx)).await {
			assert!(
				!is_peer_fault(&err),
				"a transport that went away is nobody's fault, got {err:?}"
			);
		}
	}

	/// Dropping the handle closes the connection, and the far end reads that as the clean ending it is.
	/// Closing finishes the compression stream; a driver that was simply dropped would leave it
	/// unterminated, and the peer reads a transport that ends before its stream does as a truncation, so
	/// a deliberate teardown would be reported as a fault (CHN).
	#[tokio::test]
	async fn dropping_the_handle_closes_the_connection_cleanly() {
		let psk = PresenceToken::from_bytes([0x5a; 32]);
		let (a, b) = tokio::io::duplex(1 << 16);
		let (client_ns, device_ns) = tokio::join!(
			connect_initiator(a.compat(), &psk),
			accept_responder(b.compat(), &psk)
		);
		let (mut client, client_driver) = multiplex(client_ns.unwrap(), Mode::Client);
		let (mut device, device_driver) = multiplex(device_ns.unwrap(), Mode::Server);
		let client_driving = tokio::spawn(client_driver);
		let device_driving = tokio::spawn(device_driver);

		// A real exchange first, so each direction's context has something in it to finish.
		let mut cs = client.open().await.unwrap();
		write_message(&mut cs, b"a subscription").await.unwrap();
		let mut ds = device.accept().await.unwrap();
		assert_eq!(
			read_message(&mut ds).await.unwrap().unwrap(),
			b"a subscription"
		);
		write_message(&mut ds, b"a sample").await.unwrap();
		assert_eq!(read_message(&mut cs).await.unwrap().unwrap(), b"a sample");
		// Both streams stay open, so the teardown is the only thing in flight: a stream dropped at the
		// same moment queues a reset, and whether that reset makes it out before the transport goes is a
		// race that says nothing about how a close is read.
		drop(device);
		device_driving
			.await
			.unwrap()
			.expect("closing the connection is not a failure");

		// And the client's driver ends without a fault to report, which is what reaches the operator.
		client_driving
			.await
			.unwrap()
			.expect("a closed connection reaches the peer as an ending, not a fault");
	}

	#[tokio::test]
	async fn streams_open_from_each_end() {
		let (mut client, mut device) = paired().await;

		let mut c2d = client.open().await.unwrap();
		write_message(&mut c2d, b"from client").await.unwrap();
		let mut d_in = device.accept().await.unwrap();
		assert_eq!(
			read_message(&mut d_in).await.unwrap().unwrap(),
			b"from client"
		);

		// The device opens a stream without being asked, proving the unsolicited direction.
		let mut d2c = device.open().await.unwrap();
		write_message(&mut d2c, b"from device").await.unwrap();
		let mut c_in = client.accept().await.unwrap();
		assert_eq!(
			read_message(&mut c_in).await.unwrap().unwrap(),
			b"from device"
		);
	}

	#[tokio::test]
	async fn closing_one_stream_leaves_the_others_alive() {
		let (mut client, mut device) = paired().await;

		let mut s1 = client.open().await.unwrap();
		write_message(&mut s1, b"one-a").await.unwrap();
		let mut d1 = device.accept().await.unwrap();
		assert_eq!(read_message(&mut d1).await.unwrap().unwrap(), b"one-a");

		// Open a second stream while the first is still open and mid-conversation.
		let mut s2 = client.open().await.unwrap();
		write_message(&mut s2, b"two-a").await.unwrap();
		let mut d2 = device.accept().await.unwrap();
		assert_eq!(read_message(&mut d2).await.unwrap().unwrap(), b"two-a");

		// Close the first stream; the second, and the connection, stay alive.
		s1.close().await.unwrap();
		drop(s1);
		write_message(&mut s2, b"two-b").await.unwrap();
		assert_eq!(read_message(&mut d2).await.unwrap().unwrap(), b"two-b");
		assert!(read_message(&mut d1).await.unwrap().is_none());
	}
}
