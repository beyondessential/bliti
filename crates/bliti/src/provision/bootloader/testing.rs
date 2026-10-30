//! A flash for tests: images are a build date, then a configuration, in plain text.

use std::sync::{
	Mutex,
	atomic::{AtomicBool, AtomicU32, Ordering},
};

use jiff::{civil::Date, tz::TimeZone};

use super::Firmware;

const SEPARATOR: &str = "\n---\n";

/// An image of a bootloader built on `date`, holding `config`.
pub fn image(date: Date, config: &str) -> Vec<u8> {
	let built = date
		.at(12, 0, 0, 0)
		.to_zoned(TimeZone::UTC)
		.expect("a time")
		.timestamp()
		.as_second();
	format!("BUILD_TIMESTAMP={built}{SEPARATOR}{config}").into_bytes()
}

/// A flash holding an image.
pub struct Fake {
	flash: Mutex<Vec<u8>>,
	writes: AtomicU32,
	/// Whether a write reports success without changing the flash.
	pub forgetful: AtomicBool,
	/// Whether the flash cannot be read.
	pub unreadable: AtomicBool,
}

impl Fake {
	pub fn holding(image: Vec<u8>) -> Self {
		Self {
			flash: Mutex::new(image),
			writes: AtomicU32::new(0),
			forgetful: AtomicBool::new(false),
			unreadable: AtomicBool::new(false),
		}
	}

	pub fn contents(&self) -> Vec<u8> {
		self.flash.lock().unwrap().clone()
	}

	pub fn writes(&self) -> u32 {
		self.writes.load(Ordering::SeqCst)
	}
}

impl Firmware for Fake {
	fn read(&self) -> Result<Vec<u8>, String> {
		if self.unreadable.load(Ordering::SeqCst) {
			return Err("no flash".to_owned());
		}
		Ok(self.contents())
	}

	fn write(&self, image: &[u8]) -> Result<(), String> {
		self.writes.fetch_add(1, Ordering::SeqCst);
		if !self.forgetful.load(Ordering::SeqCst) {
			*self.flash.lock().unwrap() = image.to_vec();
		}
		Ok(())
	}

	fn config(&self, image: &[u8]) -> Result<String, String> {
		let text = String::from_utf8_lossy(image);
		let (_, config) = text.split_once(SEPARATOR).ok_or("not an image")?;
		Ok(config.to_owned())
	}

	fn with_config(&self, image: &[u8], config: &str) -> Result<Vec<u8>, String> {
		let text = String::from_utf8_lossy(image);
		let (head, _) = text.split_once(SEPARATOR).ok_or("not an image")?;
		Ok(format!("{head}{SEPARATOR}{config}").into_bytes())
	}
}
