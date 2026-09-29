//! The battery curves: cell voltage to charge, for each direction of travel (CHG), and the document
//! they are read out of a device and loaded into it as (CRV).
//!
//! Charge is time at the device's own draw, since the gauge measures voltage and nothing else: 0 at
//! the discharging curve's lowest point, 1 at full, and a curve's `duration` is how long it took to
//! cover that whole scale. What is reported is renormalised between the floor and full, so 0 always
//! means the device is about to power itself off (CHG).
//!
//! The curve a build carries is `shipped-curve.json`, an interim one until a run on the production
//! cell is measured. Above 3.2 V it has the shape of HKJ's 0.5 A and 1 A discharges of a Samsung
//! INR21700-50E (lygte-info.dk), blended to the 0.136C the device draws from a 5.5 Ah 58E, since
//! the 58E's own specification (CC5563F101) publishes no curve. Below 3.2 V it is the share of the
//! 2026-09-25 v4 run-down still to go at each voltage, down to where that board gave out at 2.571 V.
//! Its duration is the 58E's 5500 mAh at the 0.75 A that run-down measured.

use std::sync::LazyLock;

use super::round;

pub use parse::Invalid;

mod parse;
pub mod store;
#[cfg(test)]
mod tests;

/// The cell voltage under load below which the device does not go on running (LOW).
pub const FLOOR_VOLTS: f64 = 2.8;

/// Places every number in a curve document is rounded to (CRV).
const PLACES: i32 = 4;

static SHIPPED: LazyLock<Document> = LazyLock::new(|| {
	let value = serde_json::from_str(include_str!("shipped-curve.json"))
		.expect("the shipped curve is JSON");
	Document::from_json(&value).expect("the shipped curve is a valid curve document")
});

/// One curve, cell voltage to charge.
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
	/// `(volts, charge)`, by rising voltage.
	pub points: Vec<(f64, f64)>,
	/// How many runs or charges refined it, 0 as a build carries it.
	pub learnt_from: u32,
	/// How far its charge is expected to be off, as a share of a full cell.
	pub error: f64,
	/// Seconds to cover its whole scale, 0 to 1.
	pub duration: f64,
}

/// Both of a device's curves. Always valid under CRV, and every number already rounded to four
/// places, so what is held is exactly what is saved and sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
	discharging: Curve,
	charging: Option<Curve>,
}

/// A time with how far either way it may be off, both in seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
	not(test),
	expect(dead_code, reason = "sent on the curve stream, still to come (T2)")
)]
pub struct Span {
	pub duration: f64,
	pub margin: f64,
}

/// Which of the two curves, for saying where a document is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
	Discharging,
	Charging,
}

impl Direction {
	fn name(self) -> &'static str {
		match self {
			Self::Discharging => "discharging",
			Self::Charging => "charging",
		}
	}
}

impl Curve {
	/// The charge at `volts`, interpolated between points and clamped to the curve's ends. A voltage
	/// that is not a number reads as the lowest point.
	pub fn charge_at(&self, volts: f64) -> f64 {
		let (Some(&(low, bottom)), Some(&(high, top))) = (self.points.first(), self.points.last())
		else {
			return 0.0;
		};
		if volts.is_nan() || volts <= low {
			return bottom;
		}
		if volts >= high {
			return top;
		}
		let above = self.points.partition_point(|&(v, _)| v <= volts);
		let (v0, c0) = self.points[above - 1];
		let (v1, c1) = self.points[above];
		c0 + (c1 - c0) * (volts - v0) / (v1 - v0)
	}

	/// The lowest voltage at which the curve reaches `charge`, clamped to the curve's ends: the
	/// voltage a cell following the curve shows, for synthetic runs.
	#[cfg(test)]
	pub fn volts_at(&self, charge: f64) -> f64 {
		let (Some(&(low, bottom)), Some(&(high, _))) = (self.points.first(), self.points.last())
		else {
			return 0.0;
		};
		if charge.is_nan() || charge <= bottom {
			return low;
		}
		let Some(reached) = self.points.iter().position(|&(_, c)| c >= charge) else {
			return high;
		};
		let (v0, c0) = self.points[reached - 1];
		let (v1, c1) = self.points[reached];
		v0 + (v1 - v0) * (charge - c0) / (c1 - c0)
	}

