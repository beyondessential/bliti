//! The batteries upower reports, over the system bus.
//!
//! upower rather than `/sys/class/power_supply` because a USB UPS speaking HID Power Device class
//! creates no sysfs power supply at all: the kernel leaves it to userspace, and upower reads it from
//! `/dev/usb/hiddevN`, a node only root may open. Reporting an external supply at all means asking
//! upower, which already holds that privilege.
//!
//! D-Bus is not a new dependency here: the crate already carries it for bluer, and already expects a
//! system daemon in `bluetoothd`.

use std::time::Duration;

use dbus::{
	Path,
	arg::{PropMap, prop_cast},
	blocking::{Connection, stdintf::org_freedesktop_dbus::Properties},
};

use super::battery::{Battery, Direction, worthwhile};

const SERVICE: &str = "org.freedesktop.UPower";
const ROOT: &str = "/org/freedesktop/UPower";
const DEVICE: &str = "org.freedesktop.UPower.Device";

/// How long a call may take. The sampler is synchronous, so a wedged upowerd must not hold it: the
/// call gives up and the caller falls back rather than waiting.
const TIMEOUT: Duration = Duration::from_millis(500);

/// `Type`, of which these two power the machine. The rest are line power and peripherals.
const TYPE_BATTERY: u32 = 2;
const TYPE_UPS: u32 = 3;

/// `State`, upower's own vocabulary for what the cell is doing.
const STATE_CHARGING: u32 = 1;
const STATE_DISCHARGING: u32 = 2;
const STATE_EMPTY: u32 = 3;
const STATE_FULLY_CHARGED: u32 = 4;
const STATE_PENDING_CHARGE: u32 = 5;
const STATE_PENDING_DISCHARGE: u32 = 6;

