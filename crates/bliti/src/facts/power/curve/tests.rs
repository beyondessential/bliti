use std::{
	fs,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};

use serde_json::{Value as Json, json};

use super::{
	store::{Store, StoreError, Stored},
	*,
};

fn curve(points: &[(f64, f64)]) -> Curve {
	Curve {
		points: points.to_vec(),
		learnt_from: 2,
		error: 0.1,
		duration: 1000.0,
	}
}

/// Floor at 2.8 V sits at charge 0.2, a quarter of the way from 2.6 V to 3.4 V.
fn document() -> Document {
	Document::new(curve(&[(2.6, 0.0), (3.4, 0.8), (4.2, 1.0)]), None).unwrap()
}

fn close(a: f64, b: f64) -> bool {
	(a - b).abs() < 1e-9
}

fn reason(document: Json) -> String {
	Document::from_json(&document).unwrap_err().to_string()
}

fn with_discharging(member: &str, value: Json) -> Json {
	let mut document = document().to_json();
	document["discharging"][member] = value;
	document
}

#[test]
fn the_shipped_curve_is_valid_as_built() {
	let shipped = Document::shipped();
	assert_eq!(shipped.discharging().learnt_from, 0);
	assert_eq!(shipped.discharging().error, unmeasured_error(0));
	assert!(shipped.charging().is_none());
	assert!(shipped.discharging().points[0].0 <= FLOOR_VOLTS);
	// Stored as built: its JSON carries nothing rounding would change.
	let json: Json = serde_json::from_str(include_str!("../shipped-curve.json")).unwrap();
	assert_eq!(Document::from_json(&json).unwrap().to_json(), json);
}

#[test]
fn the_shipped_curve_reports_empty_at_the_floor_and_full_at_the_top() {
	let shipped = Document::shipped();
	let charge = |volts| shipped.report(shipped.discharging().charge_at(volts));
	assert_eq!(charge(FLOOR_VOLTS), 0.0);
	assert_eq!(charge(2.6), 0.0);
	assert_eq!(charge(4.2), 1.0);
	assert!(charge(3.2) > 0.1 && charge(3.2) < 0.3);
}

#[test]
fn charge_interpolates_between_points_and_clamps_at_the_ends() {
	let curve = document().discharging().clone();
	assert!(close(curve.charge_at(3.0), 0.4));
	assert!(close(curve.charge_at(3.8), 0.9));
	assert_eq!(curve.charge_at(3.4), 0.8);
	assert_eq!(curve.charge_at(2.0), 0.0);
	assert_eq!(curve.charge_at(5.0), 1.0);
	assert_eq!(curve.charge_at(f64::NAN), 0.0);
}

#[test]
fn volts_are_the_inverse_of_charge() {
	let curve = curve(&[(2.6, 0.0), (3.0, 0.5), (3.5, 0.5), (4.2, 1.0)]);
	assert!(close(curve.volts_at(0.25), 2.8));
	assert!(close(curve.volts_at(0.75), 3.85));
	// Where the curve is flat, the lowest voltage that reaches the charge.
	assert!(close(curve.volts_at(0.5), 3.0));
	assert_eq!(curve.volts_at(-1.0), 2.6);
	assert_eq!(curve.volts_at(2.0), 4.2);
}

#[test]
fn a_curve_covers_only_its_own_voltages() {
	let curve = curve(&[(3.0, 0.2), (4.0, 1.0)]);
	assert!(curve.covers(3.0) && curve.covers(3.5) && curve.covers(4.0));
	assert!(!curve.covers(2.9) && !curve.covers(4.1));
}

#[test]
fn reported_charge_runs_from_the_floor_to_full() {
	let document = document();
	assert!(close(document.floor_charge(), 0.2));
	assert_eq!(document.report(0.0), 0.0);
	assert!(close(document.report(0.2), 0.0));
	assert!(close(document.report(0.6), 0.5));
	assert_eq!(document.report(1.0), 1.0);
	assert_eq!(document.report(1.5), 1.0);
}

#[test]
fn a_charging_curve_renormalises_on_the_discharging_scale() {
	let document = Document::new(
		document().discharging().clone(),
		Some(curve(&[(3.5, 0.2), (4.2, 1.0)])),
	)
	.unwrap();
	let charging = document.charging().unwrap();
	assert!(close(document.report(charging.charge_at(3.5)), 0.0));
	assert!(close(document.report(charging.charge_at(3.85)), 0.5));
}

