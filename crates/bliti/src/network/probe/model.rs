//! What an adapter is, as sysfs says: its driver, and the product on the bus it sits on.

use std::{
	fs,
	path::{Path, PathBuf},
};

/// What sysfs says of the device behind a network interface.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Sysfs {
	/// The kernel driver bound to it.
	pub driver: Option<String>,
	/// The bus it sits on, as the kernel names it (`pci`, `usb`, `sdio`).
	pub bus: Option<String>,
	/// The vendor ID, in hex.
	pub vendor: Option<String>,
	/// The product or device ID, in hex.
	pub device: Option<String>,
	/// The product's own name, which USB devices carry.
	pub product: Option<String>,
}

impl Sysfs {
	/// Read what `root` (`/sys` on a device) says of the device behind `interface`.
	pub fn read(root: &Path, interface: &str) -> Self {
		let device = root.join("class/net").join(interface).join("device");
		let bus = link_name(&device.join("subsystem"));
		// A USB network interface's device is the USB interface; the IDs and the product's name are
		// on the device it belongs to.
		let ids: PathBuf = if bus.as_deref() == Some("usb") {
			fs::canonicalize(&device)
				.ok()
				.and_then(|path| path.parent().map(Path::to_path_buf))
				.unwrap_or_else(|| device.clone())
		} else {
			device.clone()
		};
		let (vendor, id) = if bus.as_deref() == Some("usb") {
			("idVendor", "idProduct")
		} else {
			("vendor", "device")
		};
		Self {
			driver: link_name(&device.join("driver")),
			bus,
			vendor: attribute(&ids.join(vendor)).map(|v| hex(&v)),
			device: attribute(&ids.join(id)).map(|v| hex(&v)),
			product: attribute(&ids.join("product")),
		}
	}

	/// The model as capabilities state it: `iwlwifi (PCI 8086:06f0)`, say, or
	/// `rtl8xxxu (USB 0bda:8179, 802.11n NIC)`.
	pub fn model(&self) -> String {
		let driver = self.driver.as_deref().unwrap_or("unknown driver");
		let mut detail = Vec::new();
		match (&self.bus, &self.vendor, &self.device) {
			(Some(bus), Some(vendor), Some(device)) => {
				detail.push(format!("{} {vendor}:{device}", bus.to_uppercase()));
			}
			(Some(bus), _, _) => detail.push(bus.to_uppercase()),
			_ => {}
		}
		detail.extend(self.product.clone());
		if detail.is_empty() {
			driver.to_owned()
		} else {
			format!("{driver} ({})", detail.join(", "))
		}
	}
}

fn link_name(path: &Path) -> Option<String> {
	let target = fs::read_link(path).ok()?;
	Some(target.file_name()?.to_str()?.to_owned())
}

fn attribute(path: &Path) -> Option<String> {
	let text = fs::read_to_string(path).ok()?;
	let text = text.trim();
	(!text.is_empty()).then(|| text.to_owned())
}

fn hex(value: &str) -> String {
	value.trim_start_matches("0x").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_pci_adapter_is_its_driver_and_ids() {
		let sysfs = Sysfs {
			driver: Some("iwlwifi".into()),
			bus: Some("pci".into()),
			vendor: Some("8086".into()),
			device: Some("06f0".into()),
			product: None,
		};
		assert_eq!(sysfs.model(), "iwlwifi (PCI 8086:06f0)");
	}

	#[test]
	fn a_usb_adapter_carries_its_product_name() {
		let sysfs = Sysfs {
			driver: Some("rtl8xxxu".into()),
			bus: Some("usb".into()),
			vendor: Some("0bda".into()),
			device: Some("8179".into()),
			product: Some("802.11n NIC".into()),
		};
		assert_eq!(sysfs.model(), "rtl8xxxu (USB 0bda:8179, 802.11n NIC)");
	}

	#[test]
	fn nothing_readable_is_still_a_model() {
		assert_eq!(Sysfs::default().model(), "unknown driver");
	}

	#[cfg(unix)]
	#[test]
	fn sysfs_is_read_through_its_links() {
		use std::os::unix::fs::symlink;

		let root = std::env::temp_dir().join(format!("bliti-probe-sysfs-{}", std::process::id()));
		let _ = fs::remove_dir_all(&root);
		let usb = root.join("devices/usb1/1-1");
		let interface = usb.join("1-1:1.0");
		fs::create_dir_all(&interface).unwrap();
		fs::create_dir_all(root.join("bus/usb/drivers/rtl8xxxu")).unwrap();
		fs::write(usb.join("idVendor"), "0bda\n").unwrap();
		fs::write(usb.join("idProduct"), "8179\n").unwrap();
		fs::write(usb.join("product"), "802.11n NIC\n").unwrap();
		symlink(
			root.join("bus/usb/drivers/rtl8xxxu"),
			interface.join("driver"),
		)
		.unwrap();
		symlink(root.join("bus/usb"), interface.join("subsystem")).unwrap();
		let net = root.join("class/net/wlx00c0caa1b2c3");
		fs::create_dir_all(&net).unwrap();
		symlink(&interface, net.join("device")).unwrap();

		let read = Sysfs::read(&root, "wlx00c0caa1b2c3");
		fs::remove_dir_all(&root).unwrap();
		assert_eq!(read.model(), "rtl8xxxu (USB 0bda:8179, 802.11n NIC)");
	}
}
