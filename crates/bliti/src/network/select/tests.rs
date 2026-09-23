use serde_json::{Value as Json, json};

use super::*;
use crate::network::render::Band;

fn document(json: Json) -> Document {
	let Json::Object(map) = json else {
		panic!("a document is an object")
	};
	Document::parse(&map).unwrap()
}

fn radio(station: &str, access_point: Option<Alongside>) -> Radio {
	Radio {
		station: station.into(),
		access_point,
	}
}

fn hardware(radios: Vec<Radio>) -> Hardware {
	Hardware {
		wired: vec!["eth0".into(), "eth1".into()],
		radios,
	}
}

fn one_radio() -> Hardware {
	hardware(vec![radio("wlan0", Some(Alongside::Independent))])
}

fn two_radios() -> Hardware {
	hardware(vec![
		radio("wlan0", Some(Alongside::Independent)),
		radio("wlan1", Some(Alongside::Independent)),
	])
}

fn wireless(ssid: &str) -> Json {
	json!({
		"kind": "wireless", "label": ssid, "ssid": ssid,
		"security": { "kind": "psk", "passphrase": "correct horse" }
	})
}

fn pinned(ssid: &str, interface: &str) -> Json {
	let mut candidate = wireless(ssid);
	candidate["interface"] = json!(interface);
	candidate
}

fn dynamic(interface: &str) -> Json {
	json!({ "kind": "wired-dynamic", "label": interface, "interface": interface })
}

fn fixed(label: &str, address: &str, gateway: &str) -> Json {
	json!({
		"kind": "wired-static", "label": label, "interface": "eth0",
		"addresses": [address], "gateway": gateway
	})
}

fn hotspot(interface: Option<&str>) -> Json {
	let mut hotspot = json!({ "ssid": "bliti", "passphrase": "read me aloud" });
	if let Some(interface) = interface {
		hotspot["interface"] = json!(interface);
	}
	hotspot
}

fn selector(hardware: Hardware, document: Json) -> Selector {
	Selector::new(hardware, self::document(document)).unwrap()
}

fn carrier(selector: &mut Selector, interface: &str, up: bool) -> Vec<Change> {
	selector
		.handle(Event::Carrier {
			interface: interface.into(),
			up,
		})
		.changes
}

fn hear(selector: &mut Selector, interface: &str, ssid: &str, signal: i32) -> Vec<Change> {
	selector
		.handle(Event::InRange {
			interface: interface.into(),
			ssid: ssid.into(),
			signal,
		})
		.changes
}

fn lose(selector: &mut Selector, interface: &str, ssid: &str) -> Vec<Change> {
	selector
		.handle(Event::OutOfRange {
			interface: interface.into(),
			ssid: ssid.into(),
		})
		.changes
}

/// The interface a candidate is brought up on, and the attempt at it.
fn link(selector: &Selector, candidate: usize) -> Option<(&str, Attempt)> {
	selector
		.decision()
		.links
		.iter()
		.find(|(_, link)| link.candidate == candidate)
		.map(|(interface, link)| (interface.as_str(), link.attempt))
}

fn on(selector: &Selector, candidate: usize) -> Option<&str> {
	link(selector, candidate).map(|(interface, _)| interface)
}

fn attempt(selector: &Selector, candidate: usize) -> Attempt {
	link(selector, candidate)
		.unwrap_or_else(|| panic!("candidate {candidate} is not brought up"))
		.1
}

fn pass(selector: &mut Selector, candidate: usize, stage: Stage) -> Vec<Change> {
	let attempt = attempt(selector, candidate);
	selector.handle(Event::Passed { attempt, stage }).changes
}

fn fail(selector: &mut Selector, candidate: usize, stage: Stage, reason: &str) -> Vec<Change> {
	let attempt = attempt(selector, candidate);
	selector
		.handle(Event::Failed {
			attempt,
			stage,
			reason: reason.into(),
		})
		.changes
}

/// Pass every stage a candidate being tried has left.
fn establish(selector: &mut Selector, candidate: usize) -> Vec<Change> {
	let mut changes = Vec::new();
	while let State::Verifying { at } = selector.decision().states[candidate] {
		changes = pass(selector, candidate, at);
	}
	changes
}