#[test]
fn a_floor_at_full_reports_only_full() {
	let document = Document::new(curve(&[(2.5, 0.0), (2.7, 1.0)]), None).unwrap();
	assert_eq!(document.report(0.99), 0.0);
	assert_eq!(document.report(1.0), 1.0);
}

#[test]
fn a_full_charge_lasts_the_duration_between_the_floor_and_full() {
	let lasts = document().lasts();
	assert!(close(lasts.duration, 800.0));
	assert!(close(lasts.margin, 80.0));
	assert!(document().recharge().is_none());
}

#[test]
fn a_full_recharge_takes_the_charging_duration_between_the_floor_and_full() {
	let charging = Curve {
		duration: 2000.0,
		error: 0.05,
		..curve(&[(3.5, 0.2), (4.2, 1.0)])
	};
	let document = Document::new(document().discharging().clone(), Some(charging)).unwrap();
	let recharge = document.recharge().unwrap();
	assert!(close(recharge.duration, 1600.0));
	assert!(close(recharge.margin, 80.0));
}

#[test]
fn the_unmeasured_error_falls_with_every_run() {
	assert_eq!(unmeasured_error(0), 0.2);
	assert!(close(unmeasured_error(3), 0.1));
	let mut last = unmeasured_error(0);
	for n in 1..20 {
		assert!(unmeasured_error(n) < last);
		last = unmeasured_error(n);
	}
	assert!(unmeasured_error(u32::MAX) > 0.0);
}

#[test]
fn a_document_round_trips_in_the_shape_crv_gives() {
	let document = Document::new(
		document().discharging().clone(),
		Some(curve(&[(3.5, 0.2), (4.2, 1.0)])),
	)
	.unwrap();
	let json = serde_json::to_value(&document).unwrap();
	assert_eq!(
		json["discharging"],
		json!({
			"points": [[2.6, 0.0], [3.4, 0.8], [4.2, 1.0]],
			"learnt-from": 2,
			"error": 0.1,
			"duration": 1000.0,
		})
	);
	assert!(json["charging"].is_object());
	let back: Document = serde_json::from_value(json).unwrap();
	assert_eq!(back, document);
}

#[test]
fn no_charging_curve_is_no_member() {
	let json = document().to_json();
	assert!(json.get("charging").is_none());
	let mut with_null = json.clone();
	with_null["charging"] = Json::Null;
	assert_eq!(Document::from_json(&with_null).unwrap(), document());
}

#[test]
fn every_number_is_rounded_to_four_places() {
	let document = Document::new(
		Curve {
			points: vec![(2.612345, 0.0), (3.123456, 0.333333), (4.2, 1.0)],
			learnt_from: 1,
			error: 0.123456,
			duration: 26400.123456,
		},
		None,
	)
	.unwrap();
	let text = serde_json::to_string(&document).unwrap();
	assert_eq!(
		text,
		r#"{"discharging":{"duration":26400.1235,"error":0.1235,"learnt-from":1,"points":[[2.6123,0.0],[3.1235,0.3333],[4.2,1.0]]}}"#
	);
}

#[test]
fn rounding_comes_before_the_rules() {
	// Two points apart only past the fourth place are at one voltage once rounded.
	let err = Document::new(
		curve(&[(2.6, 0.0), (3.00001, 0.5), (3.00002, 0.5), (4.2, 1.0)]),
		None,
	)
	.unwrap_err();
	assert_eq!(
		err.to_string(),
		"the discharging curve's points 2 and 3 are both at 3 V"
	);
}

#[test]
fn a_document_that_is_not_one_is_refused() {
	assert_eq!(
		reason(json!([1, 2])),
		"the curve document is not a JSON object"
	);
	assert_eq!(
		reason(json!({})),
		"the curve document has no discharging curve"
	);
	assert_eq!(
		reason(json!({"discharging": 3})),
		"the discharging curve is not a JSON object"
	);
	let mut document = document().to_json();
	document["charging"] = json!("soon");
	assert_eq!(reason(document), "the charging curve is not a JSON object");
}

#[test]
fn every_member_of_a_curve_is_required() {
	for member in ["points", "learnt-from", "error", "duration"] {
		let mut document = document().to_json();
		document["discharging"]
			.as_object_mut()
			.unwrap()
			.remove(member);
		assert_eq!(
			reason(document),
			format!("the discharging curve has no {member}")
		);
	}
}

