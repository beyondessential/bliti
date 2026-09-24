//! The wired interfaces a device has, and the configuration it holds before any is recorded (CFG).

use std::{fs, path::Path};

use serde_json::{Map, Value as Json, json};

/// Where the kernel lists network interfaces.
pub const SYS_CLASS_NET: &str = "/sys/class/net";

/// The Ethernet `type` of an interface, `ARPHRD_ETHER`.
const ETHERNET: &str = "1";

/// The wired interfaces under `root`, laid out as `/sys/class/net`, sorted by name.
///
/// Wired means physical (a `device` link to the hardware beneath it, which loopback, bridges, tunnels
/// and every other virtual interface lack), Ethernet by `type`, and not wireless, which reports
/// `type` 1 too but carries `wireless` or `phy80211`.
pub fn interfaces(root: &Path) -> Vec<String> {
	let Ok(entries) = fs::read_dir(root) else {
		return Vec::new();
	};
	let mut wired: Vec<String> = entries
		.filter_map(Result::ok)
		.filter(|entry| {
			let path = entry.path();
			path.join("device").exists()
				&& fs::read_to_string(path.join("type")).is_ok_and(|kind| kind.trim() == ETHERNET)
				&& !path.join("wireless").exists()
				&& !path.join("phy80211").exists()
		})
		.filter_map(|entry| entry.file_name().into_string().ok())
		.collect();
	wired.sort();
	wired
}

/// What a device that has never recorded a configuration holds as its recorded one: a `wired-dynamic`
/// candidate on each wired interface, labelled by the interface, and no hotspot (CFG).
pub fn unconfigured(root: &Path) -> Map<String, Json> {
	let attachments = interfaces(root)
		.into_iter()
		.map(
			|interface| json!({"kind": "wired-dynamic", "label": interface, "enabled": true, "verify": true, "interface": interface}),
		)
		.collect();
	Map::from_iter([("attachments".to_owned(), Json::Array(attachments))])
}

#[cfg(test)]
mod tests {
	use std::{
		path::PathBuf,
		sync::atomic::{AtomicUsize, Ordering},
	};

	use super::*;

	/// A fake `/sys/class/net`, removed when dropped.
	struct Sys(PathBuf);

	impl Sys {
		fn new() -> Self {
			static NEXT: AtomicUsize = AtomicUsize::new(0);
			let n = NEXT.fetch_add(1, Ordering::Relaxed);
			let root =
				std::env::temp_dir().join(format!("bliti-sys-net-{}-{n}", std::process::id()));
			let _ = fs::remove_dir_all(&root);
			fs::create_dir_all(&root).unwrap();
			Self(root)
		}

		/// An interface of `kind`, backed by hardware where `physical`, carrying `extra` entries.
		fn interface(&self, name: &str, kind: u16, physical: bool, extra: &[&str]) -> &Self {
			let dir = self.0.join(name);
			fs::create_dir_all(&dir).unwrap();
			fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
			if physical {
				fs::create_dir_all(dir.join("device")).unwrap();
			}
			for entry in extra {
				fs::create_dir_all(dir.join(entry)).unwrap();
			}
			self
		}
	}

	impl Drop for Sys {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn only_physical_ethernet_that_is_not_wireless_is_wired() {
		let sys = Sys::new();
		sys.interface("lo", 772, false, &[])
			.interface("eth1", 1, true, &[])
			.interface("enp1s0", 1, true, &[])
			.interface("wlan0", 1, true, &["wireless", "phy80211"])
			.interface("wlan1", 1, true, &["phy80211"])
			.interface("br0", 1, false, &["bridge"])
			.interface("tailscale0", 65534, false, &[])
			.interface("usb-serial", 256, true, &[]);
		assert_eq!(interfaces(&sys.0), ["enp1s0", "eth1"]);
	}

	#[test]
	fn the_unconfigured_default_is_a_dynamic_candidate_per_wired_interface_and_no_hotspot() {
		let sys = Sys::new();
		sys.interface("eth0", 1, true, &[])
			.interface("wlan0", 1, true, &["wireless"]);
		assert_eq!(
			Json::Object(unconfigured(&sys.0)),
			json!({"attachments": [
				{"kind": "wired-dynamic", "label": "eth0", "enabled": true, "verify": true, "interface": "eth0"},
			]})
		);
		assert!(bliti_core::channel::config::Document::parse(&unconfigured(&sys.0)).is_ok());
	}

	#[test]
	fn a_device_with_no_wired_interface_holds_no_candidate() {
		let sys = Sys::new();
		assert_eq!(
			Json::Object(unconfigured(&sys.0)),
			json!({"attachments": []})
		);
		assert_eq!(
			Json::Object(unconfigured(&sys.0.join("missing"))),
			json!({"attachments": []})
		);
	}
}