fn state(selector: &Selector, candidate: usize) -> &State {
	&selector.decision().states[candidate]
}

fn reached(selector: &Selector, candidate: usize) -> Option<Stage> {
	match state(selector, candidate) {
		State::Unavailable { reached, .. } => Some(*reached),
		_ => None,
	}
}

fn hotspot_radio(selector: &Selector) -> Option<&str> {
	selector
		.decision()
		.hotspot
		.as_ref()
		.map(|placement| placement.radio.as_str())
}

const CHANNEL: Channel = Channel {
	band: Band::Five,
	number: 36,
};

#[test]
fn nothing_observed_leaves_everything_unavailable_at_carrier() {
	let selector = selector(
		one_radio(),
		json!({ "attachments": [dynamic("eth0"), wireless("clinic")] }),
	);
	assert!(selector.decision().links.is_empty());
	assert_eq!(selector.decision().default_route, None);
	assert_eq!(
		selector.decision().states,
		[
			State::Unavailable {
				reached: Stage::Carrier,
				reason: "eth0 has no carrier".into()
			},
			State::Unavailable {
				reached: Stage::Carrier,
				reason: "\"clinic\" is out of range".into()
			},
		]
	);
}

#[test]
fn a_wired_candidate_is_verified_from_addressing_and_a_wireless_one_from_association() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [dynamic("eth0"), wireless("clinic")] }),
	);
	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(
		state(&selector, 0),
		&State::Verifying {
			at: Stage::Addressing
		}
	);
	assert_eq!(
		state(&selector, 1),
		&State::Verifying {
			at: Stage::Association
		}
	);
}

#[test]
fn stages_pass_only_in_order() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [dynamic("eth0"), wireless("clinic")] }),
	);
	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -50);

	assert!(pass(&mut selector, 1, Stage::Addressing).is_empty());
	assert!(pass(&mut selector, 0, Stage::Association).is_empty());
	assert!(pass(&mut selector, 0, Stage::Gateway).is_empty());

	pass(&mut selector, 1, Stage::Association);
	assert_eq!(
		state(&selector, 1),
		&State::Verifying {
			at: Stage::Addressing
		}
	);
}

#[test]
fn a_lease_with_no_route_fails_at_gateway() {
	let mut selector = selector(one_radio(), json!({ "attachments": [dynamic("eth0")] }));
	carrier(&mut selector, "eth0", true);
	pass(&mut selector, 0, Stage::Addressing);
	let changes = fail(&mut selector, 0, Stage::Gateway, "10.0.0.1 did not answer");
	assert_eq!(
		changes,
		[
			Change::Link {
				interface: "eth0".into(),
				link: None
			},
			Change::State {
				candidate: 0,
				state: State::Unavailable {
					reached: Stage::Gateway,
					reason: "10.0.0.1 did not answer".into()
				}
			},
		]
	);
}

#[test]
fn association_without_an_address_fails_at_addressing() {
	let mut selector = selector(one_radio(), json!({ "attachments": [wireless("clinic")] }));
	hear(&mut selector, "wlan0", "clinic", -50);
	pass(&mut selector, 0, Stage::Association);
	fail(&mut selector, 0, Stage::Addressing, "no lease");
	assert_eq!(reached(&selector, 0), Some(Stage::Addressing));
	assert_eq!(on(&selector, 0), None);
}

#[test]
fn two_statics_on_one_port_are_told_apart_at_two_sites() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [
			fixed("ward", "192.168.60.10/24", "192.168.60.1"),
			fixed("office", "10.1.0.10/24", "10.1.0.1"),
		] }),
	);

	// At the office, the ward's gateway does not answer, so the office's is tried next.
	carrier(&mut selector, "eth0", true);
	assert_eq!(on(&selector, 0), Some("eth0"));
	assert_eq!(state(&selector, 1), &State::Standby);
	fail(
		&mut selector,
		0,
		Stage::Gateway,
		"192.168.60.1 did not answer",
	);
	assert_eq!(on(&selector, 1), Some("eth0"));
	establish(&mut selector, 1);
	assert_eq!(selector.decision().default_route, Some(1));
	assert_eq!(reached(&selector, 0), Some(Stage::Gateway));

	// Carried to the ward and plugged in, the ward's is tried first again, and holds.
	carrier(&mut selector, "eth0", false);
	assert_eq!(reached(&selector, 0), Some(Stage::Carrier));
	assert_eq!(reached(&selector, 1), Some(Stage::Carrier));
	carrier(&mut selector, "eth0", true);
	assert_eq!(on(&selector, 0), Some("eth0"));
	establish(&mut selector, 0);
	assert_eq!(selector.decision().default_route, Some(0));
	assert_eq!(state(&selector, 1), &State::Standby);
}

