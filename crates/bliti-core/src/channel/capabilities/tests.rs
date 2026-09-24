use serde_json::json;

use super::*;

fn object(value: Json) -> Map<String, Json> {
	let Json::Object(map) = value else {
		panic!("an object")
	};
	map
}

/// The Pi with a USB adapter, as NET shapes it: kinds, radios and bands keyed by their values.
fn pi_with_adapter() -> Map<String, Json> {
	object(json!({
		"attachments": {
			"kind": {
				"wireless": {
					"interface": {
						"wlan0": {
							"security": { "kind": { "psk": {}, "sae": {}, "psk-sae": {} } },
							"hidden": true
						},
						"wlx00c0caa1b2c3": {
							"security": { "kind": {
								"psk": {}, "sae": {}, "psk-sae": {},
								"enterprise": { "eap": ["peap", "ttls", "tls", "pwd"] }
							} },
							"hidden": true
						}
					},
					"nameservers": true
				},
				"wired-dynamic": { "interface": ["eth0"], "nameservers": true },
				"wired-static": { "interface": ["eth0"], "nameservers": true }
			}
		},
		"hotspot": {
			"interface": {
				"wlan0": {},
				"wlx00c0caa1b2c3": {
					"band": {
						"2ghz": { "channel": [1, 6, 11], "channel-width": [20] },
						"5ghz": { "channel": [36, 40, 44, 48], "channel-width": [20, 40, 80] }
					}
				}
			},
			"share-upstream": true,
			"isolate-clients": true,
			"dhcp-range": true
		},
		"regulatory-domain": true
	}))
}

fn check_on_pi(document: Json) -> Result<(), Invalid> {
	check(&object(document), &pi_with_adapter())
}

fn wireless(extra: Json) -> Json {
	let mut candidate = object(json!({
		"kind": "wireless", "label": "Clinic", "enabled": true, "verify": true, "ssid": "Clinic",
		"security": { "kind": "sae", "passphrase": "a good long passphrase" }
	}));
	candidate.extend(object(extra));
	Json::Object(candidate)
}

#[test]
fn a_document_within_capabilities_passes() {
	check_on_pi(json!({
		"attachments": [
			wireless(json!({ "interface": "wlan0", "hidden": true, "nameservers": ["1.1.1.1"] })),
			{ "kind": "wired-static", "label": "Office", "enabled": true, "verify": true, "interface": "eth0",
			  "addresses": ["192.168.60.20/24"], "gateway": "192.168.60.1" },
			{ "kind": "wired-dynamic", "label": "Any port", "enabled": true, "verify": true, "interface": "eth0" }
		],
		"hotspot": { "ssid": "setup", "passphrase": "read this aloud", "interface": "wlx00c0caa1b2c3",
					 "band": "5ghz", "channel": 36, "channel-width": 80, "isolate-clients": false },
		"regulatory-domain": "VU"
	}))
	.unwrap();
}

/// An empty document asks for nothing.
#[test]
fn nothing_asked_passes() {
	check_on_pi(json!({ "attachments": [] })).unwrap();
}

/// Required members pass without being listed: a kind supports what the document requires of it,
/// `enabled` and `verify` either way on every kind.
#[test]
fn required_members_are_implied_by_their_kind() {
	check_on_pi(json!({ "attachments": [wireless(json!({}))] })).unwrap();
	for flag in [true, false] {
		check_on_pi(json!({ "attachments": [
			wireless(json!({ "enabled": flag, "verify": flag })),
			{ "kind": "wired-dynamic", "label": "p", "enabled": flag, "verify": flag, "interface": "eth0" },
			{ "kind": "wired-static", "label": "s", "enabled": flag, "verify": flag, "interface": "eth0",
			  "addresses": ["192.168.60.20/24"], "gateway": "192.168.60.1" }
		] }))
		.unwrap();
	}
}

/// A member capabilities do not carry is not supported.
#[test]
fn an_unlisted_optional_member_is_refused() {
	let caps = object(json!({ "attachments": { "kind": {
		"wired-dynamic": { "interface": ["eth0"] }
	} } }));
	let err = check(
		&object(json!({ "attachments": [
			{ "kind": "wired-dynamic", "label": "p", "enabled": true, "verify": true, "interface": "eth0", "nameservers": ["1.1.1.1"] }
		] })),
		&caps,
	)
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['nameservers']");
}