/// Every battery upower says powers this machine.
///
/// An error means upower could not be reached at all, which is the caller's cue to fall back. An
/// empty list is an answer: upower is running and this machine has no battery.
pub fn batteries() -> Result<Vec<Battery>, dbus::Error> {
	let connection = Connection::new_system()?;
	let upower = connection.with_proxy(SERVICE, ROOT, TIMEOUT);
	let (paths,): (Vec<Path<'_>>,) = upower.method_call(SERVICE, "EnumerateDevices", ())?;

	let mut batteries = Vec::new();
	for path in paths {
		let device = connection.with_proxy(SERVICE, path.clone(), TIMEOUT);
		// One device that will not answer is not a reason to lose the others.
		let Ok(properties) = device.get_all(DEVICE) else {
			continue;
		};
		if let Some(battery) = from_properties(&os_name(&path), &properties) {
			batteries.push(battery);
		}
	}
	Ok(batteries)
}

/// What upower knows the device as: the last segment of its object path, without the type prefix it
/// carries there. Unique among the devices upower reports.
fn os_name(path: &Path<'_>) -> String {
	let segment = path.rsplit('/').next().unwrap_or_default();
	for prefix in ["battery_", "ups_"] {
		if let Some(rest) = segment.strip_prefix(prefix) {
			return rest.to_owned();
		}
	}
	segment.to_owned()
}

/// Whether this device is a battery powering the machine.
///
/// `PowerSupply` is upower's own answer to that, and is what separates the machine's cell and an
/// attached UPS from a wireless mouse, which upower reports as a battery it says to ignore (NFO).
fn powers_the_device(kind: u32, power_supply: bool) -> bool {
	matches!(kind, TYPE_BATTERY | TYPE_UPS) && power_supply
}

/// The cell's direction of travel, as upower gives it. Anything upower cannot place is unknown, and
/// is skipped rather than guessed (NFO).
fn direction(state: u32) -> Direction {
	match state {
		STATE_CHARGING => Direction::Charging,
		STATE_DISCHARGING => Direction::Discharging,
		STATE_EMPTY | STATE_FULLY_CHARGED | STATE_PENDING_CHARGE | STATE_PENDING_DISCHARGE => {
			Direction::Idle
		}
		_ => Direction::Unknown,
	}
}

/// One device's properties as a battery, or nothing where it is not one that powers the machine.
fn from_properties(os_name: &str, properties: &PropMap) -> Option<Battery> {
	let kind = *prop_cast::<u32>(properties, "Type")?;
	let power_supply = prop_cast::<bool>(properties, "PowerSupply")
		.copied()
		.unwrap_or(false);
	if !powers_the_device(kind, power_supply) {
		return None;
	}
	// A bay with no cell in it is hardware that is not fitted, so it is left out entirely (NFO).
	if prop_cast::<bool>(properties, "IsPresent").copied() == Some(false) {
		return None;
	}

	let text = |key| worthwhile(prop_cast::<String>(properties, key).cloned());
	let mut battery = Battery::new(os_name);
	battery.vendor = text("Vendor");
	battery.model = text("Model");
	battery.serial = text("Serial");
	battery.charge = prop_cast::<f64>(properties, "Percentage").map(|percent| percent / 100.0);
	// upower writes an unreported voltage as zero, which a UPS commonly does.
	battery.volts = prop_cast::<f64>(properties, "Voltage")
		.copied()
		.filter(|volts| *volts > 0.0);
	battery.direction = direction(prop_cast::<u32>(properties, "State").copied().unwrap_or(0));
	Some(battery)
}

#[cfg(test)]
mod tests {
	use dbus::arg::{RefArg, Variant};

	use super::*;

	/// Properties as upower would answer with them.
	fn properties(pairs: Vec<(&str, Box<dyn RefArg>)>) -> PropMap {
		pairs
			.into_iter()
			.map(|(key, value)| (key.to_owned(), Variant(value)))
			.collect()
	}

	fn laptop_cell() -> PropMap {
		properties(vec![
			("Type", Box::new(TYPE_BATTERY)),
			("PowerSupply", Box::new(true)),
			("IsPresent", Box::new(true)),
			("Vendor", Box::new("LGC-LGC3.67".to_owned())),
			("Model", Box::new("DELL T453X".to_owned())),
			("Serial", Box::new("109".to_owned())),
			("Percentage", Box::new(100.0f64)),
			("Voltage", Box::new(12.889f64)),
			("State", Box::new(STATE_FULLY_CHARGED)),
		])
	}

	/// The Eaton 3S as upower reports it: charge and state, no voltage, a placeholder serial.
	fn attached_ups() -> PropMap {
		properties(vec![
			("Type", Box::new(TYPE_UPS)),
			("PowerSupply", Box::new(true)),
			("IsPresent", Box::new(true)),
			("Vendor", Box::new("Eaton".to_owned())),
			("Model", Box::new("Eaton 3S".to_owned())),
			("Serial", Box::new("Blank".to_owned())),
			("Percentage", Box::new(100.0f64)),
			("Voltage", Box::new(0.0f64)),
			("State", Box::new(STATE_FULLY_CHARGED)),
		])
	}

	/// The wireless mouse: a battery by type, but not one that powers the machine.
	fn wireless_mouse() -> PropMap {
		properties(vec![
			("Type", Box::new(TYPE_BATTERY)),
			("PowerSupply", Box::new(false)),
			("IsPresent", Box::new(true)),
			("Model", Box::new("Logitech MX Vertical".to_owned())),
			("Percentage", Box::new(55.0f64)),
			("State", Box::new(STATE_DISCHARGING)),
		])
	}

	#[test]
	fn the_machines_own_cell_is_a_battery() {
		let battery = from_properties("BAT0", &laptop_cell()).unwrap();
		assert_eq!(battery.model.as_deref(), Some("DELL T453X"));
		assert_eq!(battery.serial.as_deref(), Some("109"));
		assert_eq!(battery.charge, Some(1.0));
		assert_eq!(battery.volts, Some(12.889));
		assert_eq!(battery.direction, Direction::Idle);
	}

	/// An external supply carrying the device is reported, and reports no voltage (NFO).
	#[test]
	fn an_attached_ups_is_a_battery_with_no_voltage() {
		let battery = from_properties("hiddev5", &attached_ups()).unwrap();
		assert_eq!(battery.model.as_deref(), Some("Eaton 3S"));
		assert_eq!(
			battery.volts, None,
			"upower writes an unreported voltage as zero"
		);
		assert_eq!(battery.serial, None, "a placeholder serial says nothing");
		assert_eq!(battery.charge, Some(1.0));
	}

	/// A peripheral's cell powers the peripheral, not the device (NFO).
	#[test]
	fn a_peripheral_is_not_a_battery_of_this_device() {
		assert!(from_properties("hidpp_battery_0", &wireless_mouse()).is_none());
	}

	#[test]
	fn line_power_is_not_a_battery() {
		let mains = properties(vec![
			("Type", Box::new(1u32)),
			("PowerSupply", Box::new(true)),
		]);
		assert!(from_properties("AC", &mains).is_none());
	}

	/// An empty bay is hardware that is not fitted, so nothing is reported for it (NFO).
	#[test]
	fn an_absent_cell_is_not_reported() {
		let empty = properties(vec![
			("Type", Box::new(TYPE_BATTERY)),
			("PowerSupply", Box::new(true)),
			("IsPresent", Box::new(false)),
		]);
		assert!(from_properties("BAT1", &empty).is_none());
	}

	#[test]
	fn the_states_map_onto_the_three_directions() {
		assert_eq!(direction(STATE_CHARGING), Direction::Charging);
		assert_eq!(direction(STATE_DISCHARGING), Direction::Discharging);
		assert_eq!(direction(STATE_FULLY_CHARGED), Direction::Idle);
		assert_eq!(direction(STATE_PENDING_CHARGE), Direction::Idle);
		assert_eq!(direction(STATE_PENDING_DISCHARGE), Direction::Idle);
		assert_eq!(direction(STATE_EMPTY), Direction::Idle);
	}

	/// upower saying it does not know is not a direction to report (NFO).
	#[test]
	fn an_unknown_state_is_unknown() {
		assert_eq!(direction(0), Direction::Unknown);
		assert_eq!(direction(99), Direction::Unknown);
	}

	#[test]
	fn the_object_path_gives_the_system_name() {
		assert_eq!(
			os_name(&Path::new("/org/freedesktop/UPower/devices/battery_BAT0").unwrap()),
			"BAT0"
		);
		assert_eq!(
			os_name(&Path::new("/org/freedesktop/UPower/devices/ups_hiddev5").unwrap()),
			"hiddev5"
		);
		assert_eq!(
			os_name(&Path::new("/org/freedesktop/UPower/devices/battery_hidpp_battery_0").unwrap()),
			"hidpp_battery_0",
			"only the type prefix is stripped"
		);
	}
}