#[test]
fn a_site_changing_around_a_cable_that_never_moved_selects_again() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [
			fixed("ward", "192.168.60.10/24", "192.168.60.1"),
			fixed("office", "10.1.0.10/24", "10.1.0.1"),
		] }),
	);
	carrier(&mut selector, "eth0", true);
	establish(&mut selector, 0);
	assert_eq!(state(&selector, 0), &State::DefaultRoute);

	let changes = fail(
		&mut selector,
		0,
		Stage::Gateway,
		"192.168.60.1 stopped answering",
	);
	assert!(changes.contains(&Change::DefaultRoute(None)));
	assert_eq!(on(&selector, 1), Some("eth0"));
	establish(&mut selector, 1);
	assert_eq!(selector.decision().default_route, Some(1));
}

#[test]
fn a_wired_and_a_wireless_candidate_are_up_at_once_with_the_route_following_the_order() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [wireless("clinic"), dynamic("eth0")] }),
	);
	carrier(&mut selector, "eth0", true);
	establish(&mut selector, 1);
	assert_eq!(selector.decision().default_route, Some(1));

	hear(&mut selector, "wlan0", "clinic", -60);
	establish(&mut selector, 0);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(on(&selector, 1), Some("eth0"));
	assert_eq!(selector.decision().default_route, Some(0));
	assert_eq!(selector.decision().states, [State::DefaultRoute, State::Up]);
}

#[test]
fn the_default_route_moves_back_when_a_higher_candidate_returns() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [dynamic("eth0"), wireless("clinic")] }),
	);
	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -60);
	establish(&mut selector, 0);
	establish(&mut selector, 1);
	assert_eq!(selector.decision().default_route, Some(0));

	let changes = carrier(&mut selector, "eth0", false);
	assert!(changes.contains(&Change::DefaultRoute(Some(1))));

	carrier(&mut selector, "eth0", true);
	assert_eq!(selector.decision().default_route, Some(1));
	let changes = establish(&mut selector, 0);
	assert!(changes.contains(&Change::DefaultRoute(Some(0))));
	assert_eq!(selector.decision().states, [State::DefaultRoute, State::Up]);
}

#[test]
fn a_higher_network_coming_into_range_takes_the_radio_from_a_lower_one() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [wireless("clinic"), wireless("fallback")] }),
	);
	hear(&mut selector, "wlan0", "fallback", -50);
	establish(&mut selector, 1);
	assert_eq!(state(&selector, 1), &State::DefaultRoute);

	hear(&mut selector, "wlan0", "clinic", -70);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(state(&selector, 1), &State::Standby);

	// The higher one failing hands the radio back.
	fail(&mut selector, 0, Stage::Association, "the key was refused");
	assert_eq!(on(&selector, 1), Some("wlan0"));
}

#[test]
fn a_failed_candidate_is_tried_again_once_available_anew_or_on_retry() {
	let mut selector = selector(one_radio(), json!({ "attachments": [wireless("clinic")] }));
	hear(&mut selector, "wlan0", "clinic", -50);
	fail(&mut selector, 0, Stage::Association, "the key was refused");

	// Heard at a new signal is not available anew.
	hear(&mut selector, "wlan0", "clinic", -45);
	assert_eq!(reached(&selector, 0), Some(Stage::Association));

	lose(&mut selector, "wlan0", "clinic");
	assert_eq!(reached(&selector, 0), Some(Stage::Carrier));
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(on(&selector, 0), Some("wlan0"));

	fail(&mut selector, 0, Stage::Association, "the key was refused");
	selector.handle(Event::Retry { candidate: 0 });
	assert_eq!(on(&selector, 0), Some("wlan0"));
}