/// A device that cannot hold a connection to chosen bands offers no `bands`, so any asked for are
/// refused at the member (WLAN).
#[test]
fn bands_the_device_does_not_offer_are_refused() {
	let err = check_on_pi(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0", "bands": ["5ghz"] }))] }),
	)
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['bands']");
}

/// Where a device offers some bands, a candidate may name any of them and none other.
#[test]
fn bands_are_held_to_those_offered() {
	let mut caps = Json::Object(pi_with_adapter());
	caps["attachments"]["kind"]["wireless"]["interface"]["wlan0"]["bands"] =
		json!(["2ghz", "5ghz"]);
	let caps = object(caps);
	let within = object(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0", "bands": ["5ghz"] }))] }),
	);
	check(&within, &caps).unwrap();
	let beyond = object(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0", "bands": ["5ghz", "6ghz"] }))] }),
	);
	assert_eq!(
		check(&beyond, &caps).unwrap_err().at,
		"$['attachments'][0]['bands'][1]"
	);
}

/// A kind absent from capabilities cannot be proposed, and the fault names the kind.
#[test]
fn an_unsupported_kind_is_refused() {
	let caps = object(
		json!({ "attachments": { "kind": { "wired-dynamic": { "interface": ["eth0"] } } } }),
	);
	let err = check(
		&object(json!({ "attachments": [wireless(json!({}))] })),
		&caps,
	)
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['kind']");
}

/// A value outside the listed ones is refused at that value.
#[test]
fn a_value_outside_an_array_is_refused() {
	let err = check_on_pi(json!({ "attachments": [
		{ "kind": "wired-dynamic", "label": "p", "enabled": true, "verify": true, "interface": "eth1" }
	] }))
	.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['interface']");
}

/// What a radio supports decides what a candidate pinned to it may carry.
#[test]
fn a_pinned_radio_holds_its_own_constraints() {
	let enterprise = json!({ "security": { "kind": "enterprise", "eap": "tls", "identity": "iti",
		"ca-certificate": "…", "domain": "radius.example", "client-certificate": "…", "client-key": "…" } });
	let mut on_adapter = object(wireless(json!({ "interface": "wlx00c0caa1b2c3" })));
	on_adapter.extend(object(enterprise.clone()));
	check_on_pi(json!({ "attachments": [on_adapter] })).unwrap();

	let mut on_builtin = object(wireless(json!({ "interface": "wlan0" })));
	on_builtin.extend(object(enterprise));
	let err = check_on_pi(json!({ "attachments": [on_builtin] })).unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['security']['kind']");
}

/// Unpinned, a candidate is within capabilities where any radio admits it (NET).
#[test]
fn an_unpinned_candidate_passes_where_any_radio_admits_it() {
	let mut candidate = object(wireless(json!({})));
	candidate.insert(
		"security".to_owned(),
		json!({ "kind": "enterprise", "eap": "pwd", "identity": "iti", "password": "p" }),
	);
	check_on_pi(json!({ "attachments": [candidate] })).unwrap();

	let mut nowhere = object(wireless(json!({})));
	nowhere.insert(
		"security".to_owned(),
		json!({ "kind": "enterprise", "eap": "fast", "identity": "iti" }),
	);
	let err = check_on_pi(json!({ "attachments": [nowhere] })).unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['security']['eap']");
}

/// A radio not in capabilities is refused where a candidate names it.
#[test]
fn an_unknown_radio_is_refused() {
	let err = check_on_pi(json!({ "attachments": [wireless(json!({ "interface": "wlan9" }))] }))
		.unwrap_err();
	assert_eq!(err.at, "$['attachments'][0]['interface']");
}

/// A radio offering the hotspot no band refuses one.
#[test]
fn a_band_is_refused_on_a_radio_that_does_not_offer_one() {
	let err = check_on_pi(json!({ "attachments": [],
		"hotspot": { "ssid": "s", "passphrase": "p", "interface": "wlan0", "band": "5ghz" } }))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['band']");
}