#[test]
fn members_of_the_wrong_type_are_refused() {
	assert_eq!(
		reason(with_discharging("points", json!({}))),
		"the discharging curve's points are not a list"
	);
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.0], [4.2, 1.0]])
		)),
		"the discharging curve's point 2 is not a pair of numbers"
	);
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.0, "half"], [4.2, 1.0]])
		)),
		"the discharging curve's point 2 is not a pair of numbers"
	);
	assert_eq!(
		reason(with_discharging("error", json!("small"))),
		"the discharging curve's error is not a number"
	);
}

#[test]
fn a_curve_needs_two_points() {
	assert_eq!(
		reason(with_discharging("points", json!([[2.6, 0.0]]))),
		"the discharging curve has 1 point, and needs at least 2"
	);
	assert_eq!(
		reason(with_discharging("points", json!([]))),
		"the discharging curve has no points, and needs at least 2"
	);
}

#[test]
fn points_rise_in_voltage_with_none_at_one_voltage() {
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.6, 0.5], [3.5, 0.6], [4.2, 1.0]])
		)),
		"the discharging curve's point 3, at 3.5 V, is below the point before it, at 3.6 V"
	);
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.5, 0.5], [3.5, 0.6], [4.2, 1.0]])
		)),
		"the discharging curve's points 2 and 3 are both at 3.5 V"
	);
}

#[test]
fn charge_stays_within_a_cell_and_never_falls() {
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.5, 1.2], [4.2, 1.0]])
		)),
		"the discharging curve's point 2, at 3.5 V, has charge 1.2, outside 0 to 1"
	);
	assert_eq!(
		reason(with_discharging(
			"points",
			json!([[2.6, 0.0], [3.5, 0.6], [3.6, 0.5], [4.2, 1.0]])
		)),
		"the discharging curve's charge falls from 0.6 at 3.5 V to 0.5 at 3.6 V"
	);
	let mut document = document().to_json();
	document["charging"] = json!({
		"points": [[3.5, -0.1], [4.2, 1.0]],
		"learnt-from": 0, "error": 0.2, "duration": 100,
	});
	assert_eq!(
		reason(document),
		"the charging curve's point 1, at 3.5 V, has charge -0.1, outside 0 to 1"
	);
}

#[test]
fn the_discharging_curve_runs_from_the_floor_to_full() {
	assert_eq!(
		reason(with_discharging("points", json!([[2.6, 0.1], [4.2, 1.0]]))),
		"the discharging curve's first point, at 2.6 V, has charge 0.1 rather than 0"
	);
	assert_eq!(
		reason(with_discharging("points", json!([[2.9, 0.0], [4.2, 1.0]]))),
		"the discharging curve's first point, at 2.9 V, is above the floor of 2.8 V"
	);
	assert_eq!(
		reason(with_discharging("points", json!([[2.6, 0.0], [4.1, 0.9]]))),
		"the discharging curve's last point, at 4.1 V, has charge 0.9 rather than 1"
	);
	// At the floor itself is allowed.
	assert!(
		Document::from_json(&with_discharging("points", json!([[2.8, 0.0], [4.2, 1.0]]))).is_ok()
	);
}

#[test]
fn a_charging_curve_need_not_span_the_scale() {
	let mut document = document().to_json();
	document["charging"] = json!({
		"points": [[3.6, 0.3], [4.1, 0.9]],
		"learnt-from": 3, "error": 0.05, "duration": 7200,
	});
	assert!(Document::from_json(&document).is_ok());
}

#[test]
fn learnt_from_is_a_whole_number() {
	assert_eq!(
		reason(with_discharging("learnt-from", json!(1.5))),
		"the discharging curve's learnt-from, 1.5, is not a whole number"
	);
	assert_eq!(
		reason(with_discharging("learnt-from", json!(-1))),
		"the discharging curve's learnt-from, -1, is negative"
	);
	assert_eq!(
		reason(with_discharging("learnt-from", json!(1e12))),
		"the discharging curve's learnt-from, 1000000000000, is too large"
	);
	assert!(Document::from_json(&with_discharging("learnt-from", json!(3.0))).is_ok());
}

#[test]
fn error_is_a_share_of_a_cell() {
	assert_eq!(
		reason(with_discharging("error", json!(1.5))),
		"the discharging curve's error, 1.5, is outside 0 to 1"
	);
	assert_eq!(
		reason(with_discharging("error", json!(-0.1))),
		"the discharging curve's error, -0.1, is outside 0 to 1"
	);
	assert!(Document::from_json(&with_discharging("error", json!(0))).is_ok());
	assert!(Document::from_json(&with_discharging("error", json!(1))).is_ok());
}

