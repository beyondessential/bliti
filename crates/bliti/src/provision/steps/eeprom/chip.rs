//! The onsemi CAT24C256 on the ID bus: 32 KiB, read and written over raw I²C (PEEP).

use std::{thread, time::Duration};

use embedded_hal::i2c::I2c;

use crate::facts::i2c;

/// Where the HAT EEPROM answers.
const ADDRESS: u8 = 0x50;

/// What the chip holds.
pub const SIZE: usize = 32 * 1024;

/// How much the chip takes in one write cycle.
const PAGE: usize = 64;

/// The most one read message carries: i2c-dev's limit.
const READ: usize = 8192;

/// How often the chip is asked whether a write cycle has finished, and how far apart. A cycle takes a
/// few milliseconds.
const POLLS: u32 = 100;
const POLL: Duration = Duration::from_millis(1);

/// Read the first `len` bytes.
pub fn read<B: I2c<Error = i2c::Error>>(bus: &mut B, len: usize) -> Result<Vec<u8>, i2c::Error> {
	if len > SIZE {
		return Err(too_long());
	}
	let mut bytes = vec![0; len];
	for (index, chunk) in bytes.chunks_mut(READ).enumerate() {
		bus.write_read(ADDRESS, &address(index * READ), chunk)?;
	}
	Ok(bytes)
}

/// Write `image` from the first byte, a page at a time, waiting out each page's write cycle. Fails at
/// the first page the chip does not take, which is every page while it is write-protected.
pub fn write<B: I2c<Error = i2c::Error>>(bus: &mut B, image: &[u8]) -> Result<(), i2c::Error> {
	if image.len() > SIZE {
		return Err(too_long());
	}
	for (index, page) in image.chunks(PAGE).enumerate() {
		let at = index * PAGE;
		let mut message = address(at).to_vec();
		message.extend_from_slice(page);
		bus.write(ADDRESS, &message)?;
		finish(bus, at)?;
	}
	Ok(())
}

/// Wait until the chip answers again, which it does once its write cycle ends.
fn finish<B: I2c<Error = i2c::Error>>(bus: &mut B, at: usize) -> Result<(), i2c::Error> {
	let mut last = Ok(());
	for _ in 0..POLLS {
		last = bus.write(ADDRESS, &address(at));
		if last.is_ok() {
			return Ok(());
		}
		thread::sleep(POLL);
	}
	last
}

/// A byte address as the chip takes it: two bytes, most significant first.
fn address(at: usize) -> [u8; 2] {
	u16::try_from(at)
		.expect("an address within the chip")
		.to_be_bytes()
}

fn too_long() -> i2c::Error {
	i2c::Error::Failed(std::io::Error::new(
		std::io::ErrorKind::InvalidInput,
		"longer than the EEPROM",
	))
}

#[cfg(test)]
pub mod tests {
	use std::sync::{Arc, Mutex};

	use embedded_hal::i2c::{ErrorType, Operation};

	use super::*;

	/// A stand-in chip, with its write protection and its write cycle.
	#[derive(Clone)]
	pub struct Chip(pub Arc<Mutex<State>>);

	pub struct State {
		pub memory: Vec<u8>,
		/// Whether the write-protect pin is high, so data bytes are refused.
		pub protected: bool,
		/// How many address-only writes are refused after each page, as the write cycle runs.
		pub busy_after_write: u32,
		busy: u32,
		pointer: usize,
		/// Every message the chip took, as its length.
		pub messages: Vec<(bool, usize)>,
	}

	impl Chip {
		pub fn blank() -> Self {
			Self::holding(&[])
		}

		pub fn holding(bytes: &[u8]) -> Self {
			let mut memory = vec![0xff; SIZE];
			memory[..bytes.len()].copy_from_slice(bytes);
			Self(Arc::new(Mutex::new(State {
				memory,
				protected: false,
				busy_after_write: 0,
				busy: 0,
				pointer: 0,
				messages: Vec::new(),
			})))
		}