#[test]
fn a_report_from_a_superseded_attempt_is_ignored() {
	let mut selector = selector(one_radio(), json!({ "attachments": [dynamic("eth0")] }));
	carrier(&mut selector, "eth0", true);
	let stale = attempt(&selector, 0);
	carrier(&mut selector, "eth0", false);
	carrier(&mut selector, "eth0", true);
	assert_ne!(attempt(&selector, 0), stale);

	let changes = selector
		.handle(Event::Failed {
			attempt: stale,
			stage: Stage::Gateway,
			reason: "late".into(),
		})
		.changes;
	assert!(changes.is_empty());
	assert_eq!(
		state(&selector, 0),
		&State::Verifying {
			at: Stage::Addressing
		}
	);
}

#[test]
fn a_hidden_network_is_tried_without_being_heard() {
	let mut candidate = wireless("back office");
	candidate["hidden"] = json!(true);
	let selector = selector(one_radio(), json!({ "attachments": [candidate] }));
	assert_eq!(on(&selector, 0), Some("wlan0"));
}

#[test]
fn a_pinned_candidate_stays_on_its_radio() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [pinned("clinic", "wlan1")] }),
	);
	hear(&mut selector, "wlan0", "clinic", -30);
	assert_eq!(
		state(&selector, 0),
		&State::Unavailable {
			reached: Stage::Carrier,
			reason: "\"clinic\" is out of range of wlan1".into()
		}
	);

	hear(&mut selector, "wlan1", "clinic", -80);
	assert_eq!(on(&selector, 0), Some("wlan1"));
}

#[test]
fn an_unpinned_candidate_takes_the_radio_hearing_it_best_and_stays_there() {
	let mut selector = selector(two_radios(), json!({ "attachments": [wireless("clinic")] }));
	hear(&mut selector, "wlan0", "clinic", -70);
	assert_eq!(on(&selector, 0), Some("wlan0"));

	// A better signal elsewhere does not move a candidate already brought up.
	let changes = hear(&mut selector, "wlan1", "clinic", -40);
	assert!(changes.is_empty());
	assert_eq!(on(&selector, 0), Some("wlan0"));

	// Tried afresh, it takes the radio hearing it best.
	fail(
		&mut selector,
		0,
		Stage::Association,
		"the access point went away",
	);
	let update = selector.handle(Event::Retry { candidate: 0 });
	assert_eq!(update.decision.links["wlan1"].candidate, 0);
	assert_eq!(on(&selector, 0), Some("wlan1"));
}

#[test]
fn an_unpinned_candidate_takes_a_radio_carrying_nothing_above_it() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [pinned("uplink", "wlan0"), wireless("clinic")] }),
	);
	hear(&mut selector, "wlan0", "uplink", -60);
	hear(&mut selector, "wlan0", "clinic", -30);
	assert_eq!(state(&selector, 1), &State::Standby);

	hear(&mut selector, "wlan1", "clinic", -80);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(on(&selector, 1), Some("wlan1"));
}

#[test]
fn a_lower_candidate_gives_way_to_a_higher_one_wanting_its_radio() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [wireless("clinic"), pinned("uplink", "wlan0")] }),
	);
	hear(&mut selector, "wlan0", "uplink", -60);
	assert_eq!(on(&selector, 1), Some("wlan0"));

	hear(&mut selector, "wlan0", "clinic", -30);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(state(&selector, 1), &State::Standby);
}

#[test]
fn the_hotspot_prefers_a_radio_carrying_no_wireless_candidate() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) }),
	);
	assert_eq!(hotspot_radio(&selector), Some("wlan0"));

	let changes = hear(&mut selector, "wlan0", "clinic", -50);
	assert!(changes.iter().any(|change| matches!(
		change,
		Change::Hotspot(Some(Placement { radio, .. })) if radio == "wlan1"
	)));
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(hotspot_radio(&selector), Some("wlan1"));
}

#[test]
fn a_pinned_hotspot_runs_on_its_radio_beside_a_client() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(Some("wlan0")) }),
	);
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(
		selector.decision().hotspot,
		Some(Placement {
			radio: "wlan0".into(),
			channel: HotspotChannel::Own
		})
	);
}

