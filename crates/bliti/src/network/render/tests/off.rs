//! Candidates and the hotspot turned off (LINK, HOT).

use super::*;

fn off(mut candidate: Json) -> Json {
	candidate["enabled"] = json!(false);
	candidate
}

fn psk(passphrase: &str) -> Json {
	json!({ "kind": "psk", "passphrase": passphrase })
}

/// A wireless candidate turned off is known to no one: iwd holds no network file for it.
#[test]
fn a_wireless_candidate_turned_off_is_not_rendered() {
	let doc = document(json!({ "attachments": [
		wireless("Clinic", psk("a long passphrase")),
		off(wireless("Office", psk("another passphrase")))
	] }));
	let out = rendered(&doc, &hardware(), &select(&doc, &[]));
	file(&out, "iwd/Clinic.psk");
	assert!(
		!paths(&out)
			.iter()
			.any(|path| path.to_string_lossy().contains("Office")),
		"{:?}",
		paths(&out)
	);
}

/// It is still checked as every candidate is.
#[test]
fn a_wireless_candidate_turned_off_is_still_checked() {
	let doc = document(json!({ "attachments": [off(wireless("Clinic", psk("short")))] }));
	assert_eq!(
		invalid_at(&doc, &hardware()),
		"$['attachments'][0]['security']['passphrase']"
	);
}

/// Sharing no file, it may join its SSID differently from one turned on.
#[test]
fn a_wireless_candidate_turned_off_shares_no_file() {
	let doc = document(json!({ "attachments": [
		wireless("Clinic", psk("the new passphrase")),
		off(wireless("Clinic", psk("the old passphrase")))
	] }));
	let out = rendered(&doc, &hardware(), &select(&doc, &[0]));
	assert!(
		file(&out, "iwd/Clinic.psk")
			.contents
			.contains("the new passphrase")
	);
}

/// Nothing turned off is brought up, so a selection holding one is refused.
#[test]
fn a_selection_holding_a_candidate_turned_off_is_refused() {
	let doc = document(json!({ "attachments": [
		{ "kind": "wired-dynamic", "label": "wall", "enabled": false, "verify": true, "interface": "eth0" }
	] }));
	assert!(matches!(
		render(&doc, &hardware(), &select(&doc, &[0])),
		Err(Error::Selection(_))
	));
}

/// A hotspot turned off runs nowhere: no hostapd configuration, no access point network.
#[test]
fn a_hotspot_turned_off_is_not_rendered() {
	let doc = document(json!({
		"attachments": [],
		"hotspot": { "enabled": false, "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let out = rendered(&doc, &hardware(), &select(&doc, &[]));
	assert!(
		!paths(&out).iter().any(|path| {
			let path = path.to_string_lossy();
			path.contains("hostapd") || path.contains("ap0")
		}),
		"{:?}",
		paths(&out)
	);
}

/// It is still checked as a hotspot turned on is.
#[test]
fn a_hotspot_turned_off_is_still_checked() {
	let doc = document(json!({
		"attachments": [],
		"hotspot": { "enabled": false, "ssid": "bliti-setup", "passphrase": "short" }
	}));
	assert_eq!(invalid_at(&doc, &hardware()), "$['hotspot']['passphrase']");
}

/// A selection running a hotspot turned off is refused.
#[test]
fn a_selection_running_a_hotspot_turned_off_is_refused() {
	let doc = document(json!({
		"attachments": [],
		"hotspot": { "enabled": false, "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let selection = Selection {
		hotspot: Some("wlan0".into()),
		..Selection::default()
	};
	assert!(matches!(
		render(&doc, &hardware(), &selection),
		Err(Error::Selection(_))
	));
}
