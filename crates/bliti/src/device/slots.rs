//! Which client holds which device transmit slot (CHN, "Several clients at once").
//!
//! A slot per client is what keeps one client leaving from ending another's channel. BlueZ, as a
//! client that is not bonded disconnects, closes the notification socket at the head of each
//! characteristic it had subscribed to, which is the earliest subscriber's rather than its own. With
//! one subscriber to a characteristic, the head is the client leaving.

use std::future::Future;

use bliti_core::slot::{Allocation, COUNT, Slot};
use bluer::Address;
use tokio::sync::Mutex;

/// The client holding each slot, by its address.
///
/// Allocations run one at a time, since freeing a slot means asking whether its holder is still
/// connected, and one running alongside could take the slot between the asking and the freeing.
#[derive(Default)]
pub struct Slots(Mutex<[Option<Address>; COUNT as usize]>);

impl Slots {
	/// Give `client` a slot, the one it already holds where it holds one.
	///
	/// A holder is freed only once no slot is free, and only if `connected` says it has gone: nothing
	/// reports a client leaving before it subscribed, so this is where a slot comes back.
	pub async fn allocate<F, Fut>(&self, client: Address, connected: F) -> Allocation
	where
		F: Fn(Address) -> Fut,
		Fut: Future<Output = bool>,
	{
		let mut held = self.0.lock().await;
		if let Some(slot) = find(held.as_slice(), Some(client)) {
			return Allocation::Given(slot);
		}
		if find(held.as_slice(), None).is_none() {
			for holder in held.iter_mut() {
				let Some(address) = *holder else { continue };
				if !connected(address).await {
					*holder = None;
				}
			}
		}
		match find(held.as_slice(), None) {
			Some(slot) => {
				held[usize::from(slot.number())] = Some(client);
				Allocation::Given(slot)
			}
			None => Allocation::Full,
		}
	}

	/// Whether `slot` was given to `client`.
	pub async fn holds(&self, slot: Slot, client: Address) -> bool {
		self.0.lock().await[usize::from(slot.number())] == Some(client)
	}
}

fn find(held: &[Option<Address>], holder: Option<Address>) -> Option<Slot> {
	let number = held.iter().position(|h| *h == holder)?;
	Slot::new(u8::try_from(number).expect("there are fewer slots than a byte counts"))
}

#[cfg(test)]
mod tests {
	use std::collections::HashSet;

	use super::*;

	fn client(n: u8) -> Address {
		Address::new([0, 0, 0, 0, 0, n])
	}

	async fn allocate(slots: &Slots, who: Address, connected: &HashSet<Address>) -> Allocation {
		slots
			.allocate(who, |address| {
				let up = connected.contains(&address);
				async move { up }
			})
			.await
	}

	#[tokio::test]
	async fn each_client_gets_a_slot_of_its_own() {
		let slots = Slots::default();
		let connected: HashSet<_> = (0..COUNT).map(client).collect();
		let mut given = HashSet::new();
		for n in 0..COUNT {
			let Allocation::Given(slot) = allocate(&slots, client(n), &connected).await else {
				panic!("client {n} was turned away with slots free");
			};
			assert!(given.insert(slot), "slot {slot:?} was given twice");
			assert!(slots.holds(slot, client(n)).await);
		}
	}

	#[tokio::test]
	async fn reading_again_gives_the_same_slot() {
		let slots = Slots::default();
		let connected = HashSet::from([client(1), client(2)]);
		let first = allocate(&slots, client(1), &connected).await;
		allocate(&slots, client(2), &connected).await;
		assert_eq!(allocate(&slots, client(1), &connected).await, first);
	}

	#[tokio::test]
	async fn a_full_device_turns_the_next_client_away() {
		let slots = Slots::default();
		let connected: HashSet<_> = (0..=COUNT).map(client).collect();
		for n in 0..COUNT {
			allocate(&slots, client(n), &connected).await;
		}
		assert_eq!(
			allocate(&slots, client(COUNT), &connected).await,
			Allocation::Full
		);
	}

	#[tokio::test]
	async fn a_slot_comes_back_once_its_holder_has_gone() {
		let slots = Slots::default();
		let mut connected: HashSet<_> = (0..=COUNT).map(client).collect();
		for n in 0..COUNT {
			allocate(&slots, client(n), &connected).await;
		}
		connected.remove(&client(3));
		let Allocation::Given(slot) = allocate(&slots, client(COUNT), &connected).await else {
			panic!("the slot of a client that left did not come back");
		};
		assert!(slots.holds(slot, client(COUNT)).await);
		assert!(!slots.holds(slot, client(3)).await);
	}

	#[tokio::test]
	async fn a_slot_is_not_held_by_a_client_it_was_not_given_to() {
		let slots = Slots::default();
		let connected = HashSet::from([client(1)]);
		let Allocation::Given(slot) = allocate(&slots, client(1), &connected).await else {
			panic!("turned away from an empty device");
		};
		assert!(!slots.holds(slot, client(2)).await);
	}
}