#[test]
fn a_hotspot_on_a_shared_channel_radio_follows_its_client() {
	let mut selector = selector(
		hardware(vec![radio("wlan0", Some(Alongside::SharedChannel))]),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) }),
	);
	assert_eq!(
		selector.decision().hotspot.as_ref().unwrap().channel,
		HotspotChannel::Own
	);

	hear(&mut selector, "wlan0", "clinic", -50);
	let follows = |channel| {
		Some(Placement {
			radio: "wlan0".into(),
			channel: HotspotChannel::Follows {
				candidate: 0,
				channel,
			},
		})
	};
	assert_eq!(selector.decision().hotspot, follows(None));

	pass(&mut selector, 0, Stage::Association);
	let changes = selector
		.handle(Event::StationChannel {
			interface: "wlan0".into(),
			channel: Some(CHANNEL),
		})
		.changes;
	assert_eq!(changes, [Change::Hotspot(follows(Some(CHANNEL)))]);
	assert_eq!(selector.selection().unwrap().station_channel, Some(CHANNEL));

	// The client leaving takes the channel with it.
	lose(&mut selector, "wlan0", "clinic");
	assert_eq!(
		selector.decision().hotspot.as_ref().unwrap().channel,
		HotspotChannel::Own
	);
	assert_eq!(selector.selection().unwrap().station_channel, None);
}

#[test]
fn a_hotspot_and_a_client_only_one_at_a_time_radio_could_carry_are_refused() {
	let hardware = || hardware(vec![radio("wlan0", Some(Alongside::OneAtATime))]);
	let refused = |document: Json| {
		Selector::new(hardware(), self::document(document))
			.unwrap_err()
			.at
	};
	assert_eq!(
		refused(json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) })),
		"$['hotspot']"
	);
	assert_eq!(
		refused(json!({
			"attachments": [pinned("clinic", "wlan0")],
			"hotspot": hotspot(Some("wlan0"))
		})),
		"$['hotspot']['interface']"
	);

	// Wired alone beside the hotspot is fine, and so is a client with a second radio to go on.
	assert!(
		Selector::new(
			hardware(),
			document(json!({ "attachments": [dynamic("eth0")], "hotspot": hotspot(None) }))
		)
		.is_ok()
	);
	assert!(
		Selector::new(
			self::hardware(vec![
				radio("wlan0", Some(Alongside::OneAtATime)),
				radio("wlan1", None),
			]),
			document(json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) }))
		)
		.is_ok()
	);
}

#[test]
fn a_client_never_takes_the_one_at_a_time_radio_the_hotspot_needs() {
	let mut selector = selector(
		hardware(vec![
			radio("wlan0", Some(Alongside::OneAtATime)),
			radio("wlan1", None),
		]),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) }),
	);
	hear(&mut selector, "wlan0", "clinic", -30);
	assert_eq!(state(&selector, 0), &State::Standby);
	assert_eq!(hotspot_radio(&selector), Some("wlan0"));

	hear(&mut selector, "wlan1", "clinic", -80);
	assert_eq!(on(&selector, 0), Some("wlan1"));
	assert_eq!(hotspot_radio(&selector), Some("wlan0"));
}

#[test]
fn two_one_at_a_time_radios_split_between_a_client_and_the_hotspot() {
	let mut selector = selector(
		hardware(vec![
			radio("wlan0", Some(Alongside::OneAtATime)),
			radio("wlan1", Some(Alongside::OneAtATime)),
		]),
		json!({
			"attachments": [wireless("clinic"), wireless("fallback")],
			"hotspot": hotspot(None)
		}),
	);
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(on(&selector, 0), Some("wlan0"));
	assert_eq!(hotspot_radio(&selector), Some("wlan1"));

	// The second radio is the hotspot's last, so a second client cannot have it.
	hear(&mut selector, "wlan1", "fallback", -50);
	assert_eq!(state(&selector, 1), &State::Standby);
	assert_eq!(hotspot_radio(&selector), Some("wlan1"));
}

