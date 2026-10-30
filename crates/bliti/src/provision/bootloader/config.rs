//! A bootloader configuration: the text the bootloader reads from its flash, of settings each of
//! the form `NAME=value` (PBL, "The configuration").

use std::{collections::BTreeMap, fmt};

/// The boot order the Pi 5's bootloader uses where none is set.
const DEFAULT_ORDER: u32 = 0xf461;

/// The digit of a boot order that names a USB mass storage device.
const USB: u32 = 0x4;

/// The most digits of a boot order that leave room for USB to be put in front.
const MAX_DIGITS: usize = 7;

/// A configuration, as lines of text with the empty ones and the comments left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
	lines: Vec<String>,
}

/// What a configuration sets for every board.
struct Settings<'a> {
	/// Each setting under no section or `[all]`, the last of a name winning.
	all: BTreeMap<&'a str, &'a str>,
	/// Whether the configuration holds anything else: a section that applies to some boards, or a line
	/// that is not a setting.
	foreign: bool,
}

impl Config {
	pub fn parse(text: &str) -> Self {
		Self {
			lines: text
				.lines()
				.map(str::trim)
				.filter(|line| !line.is_empty() && !line.starts_with('#'))
				.map(str::to_owned)
				.collect(),
		}
	}

	/// A configuration of exactly `settings`.
	pub fn of(settings: &[(&str, &str)]) -> Self {
		let mut lines = vec!["[all]".to_owned()];
		lines.extend(
			settings
				.iter()
				.map(|(name, value)| format!("{name}={value}")),
		);
		Self { lines }
	}

	fn settings(&self) -> Settings<'_> {
		let mut settings = Settings {
			all: BTreeMap::new(),
			foreign: false,
		};
		let mut in_all = true;
		for line in &self.lines {
			if let Some(section) = line
				.strip_prefix('[')
				.and_then(|rest| rest.strip_suffix(']'))
			{
				in_all = section.trim().eq_ignore_ascii_case("all");
				settings.foreign |= !in_all;
			} else if !in_all {
				settings.foreign = true;
			} else if let Some((name, value)) = line.split_once('=') {
				settings.all.insert(name.trim(), value.trim());
			} else {
				settings.foreign = true;
			}
		}
		settings
	}

	/// How this configuration differs from holding exactly `wanted`, one phrase for each difference.
	pub fn differences(&self, wanted: &[(&str, &str)]) -> Vec<String> {
		let settings = self.settings();
		let mut differences = Vec::new();
		for (name, value) in wanted {
			match settings.all.get(name) {
				Some(held) if held.eq_ignore_ascii_case(value) => {}
				Some(held) => differences.push(format!("{name} is {held}")),
				None => differences.push(format!("{name} is not set")),
			}
		}
		for name in settings.all.keys() {
			if !wanted.iter().any(|(wanted, _)| wanted == name) {
				differences.push(format!("{name} is set"));
			}
		}
		if settings.foreign {
			differences.push("it holds more than settings for every board".to_owned());
		}
		differences
	}

	/// This configuration with USB first in its boot order and the order of the other media as it
	/// was, or nothing where USB is already first.
	// spec: PRV#the-boot-order
	pub fn put_usb_first(&self) -> Result<Option<Self>, String> {
		let order = match self.settings().all.get("BOOT_ORDER") {
			Some(value) => parse_order(value)?,
			None => DEFAULT_ORDER,
		};
		let first = usb_first(order)?;
		if first == order {
			return Ok(None);
		}
		Ok(Some(self.with("BOOT_ORDER", &format!("{first:#x}"))))
	}

	/// Whether USB is first in the boot order.
	pub fn usb_is_first(&self) -> Result<bool, String> {
		self.put_usb_first().map(|put| put.is_none())
	}

	/// This configuration with the setting `name` set to `value` for every board.
	fn with(&self, name: &str, value: &str) -> Self {
		let mut lines = Vec::with_capacity(self.lines.len() + 1);
		let (mut in_all, mut set) = (true, false);
		let mut end_of_all = None;
		for line in &self.lines {
			if let Some(section) = line
				.strip_prefix('[')
				.and_then(|rest| rest.strip_suffix(']'))
			{
				in_all = section.trim().eq_ignore_ascii_case("all");
				if !in_all && end_of_all.is_none() {
					end_of_all = Some(lines.len());
				}
				lines.push(line.clone());
			} else if in_all
				&& line
					.split_once('=')
					.is_some_and(|(held, _)| held.trim() == name)
			{
				lines.push(format!("{name}={value}"));
				set = true;
			} else {
				lines.push(line.clone());
			}
		}
		if !set {
			lines.insert(end_of_all.unwrap_or(lines.len()), format!("{name}={value}"));
		}
		Self { lines }
	}
}

impl fmt::Display for Config {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		self.lines.iter().try_for_each(|line| writeln!(f, "{line}"))
	}
}

fn parse_order(value: &str) -> Result<u32, String> {
	let parsed = match value
		.strip_prefix("0x")
		.or_else(|| value.strip_prefix("0X"))
	{
		Some(hex) => u32::from_str_radix(hex, 16),
		None => value.parse(),
	};
	parsed.map_err(|_| format!("BOOT_ORDER is {value}, which is not a boot order"))
}