	/// Whether `volts` lies within the voltages the curve covers (CHG).
	pub fn covers(&self, volts: f64) -> bool {
		match (self.points.first(), self.points.last()) {
			(Some(&(low, _)), Some(&(high, _))) => (low..=high).contains(&volts),
			_ => false,
		}
	}

	/// How long the cell takes between charges `from` and `to` on this curve, with a margin of the
	/// curve's error over that time: the charge axis is time at the device's draw, so both scale
	/// with the share of the curve crossed (CHG).
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "sent on the curve stream, still to come (T2)")
	)]
	pub fn span_between(&self, from: f64, to: f64) -> Span {
		let duration = self.duration * (to - from).abs();
		Span {
			duration,
			margin: self.error * duration,
		}
	}

	fn rounded(&self) -> Self {
		Self {
			points: self
				.points
				.iter()
				.map(|&(volts, charge)| (round(volts, PLACES), round(charge, PLACES)))
				.collect(),
			learnt_from: self.learnt_from,
			error: round(self.error, PLACES),
			duration: round(self.duration, PLACES),
		}
	}
}

impl Document {
	/// A document of these curves, rounded to four places, or why it breaks CRV.
	pub fn new(discharging: Curve, charging: Option<Curve>) -> Result<Self, Invalid> {
		let discharging = discharging.rounded();
		let charging = charging.as_ref().map(Curve::rounded);
		parse::validate(&discharging, Direction::Discharging)?;
		if let Some(charging) = &charging {
			parse::validate(charging, Direction::Charging)?;
		}
		Ok(Self {
			discharging,
			charging,
		})
	}

	/// The curve the build carries, and no charging curve (CHG, CRV).
	pub fn shipped() -> Self {
		SHIPPED.clone()
	}

	pub fn discharging(&self) -> &Curve {
		&self.discharging
	}

	pub fn charging(&self) -> Option<&Curve> {
		self.charging.as_ref()
	}

	/// The discharging curve's charge at the floor, which reports as 0.
	pub fn floor_charge(&self) -> f64 {
		self.discharging.charge_at(FLOOR_VOLTS)
	}

	/// A charge from either curve as reported: its share of what lies between the floor and full,
	/// 0 at or below the floor and 1 at full (CHG). The charging curve is on the discharging curve's
	/// scale, so both renormalise alike.
	pub fn report(&self, charge: f64) -> f64 {
		let floor = self.floor_charge();
		if floor >= 1.0 {
			return if charge >= 1.0 { 1.0 } else { 0.0 };
		}
		((charge - floor) / (1.0 - floor)).clamp(0.0, 1.0)
	}

	/// How long a full charge lasts, from full down to the floor (CHG `lasts`).
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "sent on the curve stream, still to come (T2)")
	)]
	pub fn lasts(&self) -> Span {
		self.discharging.span_between(self.floor_charge(), 1.0)
	}

	/// How long a full recharge takes, from the floor up to full, where a charging curve is held
	/// (CHG `recharge`).
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "sent on the curve stream, still to come (T2)")
	)]
	pub fn recharge(&self) -> Option<Span> {
		let floor = self.floor_charge();
		self.charging
			.as_ref()
			.map(|charging| charging.span_between(floor, 1.0))
	}
}

/// The error of a curve not yet measured against a run or charge, from how many it was learnt from
/// (CHG), and of the gauge's figure on mains, as a charging curve learnt from none.
pub fn unmeasured_error(learnt_from: u32) -> f64 {
	// A fifth of a cell for one built from another cell and another board, then falling as the
	// spread of an average does, with the square root of the runs in it.
	0.2 / (f64::from(learnt_from) + 1.0).sqrt()
}
