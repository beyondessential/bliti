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

/// Errno values meaning nothing answered at the address. `io::ErrorKind` does not name either, so
/// they are matched by number.
const ENXIO: i32 = 6;
const EREMOTEIO: i32 = 121;

/// Why a register could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// No gauge can be reached here, which is the ordinary case on a machine with no such hardware and
	/// is not a fault.
	#[error("no I2C gauge")]
	NoDevice,
	/// A gauge answered and the exchange failed.
	#[error("{0}")]
	Failed(std::io::Error),
}

impl Error {
	/// Tell hardware that is not there from hardware that is and did not work (NFO).
	///
	/// Absent covers more than a missing bus node. An ordinary laptop carries a dozen I2C buses for its
	/// display connectors, all of them root-only, so a bus that will not open says nothing about a
	/// gauge; and a bus that opens but whose address goes unacknowledged is a machine whose I2C is
	/// wired to something else entirely. Neither is a backup board in difficulty.
	fn from_io(err: std::io::Error) -> Self {
		match err.kind() {
			std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => Self::NoDevice,
			_ => match err.raw_os_error() {
				Some(ENXIO | EREMOTEIO) => Self::NoDevice,
				_ => Self::Failed(err),
			},
		}
	}
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
			.map_err(Error::from_io)?;

		// SAFETY: I2C_SLAVE is the kernel's opcode for addressing a device on an I2C bus, and it takes
		// the address as an integer rather than through a pointer. A 7-bit address is in range.
		unsafe {
			ioctl(
				file.as_fd(),
				IntegerSetter::<I2C_SLAVE>::new_usize(usize::from(address)),
			)
		}
		.map_err(|errno| Error::from_io(std::io::Error::from(errno)))?;

		Ok(Self { file })
	}

	/// Read one 16-bit register, as the device sends it: most significant byte first.
	///
	/// The exchange is a write of the register number followed by a read, which is what the kernel's
	/// SMBus word read reduces to on a device that holds its register pointer between the two.
	pub fn read_word(&mut self, register: u8) -> Result<u16, Error> {
		self.file.write_all(&[register]).map_err(Error::from_io)?;
		let mut bytes = [0u8; 2];
		self.file.read_exact(&mut bytes).map_err(Error::from_io)?;
		Ok(u16::from_be_bytes(bytes))
	}
}

#[cfg(test)]
mod tests {
	use std::io::{Error as IoError, ErrorKind};

	use super::*;

	/// A laptop carries root-only I2C buses for its display connectors. One that will not open is not
	/// a gauge in difficulty, and reporting it as one would keep the device off the battery its
	/// operating system can answer for (NFO).
	#[test]
	fn a_bus_that_will_not_open_is_no_gauge() {
		for kind in [ErrorKind::NotFound, ErrorKind::PermissionDenied] {
			assert!(matches!(
				Error::from_io(IoError::new(kind, "denied")),
				Error::NoDevice
			));
		}
	}

	/// Nothing acknowledged at the gauge's address, so no backup board is fitted here.
	#[test]
	fn an_unacknowledged_address_is_no_gauge() {
		for errno in [ENXIO, EREMOTEIO] {
			assert!(matches!(
				Error::from_io(IoError::from_raw_os_error(errno)),
				Error::NoDevice
			));
		}
	}

	/// A gauge that answered and then failed is a fault nobody can see from outside the case (NFO).
	#[test]
	fn an_exchange_that_failed_is_a_fault() {
		let err = Error::from_io(IoError::from_raw_os_error(5));
		assert!(matches!(err, Error::Failed(_)), "{err:?}");
	}
}