		pub fn state(&self) -> std::sync::MutexGuard<'_, State> {
			self.0.lock().unwrap()
		}
	}

	impl ErrorType for Chip {
		type Error = i2c::Error;
	}

	impl I2c for Chip {
		fn transaction(
			&mut self,
			address: u8,
			operations: &mut [Operation<'_>],
		) -> Result<(), i2c::Error> {
			assert_eq!(address, ADDRESS);
			let mut state = self.0.lock().unwrap();
			for operation in operations {
				match operation {
					Operation::Write(bytes) => {
						state.messages.push((false, bytes.len()));
						if state.busy > 0 {
							state.busy -= 1;
							return Err(i2c::Error::NoAnswer);
						}
						let [high, low, data @ ..] = bytes else {
							panic!("a write without an address");
						};
						state.pointer = usize::from(u16::from_be_bytes([*high, *low]));
						if data.is_empty() {
							continue;
						}
						if state.protected {
							return Err(i2c::Error::NoAnswer);
						}
						assert!(
							data.len() <= PAGE && state.pointer % PAGE == 0,
							"a write runs past a page"
						);
						let at = state.pointer;
						state.memory[at..at + data.len()].copy_from_slice(data);
						state.busy = state.busy_after_write;
					}
					Operation::Read(buffer) => {
						state.messages.push((true, buffer.len()));
						assert!(buffer.len() <= READ, "a read longer than a message takes");
						let at = state.pointer;
						buffer.copy_from_slice(&state.memory[at..at + buffer.len()]);
					}
				}
			}
			Ok(())
		}
	}

	// spec: PEEP
	#[test]
	fn what_is_written_is_read_back() {
		let mut chip = Chip::blank();
		let image: Vec<u8> = (0..200u8).collect();
		write(&mut chip, &image).unwrap();
		assert_eq!(read(&mut chip, 200).unwrap(), image);
		assert_eq!(chip.state().memory[200], 0xff);
	}

	#[test]
	fn a_write_goes_a_page_at_a_time_from_the_first_byte() {
		let mut chip = Chip::blank();
		write(&mut chip, &[1; 130]).unwrap();
		let sizes: Vec<_> = chip
			.state()
			.messages
			.iter()
			.filter(|(_, len)| *len > 2)
			.map(|(_, len)| *len)
			.collect();
		assert_eq!(sizes, [66, 66, 4]);
	}

	#[test]
	fn a_read_longer_than_a_message_goes_in_chunks() {
		let mut chip = Chip::holding(&[7; 10]);
		let bytes = read(&mut chip, SIZE).unwrap();
		assert_eq!(bytes.len(), SIZE);
		assert_eq!(&bytes[..10], [7; 10]);
		let reads: Vec<_> = chip
			.state()
			.messages
			.iter()
			.filter(|(read, _)| *read)
			.map(|(_, len)| *len)
			.collect();
		assert_eq!(reads, [READ; 4]);
	}

	/// While the write-protect pin is high the chip refuses the data bytes, and nothing changes.
	// spec: PEEP
	#[test]
	fn a_protected_chip_refuses_a_write_and_keeps_its_contents() {
		let mut chip = Chip::holding(&[9; 4]);
		chip.state().protected = true;
		assert!(matches!(
			write(&mut chip, &[1; 100]),
			Err(i2c::Error::NoAnswer)
		));
		assert_eq!(read(&mut chip, 4).unwrap(), [9; 4]);
	}

	#[test]
	fn a_write_cycle_is_waited_out_before_the_next_page() {
		let mut chip = Chip::blank();
		chip.state().busy_after_write = 3;
		let image: Vec<u8> = (0..150u8).collect();
		write(&mut chip, &image).unwrap();
		assert_eq!(read(&mut chip, 150).unwrap(), image);
	}

	#[test]
	fn a_chip_that_never_finishes_a_cycle_fails_the_write() {
		let mut chip = Chip::blank();
		chip.state().busy_after_write = POLLS + 10;
		assert!(write(&mut chip, &[1; 10]).is_err());
	}

	#[test]
	fn more_than_the_chip_holds_is_refused() {
		let mut chip = Chip::blank();
		assert!(write(&mut chip, &vec![0; SIZE + 1]).is_err());
		assert!(read(&mut chip, SIZE + 1).is_err());
	}
}