#[test]
fn duration_is_greater_than_zero() {
	assert_eq!(
		reason(with_discharging("duration", json!(0))),
		"the discharging curve's duration, 0, is not greater than 0"
	);
	assert_eq!(
		reason(with_discharging("duration", json!(-5))),
		"the discharging curve's duration, -5, is not greater than 0"
	);
}

#[test]
fn numbers_a_document_cannot_carry_are_refused_when_built() {
	let refused = |curve: Curve| Document::new(curve, None).unwrap_err().to_string();
	assert_eq!(
		refused(curve(&[(2.6, 0.0), (f64::NAN, 0.5), (4.2, 1.0)])),
		"the discharging curve's point 2 is not a pair of finite numbers"
	);
	assert_eq!(
		refused(Curve {
			error: f64::NAN,
			..curve(&[(2.6, 0.0), (4.2, 1.0)])
		}),
		"the discharging curve's error is not a finite number"
	);
	assert_eq!(
		refused(Curve {
			duration: f64::INFINITY,
			..curve(&[(2.6, 0.0), (4.2, 1.0)])
		}),
		"the discharging curve's duration is not a finite number"
	);
}

/// A directory of its own for one test, removed when it ends.
struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Self {
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let n = NEXT.fetch_add(1, Ordering::Relaxed);
		let dir = std::env::temp_dir().join(format!("bliti-curve-{}-{n}", std::process::id()));
		let _ = fs::remove_dir_all(&dir);
		Self(dir)
	}

	fn file(&self) -> PathBuf {
		self.0.join("state").join("battery-curve.json")
	}

	fn store(&self) -> Store {
		Store::new(self.file())
	}

	fn write(&self, body: &str) {
		fs::create_dir_all(self.file().parent().unwrap()).unwrap();
		fs::write(self.file(), body).unwrap();
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[test]
fn no_file_starts_from_the_shipped_curve() {
	let scratch = Scratch::new();
	assert!(scratch.store().read().unwrap().is_none());
	assert_eq!(scratch.store().load(), Stored::default());
	assert_eq!(scratch.store().load().document, Document::shipped());
}

#[test]
fn a_file_that_cannot_be_used_starts_from_the_shipped_curve() {
	let scratch = Scratch::new();
	scratch.write("{not json");
	assert!(matches!(scratch.store().read(), Err(StoreError::Json(..))));
	assert_eq!(scratch.store().load(), Stored::default());

	let above_the_floor = with_discharging("points", json!([[2.9, 0.0], [4.2, 1.0]]));
	scratch.write(&json!({ "document": above_the_floor }).to_string());
	let err = scratch.store().read().unwrap_err();
	assert!(matches!(err, StoreError::Invalid(..)));
	assert!(err.to_string().ends_with("is above the floor of 2.8 V"));
	assert_eq!(scratch.store().load(), Stored::default());

	scratch.write(&json!({ "document": document().to_json(), "gauge-full": 0 }).to_string());
	assert!(matches!(
		scratch.store().read(),
		Err(StoreError::GaugeFull(..))
	));
	assert_eq!(scratch.store().load(), Stored::default());
}

#[test]
fn a_save_is_read_back_and_leaves_no_temporary_behind() {
	let scratch = Scratch::new();
	let store = scratch.store();
	let stored = Stored {
		document: document(),
		gauge_full: Some(0.97),
	};
	store.save(&stored).unwrap();
	assert_eq!(store.load(), stored);
	let on_disk: Json = serde_json::from_slice(&fs::read(scratch.file()).unwrap()).unwrap();
	assert_eq!(
		on_disk,
		json!({ "document": document().to_json(), "gauge-full": 0.97 })
	);

	let entries: Vec<_> = fs::read_dir(scratch.file().parent().unwrap())
		.unwrap()
		.map(|entry| entry.unwrap().file_name())
		.collect();
	assert_eq!(entries, ["battery-curve.json"]);
}

#[test]
fn a_save_replaces_the_one_before_it() {
	let scratch = Scratch::new();
	let store = scratch.store();
	store
		.save(&Stored {
			document: document(),
			gauge_full: Some(0.97),
		})
		.unwrap();
	store.save(&Stored::default()).unwrap();
	assert_eq!(store.read().unwrap(), Some(Stored::default()));
	let on_disk: Json = serde_json::from_slice(&fs::read(scratch.file()).unwrap()).unwrap();
	assert!(on_disk.get("gauge-full").is_none());
}