#[test]
fn the_hotspot_stays_put_while_it_has_no_better_radio() {
	let mut selector = selector(
		two_radios(),
		json!({ "attachments": [wireless("clinic")], "hotspot": hotspot(None) }),
	);
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(hotspot_radio(&selector), Some("wlan1"));
	lose(&mut selector, "wlan0", "clinic");
	assert_eq!(hotspot_radio(&selector), Some("wlan1"));
}

#[test]
fn check_refuses_interfaces_the_device_does_not_have() {
	let refused = |hardware: Hardware, document: Json| {
		check(&self::document(document), &hardware).unwrap_err().at
	};
	assert_eq!(
		refused(one_radio(), json!({ "attachments": [dynamic("eth9")] })),
		"$['attachments'][0]['interface']"
	);
	assert_eq!(
		refused(
			one_radio(),
			json!({ "attachments": [pinned("clinic", "wlan9")] })
		),
		"$['attachments'][0]['interface']"
	);
	assert_eq!(
		refused(
			hardware(vec![]),
			json!({ "attachments": [wireless("clinic")] })
		),
		"$['attachments'][0]['kind']"
	);
	assert_eq!(
		refused(
			hardware(vec![radio("wlan0", None)]),
			json!({ "attachments": [], "hotspot": hotspot(Some("wlan0")) })
		),
		"$['hotspot']['interface']"
	);
	assert_eq!(
		refused(
			hardware(vec![radio("wlan0", None)]),
			json!({ "attachments": [], "hotspot": hotspot(None) })
		),
		"$['hotspot']"
	);
}

#[test]
fn configuring_the_same_document_changes_nothing() {
	let json = json!({ "attachments": [dynamic("eth0")] });
	let mut selector = selector(one_radio(), json.clone());
	carrier(&mut selector, "eth0", true);
	let changes = selector.configure(document(json)).unwrap().changes;
	assert!(changes.is_empty());
}

#[test]
fn a_candidate_carried_into_a_new_document_keeps_its_attempt() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [dynamic("eth0"), wireless("clinic")] }),
	);
	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -50);
	establish(&mut selector, 0);
	fail(&mut selector, 1, Stage::Association, "the key was refused");
	let kept = attempt(&selector, 0);

	let changes = selector
		.configure(document(json!({ "attachments": [
			wireless("clinic"),
			wireless("office"),
			dynamic("eth0"),
		] })))
		.unwrap()
		.changes;
	assert_eq!(attempt(&selector, 2), kept);
	assert_eq!(state(&selector, 2), &State::DefaultRoute);
	assert_eq!(reached(&selector, 0), Some(Stage::Association));
	assert_eq!(
		changes
			.iter()
			.filter(|change| matches!(change, Change::State { .. }))
			.count(),
		3
	);
}

#[test]
fn a_single_radio_decision_is_a_renderer_selection() {
	let mut selector = selector(
		one_radio(),
		json!({ "attachments": [wireless("clinic"), dynamic("eth1"), dynamic("eth0")] }),
	);
	carrier(&mut selector, "eth0", true);
	hear(&mut selector, "wlan0", "clinic", -50);
	assert_eq!(
		selector.selection(),
		Some(Selection {
			active: vec![0, 2],
			station_channel: None
		})
	);

	let several = self::selector(two_radios(), json!({ "attachments": [] }));
	assert_eq!(several.selection(), None);
}

#[test]
fn stages_carry_the_wire_strings_of_link() {
	let names: Vec<&str> = [
		Stage::Carrier,
		Stage::Association,
		Stage::Addressing,
		Stage::Gateway,
	]
	.into_iter()
	.map(Stage::as_str)
	.collect();
	assert_eq!(names, ["carrier", "association", "addressing", "gateway"]);
}

#[test]
fn a_failure_names_the_stage_it_stopped_at() {
	let at = bliti_core::channel::config::path(&[
		bliti_core::channel::config::Segment::Name("attachments"),
		bliti_core::channel::config::Segment::Index(0),
	]);
	let invalid = Stage::Gateway.failed(at.clone(), "the gateway did not answer");
	assert_eq!(invalid.at, at);
	assert_eq!(invalid.reached.as_deref(), Some("gateway"));
}
