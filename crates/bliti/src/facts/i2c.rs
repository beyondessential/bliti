//! Talking to devices on an I2C bus, over the kernel's character device.
//!
//! Hand-rolled rather than taken from a crate: it is one ioctl, `I2C_RDWR`, over a pair of plain C
//! structs. The ABI has been stable for as long as Linux has had I2C, and the alternative crates
//! either wrap a C library, which the cross-build would then have to carry, or wrap exactly this.
//!
//! Every exchange is a single `I2C_RDWR`, which the kernel carries out with the bus held throughout.
//! A register select and the read that follows it therefore can't be split by another process
//! reading the same bus, as provisioning's hardware check does.

use std::{fs::File, io, os::fd::AsFd as _, path::Path, thread, time::Duration};

use embedded_hal::{
	delay::DelayNs,
	i2c::{ErrorKind, ErrorType, I2c, NoAcknowledgeSource, Operation, SevenBitAddress},
};
use rustix::ioctl::{Opcode, Updater, ioctl};

const I2C_RDWR: Opcode = 0x0707;
/// The message is a read; without it, a write.
const I2C_M_RD: u16 = 0x0001;
/// The most messages the kernel takes in one transfer.
const MAX_MESSAGES: usize = 42;

/// Errno values meaning nothing answered at the address. `io::ErrorKind` does not name either, so
/// they are matched by number.
const ENXIO: i32 = 6;
const EREMOTEIO: i32 = 121;

#[repr(C)]
struct Message {
	addr: u16,
	flags: u16,
	len: u16,
	buf: *mut u8,
}

#[repr(C)]
struct Transfer {
	msgs: *mut Message,
	nmsgs: u32,
}

/// Why an exchange could not be made.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// The bus isn't there.
	#[error("no such I2C bus")]
	NoBus,
	/// The bus is there and may not be opened from here.
	#[error("not permitted to open the I2C bus")]
	Denied,
	/// Nothing acknowledged the address.
	#[error("nothing answered at the address")]
	NoAnswer,
	/// A device answered and the exchange failed.
	#[error("{0}")]
	Failed(io::Error),
}

impl Error {
	fn from_open(err: io::Error) -> Self {
		match err.kind() {
			io::ErrorKind::NotFound => Self::NoBus,
			io::ErrorKind::PermissionDenied => Self::Denied,
			_ => Self::Failed(err),
		}
	}

	fn from_transfer(err: io::Error) -> Self {
		match err.raw_os_error() {
			Some(ENXIO | EREMOTEIO) => Self::NoAnswer,
			_ => Self::Failed(err),
		}
	}
}

impl embedded_hal::i2c::Error for Error {
	fn kind(&self) -> ErrorKind {
		match self {
			Self::NoAnswer => ErrorKind::NoAcknowledge(NoAcknowledgeSource::Unknown),
			_ => ErrorKind::Other,
		}
	}
}

/// One bus, open for exchanges with any device on it.
#[derive(Debug)]
pub struct Bus {
	file: File,
}

impl Bus {
	pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
		let file = File::options()
			.read(true)
			.write(true)
			.open(path)
			.map_err(Error::from_open)?;
		Ok(Self { file })
	}

	/// Carry out `operations` with the device at `address`, as one transfer.
	pub fn transfer(&self, address: u16, operations: &mut [Operation<'_>]) -> Result<(), Error> {
		let mut runs = runs(operations);
		if runs.len() > MAX_MESSAGES {
			return Err(Error::Failed(io::Error::new(
				io::ErrorKind::InvalidInput,
				"too many messages for one I2C transfer",
			)));
		}
		let mut messages = runs
			.iter_mut()
			.map(|run| message(address, run))
			.collect::<Result<Vec<_>, _>>()?;
		let mut transfer = Transfer {
			msgs: messages.as_mut_ptr(),
			nmsgs: messages.len() as u32,
		};
		// SAFETY: I2C_RDWR takes a `struct i2c_rdwr_ioctl_data`, which `Transfer` lays out. Each
		// message points into a buffer of `runs` at least `len` long, and both outlive the call.
		unsafe {
			ioctl(
				self.file.as_fd(),
				Updater::<I2C_RDWR, _>::new(&mut transfer),
			)
		}
		.map_err(|errno| Error::from_transfer(io::Error::from(errno)))?;
		scatter(&runs, operations);
		Ok(())
	}

	/// Read one 16-bit register, as the device sends it: most significant byte first.
	pub fn read_word(&self, address: u16, register: u8) -> Result<u16, Error> {
		let mut bytes = [0u8; 2];
		self.transfer(
			address,
			&mut [Operation::Write(&[register]), Operation::Read(&mut bytes)],
		)?;
		Ok(u16::from_be_bytes(bytes))
	}
}

impl ErrorType for Bus {
	type Error = Error;
}

