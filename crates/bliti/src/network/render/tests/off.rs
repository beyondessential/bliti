//! Candidates turned off (LINK).

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