/// Channels are keyed by band, and a channel off the band's list is refused at the channel.
#[test]
fn a_channel_is_checked_against_its_band() {
	let err = check_on_pi(json!({ "attachments": [],
		"hotspot": { "ssid": "s", "passphrase": "p", "interface": "wlx00c0caa1b2c3",
		             "band": "2ghz", "channel": 36 } }))
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']['channel']");
}

/// A channel with neither radio nor band set passes where some radio and band carry it.
#[test]
fn a_channel_with_nothing_else_set_passes_where_any_carries_it() {
	check_on_pi(json!({ "attachments": [],
		"hotspot": { "ssid": "s", "passphrase": "p", "channel": 44 } }))
	.unwrap();
	let err = check_on_pi(json!({ "attachments": [],
		"hotspot": { "ssid": "s", "passphrase": "p", "channel": 165 } }))
	.unwrap_err();
	assert!(err.at.starts_with("$['hotspot']"), "{}", err.at);
}

/// A device offering no hotspot refuses one.
#[test]
fn a_hotspot_is_refused_where_none_is_offered() {
	let caps = object(json!({ "attachments": { "kind": {} } }));
	let err = check(
		&object(json!({ "attachments": [], "hotspot": { "ssid": "s", "passphrase": "p" } })),
		&caps,
	)
	.unwrap_err();
	assert_eq!(err.at, "$['hotspot']");
}

/// A member a newer client added, which capabilities cannot know of, is refused rather than
/// silently carried: the device would not run what was written.
#[test]
fn an_unknown_member_is_refused() {
	let err = check_on_pi(json!({ "attachments": [], "x-proxy": "on" })).unwrap_err();
	assert_eq!(err.at, "$['x-proxy']");
}

/// Whole capabilities with `radios` as given, the hotspot keyed by every radio able to run one.
fn with_radios(radios: Json) -> Map<String, Json> {
	let mut mirror = Json::Object(pi_with_adapter());
	let keys: Map<String, Json> = radios
		.as_object()
		.unwrap()
		.iter()
		.filter(|(_, radio)| radio.get("alongside").is_some())
		.map(|(name, _)| (name.clone(), json!({})))
		.collect();
	mirror["hotspot"]["interface"] = Json::Object(keys);
	let mut wireless = Map::new();
	for name in radios.as_object().unwrap().keys() {
		wireless.insert(
			name.clone(),
			json!({ "security": { "kind": { "psk": {}, "sae": {}, "psk-sae": {} } }, "hidden": true }),
		);
	}
	mirror["attachments"]["kind"]["wireless"]["interface"] = Json::Object(wireless);
	object(json!({ "document": mirror, "radios": radios, "acts": {} }))
}

fn one_at_a_time() -> Map<String, Json> {
	with_radios(
		json!({ "wlan0": { "model": "m", "bands": ["2ghz"], "alongside": "one-at-a-time" } }),
	)
}

fn admitted(document: Json, capabilities: &Map<String, Json>) -> Result<(), Refused> {
	admits(&object(document), capabilities)
}

fn hotspot() -> Json {
	json!({ "ssid": "s", "passphrase": "p" })
}

/// A hotspot and a wireless candidate only a radio running one at a time could carry are refused at
/// the hotspot, or at its `interface` where it names one (HOT).
#[test]
fn a_hotspot_beside_a_client_on_a_one_at_a_time_radio_is_refused() {
	let refused = admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot() }),
		&one_at_a_time(),
	)
	.unwrap_err();
	assert_eq!(refused.rule, Rule::OneAtATime);
	assert_eq!(refused.invalid.at, "$['hotspot']");
	assert!(
		refused.invalid.reason.contains("\"Clinic\""),
		"{}",
		refused.invalid.reason
	);

	let mut pinned = object(hotspot());
	pinned.insert("interface".into(), "wlan0".into());
	let refused = admitted(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0" }))], "hotspot": pinned }),
		&one_at_a_time(),
	)
	.unwrap_err();
	assert_eq!(refused.invalid.at, "$['hotspot']['interface']");
}

