//! The bootloader on the board's flash, read and written whole (PBL, and PRV "The boot order").

use std::{fs, path::Path, process::Command};

use jiff::{Timestamp, civil::Date, tz::TimeZone};

use self::config::Config;
use super::exec::run;

pub mod config;
#[cfg(test)]
pub mod testing;

/// The SPI device the Pi 5's bootloader flash is on.
const SPI_DEVICE: &str = "/dev/spidev10.0";

/// The speeds, in kHz, a flash write is tried at: the first, then each retry at the second.
const SPEEDS: [u32; 2] = [16000, 8000];

/// How many times a write is tried.
const ATTEMPTS: usize = 5;

/// Where the medium's system links the payload partition, which is on the disk it booted from.
pub const PAYLOAD: &str = "/run/bliti-provision/payload";

/// The bootloader's flash and the tools that read and make its images, each by blocking calls that
/// say why they failed.
pub trait Firmware: Send + Sync {
	/// The image the flash holds.
	fn read(&self) -> Result<Vec<u8>, String>;

	/// Write `image` to the flash, whole, and check that the flash holds it.
	fn write(&self, image: &[u8]) -> Result<(), String>;

	/// The configuration `image` holds.
	fn config(&self, image: &[u8]) -> Result<String, String>;

	/// `image` with `config` in place of the configuration it holds.
	fn with_config(&self, image: &[u8], config: &str) -> Result<Vec<u8>, String>;
}

/// What the board's flash holds.
pub struct Held {
	pub image: Vec<u8>,
	pub date: Date,
	pub config: Config,
}

/// Read what the board's flash holds.
pub fn held(firmware: &dyn Firmware) -> Result<Held, String> {
	let image = firmware.read()?;
	let date = date(&image)?;
	let config = Config::parse(&firmware.config(&image)?);
	Ok(Held {
		image,
		date,
		config,
	})
}

/// The date of the bootloader in `image`: the UTC date it was built on, which Raspberry Pi names
/// each by. Where the image holds more than one bootloader, the earliest.
pub fn date(image: &[u8]) -> Result<Date, String> {
	const KEY: &[u8] = b"BUILD_TIMESTAMP=";
	let built = image
		.windows(KEY.len())
		.enumerate()
		.filter(|(_, window)| *window == KEY)
		.filter_map(|(at, _)| {
			let digits: String = image[at + KEY.len()..]
				.iter()
				.take_while(|byte| byte.is_ascii_digit())
				.map(|byte| char::from(*byte))
				.collect();
			digits.parse::<i64>().ok()
		})
		.min()
		.ok_or("the image holds no bootloader build date")?;
	let built = Timestamp::from_second(built).map_err(|err| err.to_string())?;
	Ok(built.to_zoned(TimeZone::UTC).date())
}

/// What putting USB first did.
#[derive(Debug, PartialEq, Eq)]
pub enum UsbFirst {
	/// The device did not boot from a USB stick.
	NotBootedFromUsb,
	/// USB was first already.
	Already,
	/// The bootloader was written.
	Written,
}

/// Put USB first in the boot order where the device booted from a USB stick, writing the
/// bootloader only where it is not first already (PRV, "The boot order").
// spec: PRV#the-boot-order
pub fn put_usb_first(
	firmware: &dyn Firmware,
	payload: &Path,
	sysfs: &Path,
) -> Result<UsbFirst, String> {
	if !booted_from_usb(payload, sysfs)? {
		return Ok(UsbFirst::NotBootedFromUsb);
	}
	let image = firmware.read()?;
	let config = Config::parse(&firmware.config(&image)?);
	let Some(first) = config.put_usb_first()? else {
		return Ok(UsbFirst::Already);
	};
	let written = firmware.with_config(&image, &first.to_string())?;
	firmware.write(&written)?;

	let after = firmware.read()?;
	if !Config::parse(&firmware.config(&after)?).usb_is_first()? {
		return Err("the bootloader does not hold USB first after it was written".to_owned());
	}
	Ok(UsbFirst::Written)
}

