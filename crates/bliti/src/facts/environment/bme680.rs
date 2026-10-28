//! The Bosch BME680: the temperature, humidity and pressure of the air inside the case.
//!
//! Driven through `bosch-bme680`, which does the datasheet's compensation. Its gas sensor is never
//! run, so its heater stays off (NFO): it would warm the air being measured, and the resistance it
//! yields means little without Bosch's closed air quality library.

use std::fmt;

use bliti_core::channel::readings::{Entry, kind};
use bosch_bme680::{
	BmeError, Configuration, DeviceAddress, IIRFilter, MeasurmentData, Oversampling,
};
use serde_json::Value as Json;

use super::Problem;
use crate::facts::i2c::{Bus, Sleep};

const CHIP: &str = "BME680";

type Driver = bosch_bme680::Bme680<Bus, Sleep>;

pub struct Bme680 {
	bus: &'static str,
	/// Opened on the first tick, and dropped after an error so the next tick starts afresh.
	driver: Option<Driver>,
}

impl fmt::Debug for Bme680 {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("Bme680")
			.field("bus", &self.bus)
			.field("open", &self.driver.is_some())
			.finish()
	}
}

impl Bme680 {
	pub fn new(bus: &'static str) -> Self {
		Self { bus, driver: None }
	}

	/// `temperature` on the `case` sensor, `humidity` and `air-pressure`, from one forced
	/// measurement.
	// spec: NFO
	pub fn readings(&mut self, at: u64) -> Vec<Entry> {
		match self.measure() {
			Ok(data) => entries(at, &data),
			Err(problem) => {
				self.driver = None;
				vec![
					case(problem.entry(at, "temperature", kind::QUANTITY)),
					problem.entry(at, "humidity", kind::FRACTION),
					problem.entry(at, "air-pressure", kind::QUANTITY),
				]
			}
		}
	}

	fn measure(&mut self) -> Result<MeasurmentData, Problem> {
		let driver = match &mut self.driver {
			Some(driver) => driver,
			None => self.driver.insert(open(self.bus)?),
		};
		driver.measure().map_err(|err| problem(self.bus, err))
	}
}

/// Open the bus and the sensor on it. Resets the chip and checks it is a BME680.
fn open(bus: &str) -> Result<Driver, Problem> {
	let handle = Bus::open(bus).map_err(|err| Problem::bus(CHIP, bus, &err))?;
	// The ambient temperature only sets the gas heater's target, and the heater never runs.
	Driver::new(handle, DeviceAddress::Secondary, Sleep, &config(), 20)
		.map_err(|err| problem(bus, err))
}

/// With no gas configuration the driver leaves the gas run off, as the soft reset in opening sets it.
// spec: NFO
fn config() -> Configuration {
	Configuration::builder()
		.temperature_oversampling(Oversampling::By2)
		.humidity_oversampling(Oversampling::By1)
		.pressure_oversampling(Oversampling::By16)
		.filter(IIRFilter::Coeff3)
		.gas_config(None)
		.build()
}

fn problem(bus: &str, err: BmeError<Bus>) -> Problem {
	match err {
		BmeError::WriteError(err) | BmeError::WriteReadError(err) => Problem::bus(CHIP, bus, &err),
		BmeError::UnexpectedChipId(id) => Problem::wrong_part(CHIP, id),
		BmeError::MeasuringTimeOut => Problem::unfinished(CHIP),
	}
}

fn entries(at: u64, data: &MeasurmentData) -> Vec<Entry> {
	let humidity = f64::from(data.humidity) / 100.0;
	vec![
		case(Entry::quantity(
			at,
			"temperature",
			"celsius",
			f64::from(data.temperature),
		)),
		Entry::fraction(at, "humidity", humidity.clamp(0.0, 1.0)),
		Entry::quantity(
			at,
			"air-pressure",
			"pascals",
			f64::from(data.pressure) * 100.0,
		),
	]
}

fn case(entry: Entry) -> Entry {
	entry.with_trait("sensor", Json::String("case".to_owned()))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn data(temperature: f32, humidity: f32, pressure: f32) -> MeasurmentData {
		MeasurmentData {
			temperature,
			humidity,
			pressure,
			gas_resistance: None,
		}
	}

	fn value(entry: &Entry) -> f64 {
		entry.value.as_ref().and_then(Json::as_f64).unwrap()
	}

	// spec: NFO
	#[test]
	fn a_measurement_is_reported_in_the_catalogue_s_units() {
		let entries = entries(1, &data(31.5, 42.25, 1013.25));
		let [temperature, humidity, pressure] = entries.as_slice() else {
			panic!("{entries:?}");
		};

		assert_eq!(temperature.name, "temperature");
		assert_eq!(temperature.unit.as_deref(), Some("celsius"));
		assert_eq!(value(temperature), 31.5);
		assert_eq!(temperature.traits.get("sensor"), Some(&Json::from("case")));

		assert_eq!(humidity.name, "humidity");
		assert_eq!(humidity.kind, kind::FRACTION);
		assert_eq!(value(humidity), 0.4225);

		assert_eq!(pressure.name, "air-pressure");
		assert_eq!(pressure.unit.as_deref(), Some("pascals"));
		assert!((value(pressure) - 101_325.0).abs() < 0.01, "{pressure:?}");

		for entry in &entries {
			assert_eq!(entry.status(), Some("passed"), "{entry:?}");
		}
	}

	/// The compensation can land a touch outside 0–100 % at the extremes, and a fraction can't.
	#[test]
	fn humidity_is_held_to_a_fraction() {
		assert_eq!(value(&entries(1, &data(20.0, 100.4, 1000.0))[1]), 1.0);
		assert_eq!(value(&entries(1, &data(20.0, -0.2, 1000.0))[1]), 0.0);
	}

	// spec: NFO
	#[test]
	fn a_sensor_that_does_not_answer_reports_all_three_broken() {
		let mut sensor = Bme680::new("/nonexistent/bliti/i2c-3");
		let entries = sensor.readings(1);
		assert_eq!(
			entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
			["temperature", "humidity", "air-pressure"]
		);
		for entry in &entries {
			assert_eq!(entry.status(), Some("broken"), "{entry:?}");
			assert!(entry.reason().unwrap().contains(CHIP), "{entry:?}");
		}
		assert_eq!(entries[0].traits.get("sensor"), Some(&Json::from("case")));
	}

	// spec: NFO
	#[test]
	fn the_gas_heater_never_runs() {
		assert_eq!(config().gas_config, None);
	}

	#[test]
	fn another_part_at_the_address_is_named_in_the_reason() {
		let problem = problem("/dev/i2c-3", BmeError::UnexpectedChipId(0x58));
		let entry = problem.entry(1, "humidity", kind::FRACTION);
		assert_eq!(entry.status(), Some("broken"));
		assert!(entry.reason().unwrap().contains("0x58"));
	}
}