/// A wireless candidate turned off could be carried by no radio, so it leaves a one-at-a-time radio
/// to the hotspot (HOT).
#[test]
fn a_hotspot_beside_a_client_turned_off_passes_on_a_one_at_a_time_radio() {
	admitted(
		json!({ "attachments": [wireless(json!({ "enabled": false }))], "hotspot": hotspot() }),
		&one_at_a_time(),
	)
	.unwrap();
}

/// Wired candidates never take the radio, so they sit beside the hotspot on any radio.
#[test]
fn a_hotspot_beside_wired_candidates_passes_on_a_one_at_a_time_radio() {
	admitted(
		json!({ "attachments": [
			{ "kind": "wired-dynamic", "label": "p", "enabled": true, "verify": true, "interface": "eth0" }
		], "hotspot": hotspot() }),
		&one_at_a_time(),
	)
	.unwrap();
}

/// A second radio the candidate or the hotspot could go on keeps them apart.
#[test]
fn a_second_radio_keeps_the_hotspot_and_a_client_apart() {
	let caps = with_radios(json!({
		"wlan0": { "model": "m", "bands": ["2ghz"], "alongside": "one-at-a-time" },
		"wlan1": { "model": "m", "bands": ["2ghz"] }
	}));
	admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot() }),
		&caps,
	)
	.unwrap();
	// Pinned to the radio the hotspot needs, the candidate has nowhere else to go.
	let refused = admitted(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0" }))], "hotspot": hotspot() }),
		&caps,
	)
	.unwrap_err();
	assert_eq!(refused.invalid.at, "$['hotspot']");

	let both = with_radios(json!({
		"wlan0": { "model": "m", "bands": ["2ghz"], "alongside": "one-at-a-time" },
		"wlan1": { "model": "m", "bands": ["2ghz"], "alongside": "one-at-a-time" }
	}));
	admitted(
		json!({ "attachments": [wireless(json!({ "interface": "wlan0" }))], "hotspot": hotspot() }),
		&both,
	)
	.unwrap();
}

/// The mirror is checked first, and its fault is told apart from a rule's.
#[test]
fn the_mirror_is_checked_before_the_radios() {
	let refused = admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot(), "x-proxy": "on" }),
		&one_at_a_time(),
	)
	.unwrap_err();
	assert_eq!(refused.rule, Rule::Mirror);
	assert_eq!(refused.invalid.at, "$['x-proxy']");
}

/// The Pi with a USB adapter where the built-in shared-channel radio offers 2.4 GHz channels for the
/// hotspot and the adapter 5 GHz ones.
fn shared_and_independent() -> Map<String, Json> {
	let mut caps = with_radios(json!({
		"wlan0": { "model": "m", "bands": ["2ghz", "5ghz"], "alongside": "shared-channel" },
		"wlx00c0caa1b2c3": { "model": "m", "bands": ["2ghz", "5ghz"], "alongside": "independent" }
	}));
	caps["document"]["hotspot"]["interface"] = json!({
		"wlan0": { "band": { "2ghz": { "channel": [1, 6, 11], "channel-width": [20] } } },
		"wlx00c0caa1b2c3": { "band": { "5ghz": { "channel": [36, 40], "channel-width": [20, 40] } } }
	});
	caps
}

/// The Pi alone: one shared-channel radio offering 2.4 GHz channels for the hotspot.
fn shared_only() -> Map<String, Json> {
	let mut caps = with_radios(json!({
		"wlan0": { "model": "m", "bands": ["2ghz", "5ghz"], "alongside": "shared-channel" }
	}));
	caps["document"]["hotspot"]["interface"] = json!({
		"wlan0": { "band": { "2ghz": { "channel": [1, 6, 11], "channel-width": [20] } } }
	});
	caps
}

fn hotspot_with(extra: Json) -> Json {
	let mut out = object(hotspot());
	out.extend(object(extra));
	Json::Object(out)
}

/// With no wireless candidate to follow, a hotspot on a shared-channel radio chooses its own channel
/// (HOT).
#[test]
fn a_shared_channel_hotspot_with_no_client_to_follow_chooses_its_channel() {
	let wired = json!({ "kind": "wired-dynamic", "label": "p", "enabled": true, "verify": true, "interface": "eth0" });
	for attachments in [json!([]), json!([wired])] {
		admitted(
			json!({ "attachments": attachments,
				"hotspot": hotspot_with(json!({ "band": "2ghz", "channel": 6, "channel-width": 20 })) }),
			&shared_only(),
		)
		.unwrap();
	}
}

