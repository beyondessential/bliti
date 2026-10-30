use std::{os::unix::fs::symlink, sync::atomic::Ordering};

use jiff::civil::date;

use super::{
	date as date_of,
	testing::{Fake, image},
	*,
};

const STOCK: &str = "[all]\nBOOT_UART=1\nBOOT_ORDER=0xf461\nNET_INSTALL_AT_POWER_ON=1\n";

#[test]
fn the_date_is_the_utc_date_the_bootloader_was_built_on() {
	assert_eq!(
		date_of(&image(date(2026, 9, 25), "")).unwrap(),
		date(2026, 9, 25)
	);
	assert_eq!(
		date_of(b"\0\0BUILD_TIMESTAMP=1790351949\0\0").unwrap(),
		date(2026, 9, 25)
	);
}

#[test]
fn an_image_with_two_bootloaders_is_as_old_as_the_older() {
	let mut both = image(date(2026, 9, 25), "");
	both.extend(image(date(2026, 5, 26), ""));
	assert_eq!(date_of(&both).unwrap(), date(2026, 5, 26));
}

#[test]
fn an_image_with_no_build_date_has_no_date() {
	assert!(date_of(b"nothing here").is_err());
	assert!(date_of(b"BUILD_TIMESTAMP=").is_err());
}

#[test]
fn what_the_flash_holds_is_its_date_and_configuration() {
	let fake = Fake::holding(image(date(2026, 9, 12), STOCK));
	let held = held(&fake).unwrap();
	assert_eq!(held.date, date(2026, 9, 12));
	assert!(held.config.usb_is_first().is_ok_and(|first| !first));
}

/// A disk at `class/block/<partition>` under a `sysfs`, reached through a device node that `/dev`
/// links to, along `bus`.
struct Layout {
	_root: tempfile::TempDir,
	payload: std::path::PathBuf,
	sysfs: std::path::PathBuf,
}

fn layout(bus: &str) -> Layout {
	let root = tempfile::tempdir().unwrap();
	let sysfs = root.path().join("sys");
	let device = sysfs.join(bus).join("block/sda/sda4");
	fs::create_dir_all(&device).unwrap();
	fs::create_dir_all(sysfs.join("class/block")).unwrap();
	symlink(&device, sysfs.join("class/block/sda4")).unwrap();
	let dev = root.path().join("dev");
	fs::create_dir_all(&dev).unwrap();
	fs::write(dev.join("sda4"), "").unwrap();
	let payload = root.path().join("payload");
	symlink(dev.join("sda4"), &payload).unwrap();
	Layout {
		payload,
		sysfs,
		_root: root,
	}
}

const USB_PATH: &str =
	"devices/platform/axi/1000120000.pcie/1f00300000.usb/xhci-hcd.0/usb1/1-1/1-1:1.0/host0";
const SD_PATH: &str = "devices/platform/axi/1000fff000.mmc/mmc_host/mmc0/mmc0:aaaa";

#[test]
fn a_disk_on_a_usb_bus_is_a_usb_stick() {
	let layout = layout(USB_PATH);
	assert!(booted_from_usb(&layout.payload, &layout.sysfs).unwrap());
}

#[test]
fn a_disk_on_the_sd_slot_is_not() {
	let layout = layout(SD_PATH);
	assert!(!booted_from_usb(&layout.payload, &layout.sysfs).unwrap());
}

#[test]
fn a_path_that_only_mentions_usb_is_not_a_usb_bus() {
	let layout = layout("devices/platform/usbhub/husb2x/block-usb/usb-ish");
	assert!(!booted_from_usb(&layout.payload, &layout.sysfs).unwrap());
}

#[test]
fn a_payload_that_cannot_be_found_is_an_error() {
	let layout = layout(USB_PATH);
	assert!(booted_from_usb(&layout.payload.join("missing"), &layout.sysfs).is_err());
}

#[test]
fn a_usb_stick_puts_usb_first_and_the_rest_is_kept() {
	let layout = layout(USB_PATH);
	let fake = Fake::holding(image(date(2026, 9, 12), STOCK));
	assert_eq!(
		put_usb_first(&fake, &layout.payload, &layout.sysfs).unwrap(),
		UsbFirst::Written
	);
	assert_eq!(fake.writes(), 1);
	let contents = fake.contents();
	let held = held(&fake).unwrap();
	assert_eq!(held.date, date(2026, 9, 12));
	assert_eq!(
		fake.config(&contents).unwrap(),
		"[all]\nBOOT_UART=1\nBOOT_ORDER=0xf614\nNET_INSTALL_AT_POWER_ON=1\n"
	);
}

#[test]
fn usb_first_already_is_not_written() {
	let layout = layout(USB_PATH);
	let fake = Fake::holding(image(date(2026, 9, 12), "BOOT_ORDER=0xf614\n"));
	assert_eq!(
		put_usb_first(&fake, &layout.payload, &layout.sysfs).unwrap(),
		UsbFirst::Already
	);
	assert_eq!(fake.writes(), 0);
}

#[test]
fn a_board_booted_from_an_sd_card_is_left() {
	let layout = layout(SD_PATH);
	let fake = Fake::holding(image(date(2026, 9, 12), STOCK));
	assert_eq!(
		put_usb_first(&fake, &layout.payload, &layout.sysfs).unwrap(),
		UsbFirst::NotBootedFromUsb
	);
	assert_eq!(fake.writes(), 0);
}

#[test]
fn a_write_that_did_not_take_is_an_error() {
	let layout = layout(USB_PATH);
	let fake = Fake::holding(image(date(2026, 9, 12), STOCK));
	fake.forgetful.store(true, Ordering::SeqCst);
	assert!(put_usb_first(&fake, &layout.payload, &layout.sysfs).is_err());
}

#[test]
fn a_flash_that_cannot_be_read_is_an_error() {
	let layout = layout(USB_PATH);
	let fake = Fake::holding(image(date(2026, 9, 12), STOCK));
	fake.unreadable.store(true, Ordering::SeqCst);
	assert!(put_usb_first(&fake, &layout.payload, &layout.sysfs).is_err());
	assert_eq!(fake.writes(), 0);
}
