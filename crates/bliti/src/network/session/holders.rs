//! Which open session holds the configuration (CFG): the one served longest, every other being
//! busy.

use std::sync::{
	Arc,
	atomic::{AtomicU64, Ordering},
};

use tokio::sync::watch::{self, error::RecvError};

/// The open sessions, in the order they opened.
#[derive(Clone)]
pub(super) struct Holders {
	open: Arc<watch::Sender<Vec<u64>>>,
	next: Arc<AtomicU64>,
}

impl Holders {
	pub(super) fn new() -> Self {
		Self {
			open: Arc::new(watch::Sender::new(Vec::new())),
			next: Arc::new(AtomicU64::new(0)),
		}
	}

	/// Enter a session as the newest, until the registration is dropped.
	pub(super) fn register(&self) -> Registration {
		let id = self.next.fetch_add(1, Ordering::Relaxed);
		self.open.send_modify(|open| open.push(id));
		Registration {
			holders: self.clone(),
			id,
			open: self.open.subscribe(),
		}
	}
}

/// One open session's place among the others.
pub(super) struct Registration {
	holders: Holders,
	id: u64,
	open: watch::Receiver<Vec<u64>>,
}

impl Registration {
	/// Whether another session holds the configuration.
	pub(super) fn busy(&self) -> bool {
		self.open.borrow().first() != Some(&self.id)
	}

	/// Resolve once a session has opened or closed.
	pub(super) async fn changed(&mut self) -> Result<(), RecvError> {
		self.open.changed().await
	}
}

impl Drop for Registration {
	fn drop(&mut self) {
		self.holders
			.open
			.send_modify(|open| open.retain(|id| *id != self.id));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_session_open_longest_holds_and_the_next_in_order_holds_once_it_closes() {
		let holders = Holders::new();
		let first = holders.register();
		let second = holders.register();
		let third = holders.register();
		assert!(!first.busy());
		assert!(second.busy());
		assert!(third.busy());

		drop(second);
		assert!(!first.busy());
		assert!(third.busy());

		drop(first);
		assert!(!third.busy());
	}
}