/// A wireless candidate the radio could carry takes the choice away, and the fault is at the first
/// of band, channel and width the hotspot sets (HOT).
#[test]
fn a_shared_channel_hotspot_beside_a_client_cannot_choose_its_channel() {
	let refused = |extra: Json| {
		admitted(
			json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot_with(extra) }),
			&shared_only(),
		)
		.unwrap_err()
	};
	let both = refused(json!({ "band": "2ghz", "channel": 6 }));
	assert_eq!(both.rule, Rule::SharedChannel);
	assert_eq!(both.invalid.at, "$['hotspot']['band']");
	assert_eq!(
		refused(json!({ "channel": 6 })).invalid.at,
		"$['hotspot']['channel']"
	);
	assert_eq!(
		refused(json!({ "channel-width": 20 })).invalid.at,
		"$['hotspot']['channel-width']"
	);

	// Setting none of them, it follows the client and is fine.
	admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot() }),
		&shared_only(),
	)
	.unwrap();
}

/// A wireless candidate turned off leaves the shared-channel radio's hotspot its choice (HOT).
#[test]
fn a_client_turned_off_leaves_a_shared_channel_hotspot_its_channel() {
	let document = json!({ "attachments": [wireless(json!({ "enabled": false }))],
		"hotspot": hotspot_with(json!({ "band": "2ghz", "channel": 6, "channel-width": 20 })) });
	admitted(document.clone(), &shared_only()).unwrap();
	assert!(
		channel_choice(&object(document), &shared_only())
			.follows
			.is_empty()
	);
}

/// A client pinned to another radio leaves the shared-channel radio's hotspot its choice, and an
/// unpinned one does not.
#[test]
fn only_a_client_the_shared_channel_radio_could_carry_takes_the_choice_away() {
	let caps = shared_and_independent();
	let on_builtin = hotspot_with(json!({ "interface": "wlan0", "channel": 6 }));
	admitted(
		json!({ "attachments": [wireless(json!({ "interface": "wlx00c0caa1b2c3" }))],
			"hotspot": on_builtin.clone() }),
		&caps,
	)
	.unwrap();
	let refused = admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": on_builtin }),
		&caps,
	)
	.unwrap_err();
	assert_eq!(refused.rule, Rule::SharedChannel);
	assert_eq!(refused.invalid.at, "$['hotspot']['channel']");
}

/// Unpinned, the hotspot's settings must suit a radio where it chooses its own channel, not merely
/// the shared-channel radio a client could take.
#[test]
fn an_unpinned_hotspot_chooses_its_channel_where_a_radio_lets_it() {
	let caps = shared_and_independent();
	admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot_with(json!({ "channel": 36 })) }),
		&caps,
	)
	.unwrap();
	let refused = admitted(
		json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot_with(json!({ "channel": 6 })) }),
		&caps,
	)
	.unwrap_err();
	assert_eq!(refused.rule, Rule::SharedChannel);
	assert_eq!(refused.invalid.at, "$['hotspot']['channel']");
}

/// Where the hotspot chooses its own channel and where it follows a client, as a client reads it to
/// decide what to offer.
#[test]
fn the_channel_choice_splits_the_hotspot_radios() {
	let caps = shared_and_independent();
	let choice = |document: Json| channel_choice(&object(document), &caps);
	assert_eq!(
		choice(json!({ "attachments": [] })),
		ChannelChoice {
			own: vec!["wlan0".into(), "wlx00c0caa1b2c3".into()],
			follows: vec![],
		}
	);
	assert_eq!(
		choice(json!({ "attachments": [wireless(json!({}))], "hotspot": hotspot() })),
		ChannelChoice {
			own: vec!["wlx00c0caa1b2c3".into()],
			follows: vec!["wlan0".into()],
		}
	);
	assert_eq!(
		choice(json!({ "attachments": [wireless(json!({}))],
			"hotspot": hotspot_with(json!({ "interface": "wlan0" })) })),
		ChannelChoice {
			own: vec![],
			follows: vec!["wlan0".into()],
		}
	);
}
