//! Candidates and the hotspot turned off (LINK, HOT).

use super::*;

fn off(mut candidate: Json) -> Json {
	candidate["enabled"] = json!(false);
	candidate
}

/// A candidate turned off goes nowhere and is reported off, heard or not, with carrier or not.
#[test]
fn a_candidate_turned_off_is_placed_nowhere() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [off(wireless("clinic")), off(dynamic("eth0"))] }),
	);
	assert_eq!(selector.decision().states, [State::Off, State::Off]);

	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -40);
	assert!(selector.decision().links.is_empty());
	assert_eq!(selector.decision().default_route, None);
	assert_eq!(selector.decision().states, [State::Off, State::Off]);
}

/// One turned off takes no radio from a candidate below it, and changes nothing for one above it.
#[test]
fn a_candidate_turned_off_leaves_the_others_as_they_were() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [wireless("clinic"), off(wireless("office")), wireless("fallback")] }),
	);
	hear(&mut selector, "wlan0", "office", -30);
	hear(&mut selector, "wlan0", "fallback", -60);
	assert_eq!(on(&selector, 2), Some("wlan0"));
	establish(&mut selector, 2);
	assert_eq!(state(&selector, 2), &State::DefaultRoute);
	assert_eq!(state(&selector, 1), &State::Off);

	hear(&mut selector, "wlan0", "clinic", -70);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	establish(&mut selector, 0);
	assert_eq!(
		selector.decision().states,
		[State::DefaultRoute, State::Off, State::Standby]
	);
}

/// Turning a candidate off takes it down, and turning it on again brings it up anew.
#[test]
fn a_candidate_turned_on_again_is_tried_anew() {
	let mut selector = selector(one_radio(), json!({ "attachments": [dynamic("eth0")] }));
	carrier(&mut selector, "eth0", true);
	establish(&mut selector, 0);
	let before = attempt(&selector, 0);

	let changes = selector
		.configure(document(json!({ "attachments": [off(dynamic("eth0"))] })))
		.unwrap()
		.changes;
	assert!(changes.contains(&Change::Link {
		interface: "eth0".into(),
		link: None
	}));
	assert!(changes.contains(&Change::DefaultRoute(None)));
	assert_eq!(state(&selector, 0), &State::Off);

	selector
		.configure(document(json!({ "attachments": [dynamic("eth0")] })))
		.unwrap();
	assert_ne!(attempt(&selector, 0), before);
	assert_eq!(
		state(&selector, 0),
		&State::Verifying {
			at: Stage::Addressing
		}
	);
}

/// A wireless candidate turned off leaves a shared-channel radio to a hotspot choosing its own
/// channel, and a one-at-a-time radio to the hotspot (HOT).
#[test]
fn a_wireless_candidate_turned_off_leaves_the_radio_to_the_hotspot() {
	let mut chosen = hotspot(None);
	chosen["channel"] = json!(36);
	let selector = selector(
		hardware(vec![radio("wlan0", Some(Alongside::SharedChannel))]),
		json!({ "attachments": [off(wireless("clinic"))], "hotspot": chosen }),
	);
	assert_eq!(
		selector.decision().hotspot,
		Some(Placement {
			radio: "wlan0".into(),
			channel: HotspotChannel::Own
		})
	);

	let mut selector = self::selector(
		hardware(vec![radio("wlan0", Some(Alongside::OneAtATime))]),
		json!({ "attachments": [off(wireless("clinic"))], "hotspot": hotspot(None) }),
	);
	hear(&mut selector, "wlan0", "clinic", -30);
	assert!(selector.decision().links.is_empty());
	assert_eq!(hotspot_radio(&selector), Some("wlan0"));
}

/// A hotspot turned off is placed on no radio, and leaves a one-at-a-time radio to a client (HOT).
#[test]
fn a_hotspot_turned_off_leaves_the_radio_to_a_client() {
	let mut hotspot = hotspot(None);
	hotspot["enabled"] = json!(false);
	let mut selector = selector(
		hardware(vec![radio("wlan0", Some(Alongside::OneAtATime))]),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot }),
	);
	assert_eq!(hotspot_radio(&selector), None);
	hear(&mut selector, "wlan0", "clinic", -30);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(hotspot_radio(&selector), None);
}
