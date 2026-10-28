//! The Lite-On LTR-308ALS: the light falling on the sensor, through the case's vents.
//!
//! No crate drives it, and it needs little: check the part, set a gain and resolution, turn it on,
//! and read a count. Lux is the datasheet's formula with no window factor, since the sensor sits
//! behind vents and not a window (NFO).

use std::fmt;

use bliti_core::channel::readings::{Entry, kind};
use embedded_hal::{delay::DelayNs, i2c::I2c};

use super::Problem;
use crate::facts::i2c::{self, Bus, Sleep};

const CHIP: &str = "LTR-308ALS";
const ADDRESS: u8 = 0x53;

const MAIN_CTRL: u8 = 0x00;
const ALS_MEAS_RATE: u8 = 0x04;
const ALS_GAIN: u8 = 0x05;
const PART_ID: u8 = 0x06;
const MAIN_STATUS: u8 = 0x07;
const ALS_DATA: u8 = 0x0d;

const PART: u8 = 0xb1;
const ALS_ENABLE: u8 = 0x02;
/// In `MAIN_STATUS`: the chip has been reset since the flag was last read, which loses its settings.
const POWER_ON: u8 = 0x20;
/// In `MAIN_STATUS`: a measurement has finished that has not been read.
const NEW_DATA: u8 = 0x08;

/// 18-bit counts over a 100 ms conversion, every 500 ms; and a gain of 3. The datasheet's default
/// range, which reads from about 0.2 lux to 52,000.
const MEAS_RATE: u8 = 0b010 << 4 | 0b011;
const GAIN: u8 = 0b001;
const GAIN_FACTOR: f64 = 3.0;
const INTEGRATION_FACTOR: f64 = 1.0;

/// How often, and how far apart, a new measurement is looked for. Spans more than one measurement.
const POLLS: u32 = 6;
const POLL_MS: u32 = 100;

pub struct Ltr308 {
	bus: &'static str,
	/// Set up on the first tick, and dropped after an error so the next tick starts afresh.
	driver: Option<Driver<Bus, Sleep>>,
}

impl fmt::Debug for Ltr308 {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Ltr308")
			.field("bus", &self.bus)
			.field("open", &self.driver.is_some())
			.finish()
	}
}

impl Ltr308 {
	pub fn new(bus: &'static str) -> Self {
		Self { bus, driver: None }
	}

	/// `light`, from the latest measurement.
	// spec: NFO
	pub fn readings(&mut self, at: u64) -> Vec<Entry> {
		match self.lux() {
			Ok(lux) => vec![Entry::quantity(at, "light", "lux", lux)],
			Err(problem) => {
				self.driver = None;
				vec![problem.entry(at, "light", kind::QUANTITY)]
			}
		}
	}

	fn lux(&mut self) -> Result<f64, Problem> {
		let driver = match &mut self.driver {
			Some(driver) => driver,
			None => {
				let bus = Bus::open(self.bus).map_err(|err| Problem::bus(CHIP, self.bus, &err))?;
				self.driver.insert(Driver::new(bus, Sleep))
			}
		};
		driver.lux().map_err(|err| err.problem(self.bus))
	}
}

#[derive(Debug)]
enum Error<E> {
	Bus(E),
	WrongPart(u8),
	Unfinished,
}

impl Error<i2c::Error> {
	fn problem(self, bus: &str) -> Problem {
		match self {
			Self::Bus(err) => Problem::bus(CHIP, bus, &err),
			Self::WrongPart(id) => Problem::wrong_part(CHIP, id),
			Self::Unfinished => Problem::unfinished(CHIP),
		}
	}
}

struct Driver<I, D> {
	i2c: I,
	delay: D,
	configured: bool,
}

impl<I: I2c, D: DelayNs> Driver<I, D> {
	fn new(i2c: I, delay: D) -> Self {
		Self {
			i2c,
			delay,
			configured: false,
		}
	}

	/// The light now, setting the chip up first where it isn't.
	fn lux(&mut self) -> Result<f64, Error<I::Error>> {
		if !self.configured {
			self.configure()?;
		}
		for _ in 0..POLLS {
			let status = self.read(MAIN_STATUS)?;
			if status & POWER_ON != 0 {
				self.configure()?;
			} else if status & NEW_DATA != 0 {
				let mut data = [0u8; 3];
				self.i2c
					.write_read(ADDRESS, &[ALS_DATA], &mut data)
					.map_err(Error::Bus)?;
				return Ok(lux(counts(data)));
			}
			self.delay.delay_ms(POLL_MS);
		}
		Err(Error::Unfinished)
	}

	fn configure(&mut self) -> Result<(), Error<I::Error>> {
		self.configured = false;
		let part = self.read(PART_ID)?;
		if part != PART {
			return Err(Error::WrongPart(part));
		}
		self.write(ALS_MEAS_RATE, MEAS_RATE)?;
		self.write(ALS_GAIN, GAIN)?;
		self.write(MAIN_CTRL, ALS_ENABLE)?;
		// Reading the status clears the power-on flag, so a later one means the chip was reset.
		self.read(MAIN_STATUS)?;
		self.configured = true;
		Ok(())
	}

	fn read(&mut self, register: u8) -> Result<u8, Error<I::Error>> {
		let mut byte = [0u8];
		self.i2c
			.write_read(ADDRESS, &[register], &mut byte)
			.map_err(Error::Bus)?;
		Ok(byte[0])
	}

	fn write(&mut self, register: u8, value: u8) -> Result<(), Error<I::Error>> {
		self.i2c
			.write(ADDRESS, &[register, value])
			.map_err(Error::Bus)
	}
}

