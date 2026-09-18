//! Bytes waiting to go out to a transport that takes them a few at a time.

use std::{
	io,
	pin::Pin,
	task::{Context, Poll},
};

use futures::AsyncWrite;

/// Bytes written by a layer of this stack but not yet accepted by the one beneath it.
///
/// An `AsyncWrite` is free to accept part of what it is given, so anything layered over one has to
/// remember how far through its own buffer it got and resume there. Both wrappers of this stack do,
/// and both do it the same way, so the loop lives here rather than once per layer.
///
/// Draining keeps the buffer's capacity, so a connection settles on one allocation per direction
/// rather than one per message.
#[derive(Debug, Default)]
pub struct WriteBacklog {
	buf: Vec<u8>,
	sent: usize,
}

impl WriteBacklog {
	/// Add bytes to the back of the backlog.
	pub fn extend(&mut self, bytes: &[u8]) {
		self.buf.extend_from_slice(bytes);
	}

	/// Write what is waiting to `inner`. `Ready(Ok(()))` only once the backlog is fully drained.
	pub fn poll_drain<W: AsyncWrite + Unpin>(
		&mut self,
		inner: &mut W,
		cx: &mut Context<'_>,
	) -> Poll<io::Result<()>> {
		while self.sent < self.buf.len() {
			let n =
				std::task::ready!(Pin::new(&mut *inner).poll_write(cx, &self.buf[self.sent..]))?;
			if n == 0 {
				return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
			}
			self.sent += n;
		}
		self.buf.clear();
		self.sent = 0;
		Poll::Ready(Ok(()))
	}
}

#[cfg(test)]
mod tests {
	use std::task::Waker;

	use super::*;

	/// An `AsyncWrite` that accepts at most `take` bytes per call, as a transport delivering in chunks
	/// no larger than a negotiated attribute size does.
	struct Trickle {
		taken: Vec<u8>,
		take: usize,
	}

	impl AsyncWrite for Trickle {
		fn poll_write(
			mut self: Pin<&mut Self>,
			_cx: &mut Context<'_>,
			buf: &[u8],
		) -> Poll<io::Result<usize>> {
			let n = buf.len().min(self.take);
			let chunk = buf[..n].to_vec();
			self.taken.extend_from_slice(&chunk);
			Poll::Ready(Ok(n))
		}

		fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}

		fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
			Poll::Ready(Ok(()))
		}
	}

	/// A transport that accepts a few bytes at a time gets everything, in order, across the several
	/// writes it takes. A drain that resumed at the wrong offset would drop or repeat bytes, which a
	/// length-prefixed reader downstream reads as framing and never recovers from.
	#[test]
	fn a_backlog_drains_across_a_transport_that_takes_a_few_bytes_at_a_time() {
		let mut inner = Trickle {
			taken: Vec::new(),
			take: 3,
		};
		let mut backlog = WriteBacklog::default();
		backlog.extend(b"a length prefix");
		backlog.extend(b" and then a body");

		let mut cx = Context::from_waker(Waker::noop());
		assert!(matches!(
			backlog.poll_drain(&mut inner, &mut cx),
			Poll::Ready(Ok(()))
		));
		assert_eq!(inner.taken, b"a length prefix and then a body");

		// Drained, so the next message starts from the front of the same allocation.
		backlog.extend(b"the next one");
		assert!(matches!(
			backlog.poll_drain(&mut inner, &mut cx),
			Poll::Ready(Ok(()))
		));
		assert_eq!(inner.taken, b"a length prefix and then a bodythe next one");
	}

	/// A transport that will take nothing at all is a failed write, not a spin.
	#[test]
	fn a_transport_that_accepts_nothing_is_an_error() {
		let mut inner = Trickle {
			taken: Vec::new(),
			take: 0,
		};
		let mut backlog = WriteBacklog::default();
		backlog.extend(b"anything");

		let mut cx = Context::from_waker(Waker::noop());
		let Poll::Ready(Err(err)) = backlog.poll_drain(&mut inner, &mut cx) else {
			panic!("a transport that accepts nothing must fail the write")
		};
		assert_eq!(err.kind(), io::ErrorKind::WriteZero);
	}
}
