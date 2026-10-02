//! The device transmit slots of CHN, "Transport": a notify characteristic for each connected client,
//! and the allocation a client reads to learn which one is its own.

use uuid::Uuid;

/// The GATT characteristic a client reads to be given its slot (CHN, "Transport").
pub const CHARACTERISTIC_UUID_ALLOCATION: Uuid =
	Uuid::from_u128(0x7f0c7b93_87a1_40d3_abcb_f8381c30824f);

/// How many slots a device offers, and so how many clients it serves at once.
pub const COUNT: u8 = 8;

/// The device transmit UUIDs with the last byte cleared; a slot's UUID ends in its number.
const DEVICE_TX_BASE: u128 = 0xa7aabad6_3fc2_4c9b_953b_03a70a193e00;

/// One of the device transmit characteristics a device notifies on, given to one client at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot(u8);

impl Slot {
	pub fn new(number: u8) -> Option<Self> {
		(number < COUNT).then_some(Self(number))
	}

	/// Every slot a device offers, in order.
	pub fn all() -> impl Iterator<Item = Self> {
		(0..COUNT).map(Self)
	}

	pub fn number(self) -> u8 {
		self.0
	}

	/// The device transmit characteristic this slot is.
	pub fn uuid(self) -> Uuid {
		Uuid::from_u128(DEVICE_TX_BASE | u128::from(self.0))
	}
}

/// What a read of the allocation characteristic answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allocation {
	Given(Slot),
	/// Every slot is held by another connected client.
	Full,
}

impl Allocation {
	pub fn to_value(self) -> Vec<u8> {
		match self {
			Self::Given(slot) => vec![slot.0],
			Self::Full => Vec::new(),
		}
	}

	pub fn from_value(value: &[u8]) -> Result<Self, BadAllocation> {
		match value {
			[] => Ok(Self::Full),
			[number] => Slot::new(*number)
				.map(Self::Given)
				.ok_or(BadAllocation::NoSuchSlot(*number)),
			_ => Err(BadAllocation::Length(value.len())),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadAllocation {
	#[error("the device gave slot {0}, which it does not offer")]
	NoSuchSlot(u8),
	#[error("the device's slot allocation is {0} bytes rather than one")]
	Length(usize),
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn slot_uuids_end_in_the_slot_number() {
		let uuids: Vec<_> = Slot::all().map(|slot| slot.uuid().to_string()).collect();
		assert_eq!(uuids.len(), usize::from(COUNT));
		assert_eq!(uuids[0], "a7aabad6-3fc2-4c9b-953b-03a70a193e00");
		assert_eq!(uuids[7], "a7aabad6-3fc2-4c9b-953b-03a70a193e07");
	}

	#[test]
	fn allocation_round_trips() {
		for allocation in Slot::all().map(Allocation::Given).chain([Allocation::Full]) {
			assert_eq!(
				Allocation::from_value(&allocation.to_value()),
				Ok(allocation)
			);
		}
	}

	#[test]
	fn a_slot_the_device_does_not_offer_is_refused() {
		assert_eq!(
			Allocation::from_value(&[COUNT]),
			Err(BadAllocation::NoSuchSlot(COUNT))
		);
		assert_eq!(
			Allocation::from_value(&[0, 0]),
			Err(BadAllocation::Length(2))
		);
	}
}
