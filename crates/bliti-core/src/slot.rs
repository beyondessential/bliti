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

/// The smallest ATT_MTU a link can have, which every link starts at (CHN, "Transport").
pub const MIN_ATT_MTU: u16 = 23;

/// The most a write or a notification may carry on a link whose ATT_MTU is `mtu`: the MTU less the
/// three-byte ATT header (CHN, "Transport"). An MTU below the smallest a link can have is taken as
/// that smallest.
pub fn payload(mtu: usize) -> usize {
	mtu.max(MIN_ATT_MTU.into()) - 3
}

/// The device's decision on a client reading the allocation characteristic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allocation {
	Given(Slot),
	/// Every slot is held by another connected client.
	Full,
}

impl Allocation {
	/// What the read answers, carrying the ATT_MTU negotiated with the reading client where a slot is
	/// given.
	pub fn to_value(self, mtu: u16) -> Vec<u8> {
		match self {
			Self::Given(slot) => {
				let [high, low] = mtu.to_be_bytes();
				vec![slot.0, high, low]
			}
			Self::Full => Vec::new(),
		}
	}
}

/// What a client reads from the allocation characteristic: its slot and the link's ATT_MTU, or that
/// every slot is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Allocated {
	Given { slot: Slot, mtu: u16 },
	Full,
}

impl Allocated {
	pub fn from_value(value: &[u8]) -> Result<Self, BadAllocation> {
		match value {
			[] => Ok(Self::Full),
			[number, high, low] => Slot::new(*number)
				.map(|slot| Self::Given {
					slot,
					mtu: u16::from_be_bytes([*high, *low]),
				})
				.ok_or(BadAllocation::NoSuchSlot(*number)),
			_ => Err(BadAllocation::Length(value.len())),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BadAllocation {
	#[error("the device gave slot {0}, which it does not offer")]
	NoSuchSlot(u8),
	#[error("the device's slot allocation is {0} bytes rather than three")]
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
	fn allocation_round_trips_with_the_mtu() {
		for slot in Slot::all() {
			assert_eq!(
				Allocated::from_value(&Allocation::Given(slot).to_value(517)),
				Ok(Allocated::Given { slot, mtu: 517 })
			);
		}
		assert_eq!(Allocation::Given(Slot(2)).to_value(517), [2, 0x02, 0x05]);
		assert_eq!(
			Allocated::from_value(&Allocation::Full.to_value(517)),
			Ok(Allocated::Full)
		);
	}

	#[test]
	fn a_slot_the_device_does_not_offer_is_refused() {
		assert_eq!(
			Allocated::from_value(&[COUNT, 0, 23]),
			Err(BadAllocation::NoSuchSlot(COUNT))
		);
		assert_eq!(Allocated::from_value(&[0]), Err(BadAllocation::Length(1)));
	}

	#[test]
	fn a_payload_is_the_mtu_less_the_att_header() {
		assert_eq!(payload(517), 514);
		assert_eq!(payload(23), 20);
		assert_eq!(payload(0), 20, "no link is smaller than the smallest MTU");
	}
}