/// The 20-bit count, least significant byte first.
fn counts(data: [u8; 3]) -> u32 {
	u32::from(data[0]) | u32::from(data[1]) << 8 | u32::from(data[2] & 0x0f) << 16
}

fn lux(counts: u32) -> f64 {
	0.6 * f64::from(counts) / (GAIN_FACTOR * INTEGRATION_FACTOR)
}

#[cfg(test)]
mod tests {
	use std::convert::Infallible;

	use embedded_hal::i2c::{ErrorType, Operation};

	use super::*;

	/// A stand-in chip: a register file, with the status flags clearing when read as the real one's
	/// do.
	struct Chip {
		registers: [u8; 0x27],
		/// What `MAIN_STATUS` reads as on each successive read, before falling back to the register.
		statuses: Vec<u8>,
	}

	impl Chip {
		fn new(part: u8) -> Self {
			let mut registers = [0u8; 0x27];
			registers[usize::from(PART_ID)] = part;
			registers[usize::from(MAIN_STATUS)] = POWER_ON;
			Self {
				registers,
				statuses: Vec::new(),
			}
		}
	}

	impl ErrorType for Chip {
		type Error = Infallible;
	}

	impl I2c for Chip {
		fn transaction(
			&mut self,
			address: u8,
			operations: &mut [Operation<'_>],
		) -> Result<(), Infallible> {
			assert_eq!(address, ADDRESS);
			let mut pointer = 0usize;
			for operation in operations {
				match operation {
					Operation::Write([register, values @ ..]) => {
						pointer = usize::from(*register);
						for (i, value) in values.iter().enumerate() {
							self.registers[pointer + i] = *value;
						}
					}
					Operation::Write([]) => {}
					Operation::Read(buffer) => {
						for (i, byte) in buffer.iter_mut().enumerate() {
							let register = pointer + i;
							*byte = if register == usize::from(MAIN_STATUS) {
								let status = if self.statuses.is_empty() {
									self.registers[register]
								} else {
									self.statuses.remove(0)
								};
								self.registers[register] &= !(POWER_ON | NEW_DATA);
								status
							} else {
								self.registers[register]
							};
						}
					}
				}
			}
			Ok(())
		}
	}

	struct NoWait;

	impl DelayNs for NoWait {
		fn delay_ns(&mut self, _ns: u32) {}
	}

	fn data(chip: &mut Chip, counts: u32) {
		let at = usize::from(ALS_DATA);
		chip.registers[at..at + 3].copy_from_slice(&counts.to_le_bytes()[..3]);
	}

	#[test]
	fn lux_follows_the_datasheet_at_gain_3_and_100_ms() {
		assert_eq!(lux(5), 1.0);
		// The datasheet gives this setting's range as about 0.2 lux to 52,000.
		assert!((lux(1) - 0.2).abs() < 1e-9);
		assert!((lux((1 << 18) - 1) - 52_428.6).abs() < 0.1);
	}

	#[test]
	fn the_count_is_twenty_bits_least_significant_byte_first() {
		assert_eq!(counts([0x34, 0x12, 0xf5]), 0x5_1234);
	}

	// spec: NFO
	#[test]
	fn the_chip_is_set_up_and_read() {
		let mut chip = Chip::new(PART);
		data(&mut chip, 500);
		chip.statuses = vec![POWER_ON, NEW_DATA];
		let mut driver = Driver::new(chip, NoWait);
		assert_eq!(driver.lux().unwrap(), 100.0);
		let registers = driver.i2c.registers;
		assert_eq!(registers[usize::from(MAIN_CTRL)], ALS_ENABLE);
		assert_eq!(registers[usize::from(ALS_MEAS_RATE)], 0x23);
		assert_eq!(registers[usize::from(ALS_GAIN)], GAIN);
	}

	// spec: NFO
	#[test]
	fn another_part_at_the_address_is_not_read() {
		let mut driver = Driver::new(Chip::new(0xb2), NoWait);
		assert!(matches!(driver.lux(), Err(Error::WrongPart(0xb2))));
		assert_eq!(driver.i2c.registers[usize::from(MAIN_CTRL)], 0);

		let problem = Error::<i2c::Error>::WrongPart(0xb2).problem("/dev/i2c-3");
		let entry = problem.entry(1, "light", kind::QUANTITY);
		assert_eq!(entry.status(), Some("broken"));
		assert!(entry.reason().unwrap().contains(CHIP));
	}

	#[test]
	fn a_measurement_that_never_finishes_is_reported() {
		let mut driver = Driver::new(Chip::new(PART), NoWait);
		assert!(matches!(driver.lux(), Err(Error::Unfinished)));
	}

	/// A chip reset since it was set up has lost its settings, so it is set up again before reading.
	#[test]
	fn a_chip_reset_since_setup_is_set_up_again() {
		let mut chip = Chip::new(PART);
		data(&mut chip, 15);
		let mut driver = Driver::new(chip, NoWait);
		driver.configure().unwrap();
		driver.i2c.registers[usize::from(MAIN_CTRL)] = 0;
		driver.i2c.statuses = vec![POWER_ON, 0, NEW_DATA];
		assert_eq!(driver.lux().unwrap(), 3.0);
		assert_eq!(driver.i2c.registers[usize::from(MAIN_CTRL)], ALS_ENABLE);
	}

	// spec: NFO
	#[test]
	fn a_sensor_that_does_not_answer_reports_light_broken() {
		let entries = Ltr308::new("/nonexistent/bliti/i2c-3").readings(1);
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].name, "light");
		assert_eq!(entries[0].status(), Some("broken"));
		assert!(entries[0].reason().unwrap().contains(CHIP));
	}
}