/// `order` with USB tried first. The order's digits are the media, the lowest digit tried first.
fn usb_first(order: u32) -> Result<u32, String> {
	let mut digits = Vec::new();
	let mut rest = order;
	while rest > 0 {
		digits.push(rest & 0xf);
		rest >>= 4;
	}
	if digits.first() == Some(&USB) {
		return Ok(order);
	}
	digits.retain(|digit| *digit != USB);
	if digits.len() > MAX_DIGITS {
		return Err(format!(
			"BOOT_ORDER {order:#x} has no room for another medium"
		));
	}
	digits.insert(0, USB);
	Ok(digits.iter().rev().fold(0, |acc, digit| acc << 4 | digit))
}

#[cfg(test)]
mod tests {
	use super::*;

	const WANTED: [(&str, &str); 2] = [("BOOT_ORDER", "0xf61"), ("BOOT_UART", "1")];

	fn put(config: &str) -> Option<String> {
		Config::parse(config)
			.put_usb_first()
			.unwrap()
			.map(|config| config.to_string())
	}

	#[test]
	fn usb_goes_ahead_of_the_others_and_the_rest_keep_their_order() {
		assert_eq!(usb_first(0xf461).unwrap(), 0xf614);
		assert_eq!(usb_first(0xf61).unwrap(), 0xf614);
		assert_eq!(usb_first(0xf146).unwrap(), 0xf164);
	}

	#[test]
	fn usb_already_first_is_left() {
		assert_eq!(usb_first(0xf614).unwrap(), 0xf614);
		assert_eq!(put("[all]\nBOOT_ORDER=0xf614\n"), None);
	}

	#[test]
	fn a_boot_order_with_no_room_is_refused() {
		assert!(usb_first(0x1111_1111).is_err());
	}

	#[test]
	fn the_boot_order_is_rewritten_in_place_and_the_rest_kept() {
		assert_eq!(
			put("[all]\nBOOT_UART=1\nBOOT_ORDER=0xf461\nNET_INSTALL_AT_POWER_ON=1\n").unwrap(),
			"[all]\nBOOT_UART=1\nBOOT_ORDER=0xf614\nNET_INSTALL_AT_POWER_ON=1\n",
		);
	}

	#[test]
	fn a_configuration_without_a_boot_order_starts_from_the_default() {
		assert_eq!(
			put("[all]\nBOOT_UART=1\n").unwrap(),
			"[all]\nBOOT_UART=1\nBOOT_ORDER=0xf614\n"
		);
		assert_eq!(put("").unwrap(), "BOOT_ORDER=0xf614\n");
	}

	#[test]
	fn a_boot_order_added_goes_ahead_of_sections_for_some_boards() {
		assert_eq!(
			put("BOOT_UART=1\n[gpio4=0]\nBOOT_UART=0\n").unwrap(),
			"BOOT_UART=1\nBOOT_ORDER=0xf614\n[gpio4=0]\nBOOT_UART=0\n",
		);
	}

	#[test]
	fn a_boot_order_in_a_section_for_some_boards_is_not_the_one_for_every_board() {
		assert_eq!(
			put("[gpio4=0]\nBOOT_ORDER=0xf614\n").unwrap(),
			"BOOT_ORDER=0xf614\n[gpio4=0]\nBOOT_ORDER=0xf614\n",
		);
	}

	#[test]
	fn decimal_boot_orders_are_read() {
		assert_eq!(put("BOOT_ORDER=62561\n").unwrap(), "BOOT_ORDER=0xf614\n");
		assert!(Config::parse("BOOT_ORDER=sd\n").put_usb_first().is_err());
	}

	#[test]
	fn comments_and_spacing_do_not_count_as_differences() {
		let config = Config::parse("# the Iti's\n\n[all]\n BOOT_ORDER = 0xF61 \nBOOT_UART=1\n");
		assert!(config.differences(&WANTED).is_empty());
	}

	#[test]
	fn each_difference_is_named() {
		let config = Config::parse("BOOT_ORDER=0xf461\nNET_INSTALL_AT_POWER_ON=1\n");
		assert_eq!(
			config.differences(&WANTED),
			[
				"BOOT_ORDER is 0xf461",
				"BOOT_UART is not set",
				"NET_INSTALL_AT_POWER_ON is set"
			],
		);
	}

	#[test]
	fn a_section_for_some_boards_is_a_difference() {
		let config = Config::parse("BOOT_ORDER=0xf61\nBOOT_UART=1\n[gpio4=0]\nBOOT_UART=0\n");
		assert_eq!(
			config.differences(&WANTED),
			["it holds more than settings for every board"]
		);
	}

	#[test]
	fn a_configuration_of_settings_holds_exactly_them() {
		let config = Config::of(&WANTED);
		assert!(config.differences(&WANTED).is_empty());
		assert_eq!(config.to_string(), "[all]\nBOOT_ORDER=0xf61\nBOOT_UART=1\n");
	}
}