/// Whether the disk the payload partition is on is attached by USB. The medium's system links the
/// payload from the disk it booted from.
fn booted_from_usb(payload: &Path, sysfs: &Path) -> Result<bool, String> {
	let node = fs::canonicalize(payload)
		.map_err(|err| format!("cannot find the partition {}: {err}", payload.display()))?;
	let name = node
		.file_name()
		.ok_or_else(|| format!("{} names no partition", node.display()))?;
	let device = fs::canonicalize(sysfs.join("class/block").join(name))
		.map_err(|err| format!("cannot find {} in sysfs: {err}", node.display()))?;
	Ok(device.components().any(|part| {
		part.as_os_str()
			.to_str()
			.and_then(|part| part.strip_prefix("usb"))
			.is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()))
	}))
}

/// The Pi 5's bootloader flash, reached through `flashrom`, and its images made with
/// `rpi-eeprom-config`, as Raspberry Pi's own updater does.
pub struct Spi;

impl Spi {
	/// Run `flashrom` against the bootloader's flash at `speed`, with `flag` and then `image`.
	fn flashrom(speed: u32, flag: &str, image: &Path) -> Result<(), String> {
		let mut command = Command::new("flashrom");
		command
			.arg("-p")
			.arg(format!("linux_spi:dev={SPI_DEVICE},spispeed={speed}"))
			.arg(flag)
			.arg(image);
		run(&mut command).map(drop)
	}
}

impl Firmware for Spi {
	fn read(&self) -> Result<Vec<u8>, String> {
		let dir = tempfile::tempdir().map_err(|err| err.to_string())?;
		let path = dir.path().join("flash");
		let mut result = Err(String::new());
		for speed in SPEEDS {
			result = Self::flashrom(speed, "-r", &path);
			if result.is_ok() {
				break;
			}
		}
		result?;
		fs::read(&path).map_err(|err| format!("cannot read what flashrom read: {err}"))
	}

	fn write(&self, image: &[u8]) -> Result<(), String> {
		let dir = tempfile::tempdir().map_err(|err| err.to_string())?;
		let path = dir.path().join("image");
		fs::write(&path, image).map_err(|err| err.to_string())?;
		let mut result = Err(String::new());
		for attempt in 0..ATTEMPTS {
			let speed = SPEEDS[attempt.min(SPEEDS.len() - 1)];
			result = Self::flashrom(speed, "-w", &path)
				.and_then(|()| Self::flashrom(speed, "-v", &path));
			if result.is_ok() {
				break;
			}
		}
		result
	}

	fn config(&self, image: &[u8]) -> Result<String, String> {
		let dir = tempfile::tempdir().map_err(|err| err.to_string())?;
		let path = dir.path().join("image");
		fs::write(&path, image).map_err(|err| err.to_string())?;
		let output = run(Command::new("rpi-eeprom-config").arg(&path))?;
		String::from_utf8(output.stdout).map_err(|err| err.to_string())
	}

	fn with_config(&self, image: &[u8], config: &str) -> Result<Vec<u8>, String> {
		let dir = tempfile::tempdir().map_err(|err| err.to_string())?;
		let (from, conf, to) = (
			dir.path().join("from"),
			dir.path().join("config"),
			dir.path().join("to"),
		);
		fs::write(&from, image).map_err(|err| err.to_string())?;
		fs::write(&conf, config).map_err(|err| err.to_string())?;
		run(Command::new("rpi-eeprom-config")
			.arg("--config")
			.arg(&conf)
			.arg("--timestamp")
			.arg(Timestamp::now().as_second().to_string())
			.arg("--out")
			.arg(&to)
			.arg(&from))?;
		fs::read(&to).map_err(|err| format!("cannot read the image rpi-eeprom-config made: {err}"))
	}
}

#[cfg(test)]
mod tests;
