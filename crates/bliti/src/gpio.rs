//! GPIO lines, found by the name the kernel gives them.
//!
//! By name rather than by number, because the pin header is `gpiochip0` on some kernels and
//! `gpiochip4` on others, and a hardcoded number quietly reads a different pin on the wrong one.
//!
//! Hand-rolled on the kernel's character-device ABI for the same reason as the I2C side: it is a few
//! ioctls over plain C structs, and the crates that wrap it either bring a C library the cross-build
//! would have to carry or wrap exactly this. A line read once goes through the first version of the
//! ABI; a line watched for edges through the second, which is the one that carries debouncing.

use std::{
	fs::File,
	io::{self, Read as _},
	os::fd::{AsFd as _, FromRawFd as _},
	time::Duration,
};

use rustix::ioctl::{Opcode, Updater, ioctl, opcode};

const GPIO_GET_LINEINFO: Opcode = opcode::read_write::<LineInfo>(0xb4, 0x02);
const GPIO_GET_LINEHANDLE: Opcode = opcode::read_write::<HandleRequest>(0xb4, 0x03);
const GPIOHANDLE_GET_LINE_VALUES: Opcode = opcode::read_write::<HandleData>(0xb4, 0x08);
const GPIOHANDLE_REQUEST_INPUT: u32 = 1;

const GPIO_V2_GET_LINE: Opcode = opcode::read_write::<LineRequest>(0xb4, 0x07);
const GPIO_V2_LINE_GET_VALUES: Opcode = opcode::read_write::<LineValues>(0xb4, 0x0e);
const GPIO_V2_LINE_FLAG_INPUT: u64 = 1 << 2;
const GPIO_V2_LINE_FLAG_EDGE_RISING: u64 = 1 << 4;
const GPIO_V2_LINE_FLAG_EDGE_FALLING: u64 = 1 << 5;
const GPIO_V2_LINE_ATTR_ID_DEBOUNCE: u32 = 3;
const GPIO_V2_LINE_EVENT_RISING_EDGE: u32 = 1;

/// How many lines a request can carry. Fixed by the ABI.
const LINES: usize = 64;

/// How many attributes a line configuration can carry. Fixed by the ABI.
const ATTRIBUTES: usize = 10;

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
	offsets: [u32; LINES],
	flags: u32,
	defaults: [u8; LINES],
	consumer: [u8; 32],
	lines: u32,
	fd: i32,
}

#[repr(C)]
struct HandleData {
	values: [u8; LINES],
}

