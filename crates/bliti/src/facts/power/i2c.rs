//! Reading a register from an I2C device, over the kernel's character device.
//!
//! Hand-rolled rather than taken from a crate: it is one ioctl to say which device on the bus is
//! being addressed, then an ordinary write of the register number and a read of its value. The ABI
//! has been stable for as long as Linux has had I2C, and the alternative crates either wrap a C
//! library, which the cross-build would then have to carry, or wrap exactly this.

use std::{
	fs::File,
	io::{Read as _, Write as _},
	os::fd::AsFd as _,
	path::Path,
};

use rustix::ioctl::{IntegerSetter, Opcode, ioctl};

/// Address the device at this address for subsequent reads and writes on the bus.
const I2C_SLAVE: Opcode = 0x0703;

/// Why a register could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// There is no I2C bus here at all, which is the ordinary case on a machine with no such hardware
	/// and is not a fault.
	#[error("no I2C bus")]
	NoDevice,
	/// The bus is there and the exchange failed.
	#[error("{0}")]
	Failed(#[from] std::io::Error),
}

/// One bus, addressed at one device.
#[derive(Debug)]
pub struct Bus {
	file: File,
}

impl Bus {
	/// Open a bus and address a device on it.
	pub fn open(path: impl AsRef<Path>, address: u16) -> Result<Self, Error> {
		let file = File::options()
			.read(true)
			.write(true)
			.open(path)
			.map_err(|err| match err.kind() {
				std::io::ErrorKind::NotFound => Error::NoDevice,
				_ => Error::Failed(err),
			})?;

		// SAFETY: I2C_SLAVE is the kernel's opcode for addressing a device on an I2C bus, and it takes
		// the address as an integer rather than through a pointer. A 7-bit address is in range.
		unsafe {
			ioctl(
				file.as_fd(),
				IntegerSetter::<I2C_SLAVE>::new_usize(usize::from(address)),
			)
		}
		.map_err(|errno| Error::Failed(std::io::Error::from(errno)))?;

		Ok(Self { file })
	}

	/// Read one 16-bit register, as the device sends it: most significant byte first.
	///
	/// The exchange is a write of the register number followed by a read, which is what the kernel's
	/// SMBus word read reduces to on a device that holds its register pointer between the two.
	pub fn read_word(&mut self, register: u8) -> Result<u16, Error> {
		self.file.write_all(&[register])?;
		let mut bytes = [0u8; 2];
		self.file.read_exact(&mut bytes)?;
		Ok(u16::from_be_bytes(bytes))
	}
}