impl I2c<SevenBitAddress> for Bus {
	fn transaction(
		&mut self,
		address: SevenBitAddress,
		operations: &mut [Operation<'_>],
	) -> Result<(), Error> {
		Bus::transfer(self, u16::from(address), operations)
	}
}

/// Waiting, for the drivers, by putting the thread to sleep.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sleep;

impl DelayNs for Sleep {
	fn delay_ns(&mut self, ns: u32) {
		thread::sleep(Duration::from_nanos(u64::from(ns)));
	}
}

/// One message's worth of operations: a read or a write, and its bytes.
#[derive(Debug, PartialEq, Eq)]
struct Run {
	read: bool,
	bytes: Vec<u8>,
}

/// The operations as messages. Adjacent operations of one kind go out as one message, since
/// embedded-hal has them sent without a restart between them.
fn runs(operations: &[Operation<'_>]) -> Vec<Run> {
	let mut runs: Vec<Run> = Vec::new();
	for operation in operations {
		let (read, bytes) = match operation {
			Operation::Read(buffer) => (true, vec![0; buffer.len()]),
			Operation::Write(bytes) => (false, bytes.to_vec()),
		};
		match runs.last_mut() {
			Some(last) if last.read == read => last.bytes.extend(bytes),
			_ => runs.push(Run { read, bytes }),
		}
	}
	runs
}

fn message(address: u16, run: &mut Run) -> Result<Message, Error> {
	let len = u16::try_from(run.bytes.len()).map_err(|_| {
		Error::Failed(io::Error::new(
			io::ErrorKind::InvalidInput,
			"I2C message too long",
		))
	})?;
	Ok(Message {
		addr: address,
		flags: if run.read { I2C_M_RD } else { 0 },
		len,
		buf: run.bytes.as_mut_ptr(),
	})
}

/// Hand what the reads brought back to the operations that asked for it.
fn scatter(runs: &[Run], operations: &mut [Operation<'_>]) {
	let mut read = runs
		.iter()
		.filter(|run| run.read)
		.flat_map(|run| &run.bytes);
	for operation in operations {
		if let Operation::Read(buffer) = operation {
			for (byte, from) in buffer.iter_mut().zip(&mut read) {
				*byte = *from;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use std::io::{Error as IoError, ErrorKind as IoKind};

	use super::*;

	#[test]
	fn a_register_read_is_a_write_then_a_read() {
		let mut buffer = [0u8; 2];
		let operations = [Operation::Write(&[0x04]), Operation::Read(&mut buffer)];
		let mut runs = runs(&operations);
		assert_eq!(
			runs,
			[
				Run {
					read: false,
					bytes: vec![0x04]
				},
				Run {
					read: true,
					bytes: vec![0, 0]
				},
			]
		);

		let messages: Vec<_> = runs
			.iter_mut()
			.map(|run| message(0x36, run).unwrap())
			.collect();
		assert_eq!(
			messages
				.iter()
				.map(|m| (m.addr, m.flags, m.len))
				.collect::<Vec<_>>(),
			[(0x36, 0, 1), (0x36, I2C_M_RD, 2)]
		);
	}

	#[test]
	fn adjacent_operations_of_one_kind_go_out_as_one_message() {
		let (mut first, mut second) = ([0u8; 1], [0u8; 2]);
		let operations = [
			Operation::Write(&[0x1d]),
			Operation::Write(&[0x01, 0x02]),
			Operation::Read(&mut first),
			Operation::Read(&mut second),
		];
		let runs = runs(&operations);
		assert_eq!(
			runs.iter()
				.map(|run| (run.read, run.bytes.len()))
				.collect::<Vec<_>>(),
			[(false, 3), (true, 3)]
		);
		assert_eq!(runs[0].bytes, [0x1d, 0x01, 0x02]);
	}

	#[test]
	fn what_a_read_brings_back_reaches_its_operation() {
		let (mut first, mut second) = ([0u8; 1], [0u8; 2]);
		let mut operations = [
			Operation::Write(&[0x0d]),
			Operation::Read(&mut first),
			Operation::Read(&mut second),
		];
		let mut runs = runs(&operations);
		runs[1].bytes.copy_from_slice(&[0xaa, 0xbb, 0xcc]);
		scatter(&runs, &mut operations);
		assert_eq!(first, [0xaa]);
		assert_eq!(second, [0xbb, 0xcc]);
	}

	/// A bus that will not open for permission is a precondition not met, which the HAT sensors
	/// report as skipped and not as broken (NFO).
	#[test]
	fn a_denied_bus_is_told_from_a_missing_one() {
		assert!(matches!(
			Error::from_open(IoError::new(IoKind::NotFound, "gone")),
			Error::NoBus
		));
		assert!(matches!(
			Error::from_open(IoError::new(IoKind::PermissionDenied, "denied")),
			Error::Denied
		));
	}

	#[test]
	fn an_unacknowledged_address_is_no_answer() {
		for errno in [ENXIO, EREMOTEIO] {
			assert!(matches!(
				Error::from_transfer(IoError::from_raw_os_error(errno)),
				Error::NoAnswer
			));
		}
	}

	/// A device that answered and then failed is a fault nobody can see from outside the case (NFO).
	#[test]
	fn an_exchange_that_failed_is_a_fault() {
		let err = Error::from_transfer(IoError::from_raw_os_error(5));
		assert!(matches!(err, Error::Failed(_)), "{err:?}");
	}
}
