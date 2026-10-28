//! Reading one GPIO line by the name the kernel gives it.
//!
//! By name rather than by number, because the pin header is `gpiochip0` on some kernels and
//! `gpiochip4` on others, and a hardcoded number quietly reads a different pin on the wrong one.
//!
//! Hand-rolled on the kernel's character-device ABI for the same reason as the I2C side: it is two
//! ioctls over a pair of plain C structs, and the crates that wrap it either bring a C library the
//! cross-build would have to carry or wrap exactly this.

use std::{fs::File, io, os::fd::AsFd as _};

use rustix::ioctl::{Opcode, Updater, ioctl};

const GPIO_GET_LINEINFO: Opcode = 0xc048_b402;
const GPIO_GET_LINEHANDLE: Opcode = 0xc16c_b403;
const GPIOHANDLE_GET_LINE_VALUES: Opcode = 0xc040_b408;
const GPIOHANDLE_REQUEST_INPUT: u32 = 1;

/// How many lines a handle request can carry. Fixed by the ABI.
const HANDLE_LINES: usize = 64;

#[repr(C)]
#[derive(Clone, Copy)]
struct LineInfo {
	offset: u32,
	flags: u32,
	name: [u8; 32],
	consumer: [u8; 32],
}

#[repr(C)]
struct HandleRequest {
	offsets: [u32; HANDLE_LINES],
	flags: u32,
	defaults: [u8; HANDLE_LINES],
	consumer: [u8; 32],
	lines: u32,
	fd: i32,
}

#[repr(C)]
struct HandleData {
	values: [u8; HANDLE_LINES],
}

/// Whether the named line is high, searching every chip for one that carries it.
pub fn read_by_name(name: &str) -> io::Result<bool> {
	let mut chips: Vec<_> = std::fs::read_dir("/dev")?
		.flatten()
		.map(|entry| entry.path())
		.filter(|path| {
			path.file_name()
				.and_then(|name| name.to_str())
				.is_some_and(|name| name.starts_with("gpiochip"))
		})
		.collect();
	// A stable order, so the same line is found on every sample.
	chips.sort();

	for chip in chips {
		let Ok(file) = File::open(&chip) else {
			continue;
		};
		let Some(offset) = find_line(&file, name) else {
			continue;
		};
		return read_line(&file, offset);
	}
	Err(io::Error::new(
		io::ErrorKind::NotFound,
		format!("no GPIO line named {name}"),
	))
}

/// The offset of the named line on this chip, where it carries one.
fn find_line(chip: &File, name: &str) -> Option<u32> {
	// The ABI gives no count without another ioctl, and every chip this runs on is far short of this.
	for offset in 0..64u32 {
		let mut info = LineInfo {
			offset,
			flags: 0,
			name: [0; 32],
			consumer: [0; 32],
		};
		// SAFETY: the opcode is the kernel's for reading line information, and `LineInfo` is the
		// struct it reads and writes.
		if unsafe {
			ioctl(
				chip.as_fd(),
				Updater::<GPIO_GET_LINEINFO, LineInfo>::new(&mut info),
			)
		}
		.is_err()
		{
			break;
		}
		if as_str(&info.name) == name {
			return Some(offset);
		}
	}
	None
}

/// The level of one line, requested as an input for the moment it takes to read it.
fn read_line(chip: &File, offset: u32) -> io::Result<bool> {
	let mut request = HandleRequest {
		offsets: [0; HANDLE_LINES],
		flags: GPIOHANDLE_REQUEST_INPUT,
		defaults: [0; HANDLE_LINES],
		consumer: [0; 32],
		lines: 1,
		fd: -1,
	};
	request.offsets[0] = offset;
	for (slot, byte) in request.consumer.iter_mut().zip(b"bliti") {
		*slot = *byte;
	}

	// SAFETY: the opcode is the kernel's for requesting a line handle, and `HandleRequest` is the
	// struct it reads and writes. It returns a file descriptor in the struct's `fd`.
	unsafe {
		ioctl(
			chip.as_fd(),
			Updater::<GPIO_GET_LINEHANDLE, HandleRequest>::new(&mut request),
		)
	}
	.map_err(io::Error::from)?;

	// Owned so the handle is closed however this returns: leaking it would hold the line requested
	// and every later read would be refused.
	let handle = unsafe { <File as std::os::fd::FromRawFd>::from_raw_fd(request.fd) };
	let mut data = HandleData {
		values: [0; HANDLE_LINES],
	};
	// SAFETY: the opcode is the kernel's for reading the values of a requested handle, and
	// `HandleData` is the struct it writes.
	unsafe {
		ioctl(
			handle.as_fd(),
			Updater::<GPIOHANDLE_GET_LINE_VALUES, HandleData>::new(&mut data),
		)
	}
	.map_err(io::Error::from)?;

	Ok(data.values[0] != 0)
}

/// A NUL-padded fixed-width name, as the kernel writes them.
fn as_str(raw: &[u8; 32]) -> &str {
	let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
	std::str::from_utf8(&raw[..end]).unwrap_or("")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_name_stops_at_its_nul() {
		let mut raw = [0u8; 32];
		raw[..5].copy_from_slice(b"GPIO6");
		assert_eq!(as_str(&raw), "GPIO6");
		assert_eq!(as_str(&[0; 32]), "");
	}

	/// A machine with no such line says so rather than answering for a different one.
	#[test]
	fn an_absent_line_is_not_found() {
		let found = read_by_name("NO_SUCH_LINE_HERE");
		assert!(found.is_err());
	}
}
