//! The curve document on the wire and in the curve file, and CRV's rules for it.

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::{Map, Value as Json, json};

use super::{Curve, Direction, Document, FLOOR_VOLTS};

/// Why a curve document breaks CRV, in words a `refused` can carry as its reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Invalid(String);

impl Document {
	/// A document from its JSON, or why it cannot be loaded (CRV). A `charging` of `null` is taken
	/// as none.
	pub fn from_json(value: &Json) -> Result<Self, Invalid> {
		let Json::Object(document) = value else {
			return Err(invalid("the curve document is not a JSON object"));
		};
		let discharging = match document.get("discharging") {
			None | Some(Json::Null) => {
				return Err(invalid("the curve document has no discharging curve"));
			}
			Some(curve) => curve_from_json(curve, Direction::Discharging)?,
		};
		let charging = match document.get("charging") {
			None | Some(Json::Null) => None,
			Some(curve) => Some(curve_from_json(curve, Direction::Charging)?),
		};
		Self::new(discharging, charging)
	}

	/// The document in CRV's shape, with no `charging` where none is held.
	pub fn to_json(&self) -> Json {
		let mut document = Map::new();
		document.insert("discharging".to_owned(), curve_to_json(&self.discharging));
		if let Some(charging) = &self.charging {
			document.insert("charging".to_owned(), curve_to_json(charging));
		}
		Json::Object(document)
	}
}

impl Serialize for Document {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		self.to_json().serialize(serializer)
	}
}

impl<'de> Deserialize<'de> for Document {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let value = Json::deserialize(deserializer)?;
		Self::from_json(&value).map_err(de::Error::custom)
	}
}

fn curve_to_json(curve: &Curve) -> Json {
	let points: Vec<Json> = curve
		.points
		.iter()
		.map(|&(volts, charge)| json!([volts, charge]))
		.collect();
	json!({
		"points": points,
		"learnt-from": curve.learnt_from,
		"error": curve.error,
		"duration": curve.duration,
	})
}

fn curve_from_json(value: &Json, direction: Direction) -> Result<Curve, Invalid> {
	let name = direction.name();
	let Json::Object(curve) = value else {
		return Err(invalid(format!("the {name} curve is not a JSON object")));
	};
	let member = |key: &str| {
		curve
			.get(key)
			.ok_or_else(|| invalid(format!("the {name} curve has no {key}")))
	};
	let number = |key: &str| {
		member(key)?
			.as_f64()
			.ok_or_else(|| invalid(format!("the {name} curve's {key} is not a number")))
	};

	let Json::Array(listed) = member("points")? else {
		return Err(invalid(format!("the {name} curve's points are not a list")));
	};
	let points = listed
		.iter()
		.enumerate()
		.map(|(i, point)| {
			match point.as_array().map(Vec::as_slice) {
				Some([volts, charge]) => volts.as_f64().zip(charge.as_f64()),
				_ => None,
			}
			.ok_or_else(|| {
				invalid(format!(
					"the {name} curve's point {} is not a pair of numbers",
					i + 1
				))
			})
		})
		.collect::<Result<_, _>>()?;

	let learnt = number("learnt-from")?;
	if learnt.fract() != 0.0 {
		return Err(invalid(format!(
			"the {name} curve's learnt-from, {learnt}, is not a whole number"
		)));
	}
	if learnt < 0.0 {
		return Err(invalid(format!(
			"the {name} curve's learnt-from, {learnt}, is negative"
		)));
	}
	if learnt > f64::from(u32::MAX) {
		return Err(invalid(format!(
			"the {name} curve's learnt-from, {learnt}, is too large"
		)));
	}

	Ok(Curve {
		points,
		learnt_from: learnt as u32,
		error: number("error")?,
		duration: number("duration")?,
	})
}

/// CRV's rules for one curve, and for the discharging curve's ends.
pub(super) fn validate(curve: &Curve, direction: Direction) -> Result<(), Invalid> {
	let name = direction.name();
	let points = &curve.points;
	if points.len() < 2 {
		let count = match points.len() {
			0 => "no points".to_owned(),
			_ => "1 point".to_owned(),
		};
		return Err(invalid(format!(
			"the {name} curve has {count}, and needs at least 2"
		)));
	}

	for (i, &(volts, charge)) in points.iter().enumerate() {
		let n = i + 1;
		if !volts.is_finite() || !charge.is_finite() {
			return Err(invalid(format!(
				"the {name} curve's point {n} is not a pair of finite numbers"
			)));
		}
		if !(0.0..=1.0).contains(&charge) {
			return Err(invalid(format!(
				"the {name} curve's point {n}, at {volts} V, has charge {charge}, outside 0 to 1"
			)));
		}
		let Some(&(before_volts, before_charge)) = i.checked_sub(1).map(|b| &points[b]) else {
			continue;
		};
		if volts == before_volts {
			return Err(invalid(format!(
				"the {name} curve's points {i} and {n} are both at {volts} V"
			)));
		}
		if volts < before_volts {
			return Err(invalid(format!(
				"the {name} curve's point {n}, at {volts} V, is below the point before it, at \
				 {before_volts} V"
			)));
		}
		if charge < before_charge {
			return Err(invalid(format!(
				"the {name} curve's charge falls from {before_charge} at {before_volts} V to \
				 {charge} at {volts} V"
			)));
		}
	}

	if direction == Direction::Discharging {
		let (first_volts, first_charge) = points[0];
		let (last_volts, last_charge) = points[points.len() - 1];
		if first_charge != 0.0 {
			return Err(invalid(format!(
				"the {name} curve's first point, at {first_volts} V, has charge {first_charge} \
				 rather than 0"
			)));
		}
		if first_volts > FLOOR_VOLTS {
			return Err(invalid(format!(
				"the {name} curve's first point, at {first_volts} V, is above the floor of \
				 {FLOOR_VOLTS} V"
			)));
		}
		if last_charge != 1.0 {
			return Err(invalid(format!(
				"the {name} curve's last point, at {last_volts} V, has charge {last_charge} \
				 rather than 1"
			)));
		}
	}

	let error = curve.error;
	if !error.is_finite() {
		return Err(invalid(format!(
			"the {name} curve's error is not a finite number"
		)));
	}
	if !(0.0..=1.0).contains(&error) {
		return Err(invalid(format!(
			"the {name} curve's error, {error}, is outside 0 to 1"
		)));
	}
	let duration = curve.duration;
	if !duration.is_finite() {
		return Err(invalid(format!(
			"the {name} curve's duration is not a finite number"
		)));
	}
	if duration <= 0.0 {
		return Err(invalid(format!(
			"the {name} curve's duration, {duration}, is not greater than 0"
		)));
	}
	Ok(())
}

fn invalid(reason: impl Into<String>) -> Invalid {
	Invalid(reason.into())
}