#[repr(C)]
#[derive(Clone, Copy)]
union AttributeValue {
	flags: u64,
	debounce_period_us: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct LineAttribute {
	id: u32,
	padding: u32,
	value: AttributeValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ConfigAttribute {
	attribute: LineAttribute,
	mask: u64,
}

#[repr(C)]
struct LineConfig {
	flags: u64,
	attributes: u32,
	padding: [u32; 5],
	attrs: [ConfigAttribute; ATTRIBUTES],
}

#[repr(C)]
struct LineRequest {
	offsets: [u32; LINES],
	consumer: [u8; 32],
	config: LineConfig,
	lines: u32,
	event_buffer_size: u32,
	padding: [u32; 5],
	fd: i32,
}

#[repr(C)]
struct LineValues {
	bits: u64,
	mask: u64,
}

/// The size of one edge event as the kernel writes it. Only its `id`, at byte 8, is read.
const EVENT_SIZE: usize = 48;

/// Whether the named line is high, searching every chip for one that carries it.
pub fn read_by_name(name: &str) -> io::Result<bool> {
	let (chip, offset) = locate(name)?;
	read_line(&chip, offset)
}

/// A line held as an input and watched for edges, the kernel debouncing it.
#[derive(Debug)]
pub struct Edges {
	/// The line request. Closing it releases the line.
	request: File,
}

impl Edges {
	/// Request the named line as an input reporting both edges, each only once the line has held its
	/// new level for `debounce`. The kernel debounces in software where the chip cannot.
	pub fn request(name: &str, debounce: Duration) -> io::Result<Self> {
		let (chip, offset) = locate(name)?;

		let mut debounced = AttributeValue { flags: 0 };
		debounced.debounce_period_us = u32::try_from(debounce.as_micros()).unwrap_or(u32::MAX);
		let unused = ConfigAttribute {
			attribute: LineAttribute {
				id: 0,
				padding: 0,
				value: AttributeValue { flags: 0 },
			},
			mask: 0,
		};
		let mut attrs = [unused; ATTRIBUTES];
		attrs[0] = ConfigAttribute {
			attribute: LineAttribute {
				id: GPIO_V2_LINE_ATTR_ID_DEBOUNCE,
				padding: 0,
				value: debounced,
			},
			// The first, and only, line of the request.
			mask: 1,
		};
		let mut request = LineRequest {
			offsets: [0; LINES],
			consumer: consumer(),
			config: LineConfig {
				flags: GPIO_V2_LINE_FLAG_INPUT
					| GPIO_V2_LINE_FLAG_EDGE_RISING
					| GPIO_V2_LINE_FLAG_EDGE_FALLING,
				attributes: 1,
				padding: [0; 5],
				attrs,
			},
			lines: 1,
			// The kernel's default.
			event_buffer_size: 0,
			padding: [0; 5],
			fd: -1,
		};
		request.offsets[0] = offset;

		// SAFETY: the opcode is the kernel's for requesting lines, and `LineRequest` is the struct it
		// reads and writes. It returns a file descriptor in the struct's `fd`.
		unsafe {
			ioctl(
				chip.as_fd(),
				Updater::<GPIO_V2_GET_LINE, LineRequest>::new(&mut request),
			)
		}
		.map_err(io::Error::from)?;
		// SAFETY: the kernel has just handed this descriptor over, and nothing else owns it.
		let request = unsafe { File::from_raw_fd(request.fd) };
		Ok(Self { request })
	}

	/// Whether the line is high now.
	pub fn level(&self) -> io::Result<bool> {
		let mut values = LineValues { bits: 0, mask: 1 };
		// SAFETY: the opcode is the kernel's for reading the values of requested lines, and
		// `LineValues` is the struct it reads and writes.
		unsafe {
			ioctl(
				self.request.as_fd(),
				Updater::<GPIO_V2_LINE_GET_VALUES, LineValues>::new(&mut values),
			)
		}
		.map_err(io::Error::from)?;
		Ok(values.bits & 1 != 0)
	}

	/// Wait for the next edge, returning whether the line is high after it.
	pub fn next(&mut self) -> io::Result<bool> {
		let mut event = [0u8; EVENT_SIZE];
		self.request.read_exact(&mut event)?;
		Ok(rose(&event))
	}
}

/// Whether an edge event records the line rising.
fn rose(event: &[u8; EVENT_SIZE]) -> bool {
	let id = u32::from_ne_bytes([event[8], event[9], event[10], event[11]]);
	id == GPIO_V2_LINE_EVENT_RISING_EDGE
}

/// The chip carrying the named line, and its offset there.
fn locate(name: &str) -> io::Result<(File, u32)> {
	let mut chips: Vec<_> = std::fs::read_dir("/dev")?
		.flatten()
		.map(|entry| entry.path())
		.filter(|path| {
			path.file_name()
				.and_then(|name| name.to_str())
				.is_some_and(|name| name.starts_with("gpiochip"))
		})
		.collect();
	// A stable order, so the same line is found every time.
	chips.sort();

	for chip in chips {
		let Ok(file) = File::open(&chip) else {
			continue;
		};
		if let Some(offset) = find_line(&file, name) {
			return Ok((file, offset));
		}
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
		offsets: [0; LINES],
		flags: GPIOHANDLE_REQUEST_INPUT,
		defaults: [0; LINES],
		consumer: consumer(),
		lines: 1,
		fd: -1,
	};
	request.offsets[0] = offset;

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
	// SAFETY: the kernel has just handed this descriptor over, and nothing else owns it.
	let handle = unsafe { File::from_raw_fd(request.fd) };
	let mut data = HandleData { values: [0; LINES] };
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

/// The name a requested line is held under, as the kernel shows it to anyone else looking.
fn consumer() -> [u8; 32] {
	let mut consumer = [0; 32];
	consumer[..5].copy_from_slice(b"bliti");
	consumer
}

/// A NUL-padded fixed-width name, as the kernel writes them.
fn as_str(raw: &[u8; 32]) -> &str {
	let end = raw.iter().position(|byte| *byte == 0).unwrap_or(raw.len());
	std::str::from_utf8(&raw[..end]).unwrap_or("")
}

#[cfg(test)]
mod tests {
	use std::mem::size_of;

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
		assert!(read_by_name("NO_SUCH_LINE_HERE").is_err());
		assert!(Edges::request("NO_SUCH_LINE_HERE", Duration::from_millis(20)).is_err());
	}

	/// The sizes `linux/gpio.h` gives these structs, which the opcodes are built from.
	#[test]
	fn the_structs_match_the_kernel_abi() {
		assert_eq!(size_of::<LineInfo>(), 72);
		assert_eq!(size_of::<HandleRequest>(), 364);
		assert_eq!(size_of::<HandleData>(), 64);
		assert_eq!(size_of::<LineAttribute>(), 16);
		assert_eq!(size_of::<ConfigAttribute>(), 24);
		assert_eq!(size_of::<LineConfig>(), 272);
		assert_eq!(size_of::<LineRequest>(), 592);
		assert_eq!(size_of::<LineValues>(), 16);
	}

	/// The opcodes as `linux/gpio.h` spells them out.
	#[test]
	fn the_opcodes_match_the_kernel_abi() {
		assert_eq!(GPIO_GET_LINEINFO, 0xc048_b402);
		assert_eq!(GPIO_GET_LINEHANDLE, 0xc16c_b403);
		assert_eq!(GPIOHANDLE_GET_LINE_VALUES, 0xc040_b408);
		assert_eq!(GPIO_V2_GET_LINE, 0xc250_b407);
		assert_eq!(GPIO_V2_LINE_GET_VALUES, 0xc010_b40e);
	}

	#[test]
	fn an_event_says_which_way_the_line_went() {
		let mut event = [0u8; EVENT_SIZE];
		event[8..12].copy_from_slice(&1u32.to_ne_bytes());
		assert!(rose(&event));
		event[8..12].copy_from_slice(&2u32.to_ne_bytes());
		assert!(!rose(&event));
	}
}
