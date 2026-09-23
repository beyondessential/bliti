use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	sync::{Arc, Mutex},
	time::Duration,
};

use bliti_core::channel::config::Document;
use serde_json::{Map, Value as Json, json};
use tokio::sync::{mpsc, oneshot};

use self::fake::{Calls, FakeAir, FakeGateway, FakeIwd, FakeSystem, Scratch};
use super::*;
use crate::network::{
	observe::{Joined, Observation, Operating, Station, Surveyed, bss::AccessPoint},
	probe::{Band, BandInfo, Channel},
	select::Alongside,
};

mod fake;
mod sweep;
mod verdict;

/// A backend on fakes, and the ends a test drives it from.
struct Rig {
	stack: Stack,
	observe: mpsc::UnboundedSender<Observation>,
	system: Calls,
	/// The system calls that fail.
	failing: Arc<Mutex<BTreeSet<String>>>,
	iwd: Arc<FakeIwd>,
	gateway: Arc<FakeGateway>,
	scratch: Scratch,
}

impl Rig {
	async fn new(air: FakeAir) -> Self {
		let scratch = Scratch::new();
		let config = Config {
			wired: vec!["eth0".into()],
			paths: scratch.paths(),
			state: scratch.0.join("record"),
			access_point: "ap0".into(),
		};
		let iwd = Arc::new(FakeIwd::default());
		let gateway = Arc::new(FakeGateway::default());
		let platform = Platform {
			iwd: iwd.clone(),
			air: Arc::new(air),
			gateway: gateway.clone(),
		};
		let system = Calls::default();
		let failing = Arc::<Mutex<BTreeSet<String>>>::default();
		let (observe, observations) = mpsc::unbounded_channel();
		let stack = Stack::start(
			config,
			platform,
			Box::new(FakeSystem {
				calls: system.clone(),
				failing: failing.clone(),
			}),
			observations,
		)
		.await
		.unwrap();
		Self {
			stack,
			observe,
			system,
			failing,
			iwd,
			gateway,
			scratch,
		}
	}

	async fn wired() -> Self {
		Self::new(FakeAir::default()).await
	}

	async fn wireless() -> Self {
		Self::new(FakeAir {
			radios: vec![radio()],
			..FakeAir::default()
		})
		.await
	}

	/// Send observations and let the backend take them in.
	async fn see(&self, observations: impl IntoIterator<Item = Observation>) {
		for observation in observations {
			self.observe.send(observation).unwrap();
		}
		idle().await;
	}

	fn answers(&self, gateway: &str) {
		self.gateway
			.answering
			.lock()
			.unwrap()
			.push(gateway.parse().unwrap());
	}

	fn states(&self) -> Vec<Json> {
		self.stack.states().borrow().clone().unwrap_or_default()
	}

	fn read(&self, below: &str) -> String {
		fs::read_to_string(self.scratch.0.join(below)).unwrap_or_default()
	}

	fn calls(&self) -> Vec<String> {
		self.system.lock().unwrap().clone()
	}

	/// What iwd was asked, in order.
	fn asked(&self) -> Vec<String> {
		self.iwd.calls.lock().unwrap().clone()
	}

	/// Have every scan hear `ssid`, and joining it go as `join` says.
	fn hears(&self, ssid: &str, join: Result<Joined, String>) {
		self.iwd.heard.lock().unwrap().insert(ssid.into(), -55);
		self.iwd.joins.lock().unwrap().insert(ssid.into(), join);
	}

	/// Have scans hear nothing.
	fn hears_nothing(&self) {
		self.iwd.heard.lock().unwrap().clear();
	}
}

/// Let every task that can run, run.
async fn idle() {
	tokio::time::sleep(Duration::from_millis(1)).await;
}

fn radio() -> RadioInfo {
	let channel = |number: u32, frequency: u32| Channel {
		number,
		frequency,
		max_width: 40,
		no_ir: false,
		radar: false,
	};
	RadioInfo {
		station: "wld0".into(),
		model: "brcmfmac".into(),
		bands: BTreeMap::from([(
			Band::TwoPointFour,
			BandInfo {
				channels: vec![channel(1, 2412), channel(6, 2437), channel(11, 2462)],
				widths: vec![20, 40],
			},
		)]),
		alongside: Some(Alongside::SharedChannel),
		sae: true,
		scan: true,
		survey: true,
	}
}

fn document(json: Json) -> Document {
	Document::parse(json.as_object().unwrap()).unwrap()
}

fn dynamic() -> Json {
	json!({"kind": "wired-dynamic", "label": "wall", "verify": true, "interface": "eth0"})
}

fn fixed(address: &str, gateway: &str) -> Json {
	json!({
		"kind": "wired-static", "label": gateway, "verify": true, "interface": "eth0",
		"addresses": [address], "gateway": gateway
	})
}

fn clinic() -> Json {
	json!({
		"kind": "wireless", "label": "clinic", "verify": true, "ssid": "clinic",
		"security": {"kind": "psk", "passphrase": "correct horse"}
	})
}

fn carrier(up: bool) -> Observation {
	Observation::Carrier {
		interface: "eth0".into(),
		up,
	}
}

fn leased(interface: &str, address: &str) -> Observation {
	Observation::Address {
		interface: interface.into(),
		address: address.parse().unwrap(),
		dynamic: true,
		present: true,
	}
}

fn configured(address: &str) -> Observation {
	Observation::Address {
		interface: "eth0".into(),
		address: address.parse().unwrap(),
		dynamic: false,
		present: true,
	}
}

fn routed(interface: &str, gateway: &str) -> Observation {
	Observation::Route {
		interface: interface.into(),
		gateway: gateway.parse().unwrap(),
		present: true,
	}
}

fn joined(ssid: &str, frequency: u32) -> Joined {
	Joined {
		ssid: ssid.into(),
		frequency: Some(frequency),
		security: Some("WPA2-Personal".into()),
	}
}

/// Apply `document` on a task, so the test can observe for it while it is verified.
fn applying(rig: &mut Rig, document: Document) -> oneshot::Receiver<Result<(), Invalid>> {
	proposing(rig, document, true)
}

/// Apply `document`, verified or not.
fn proposing(
	rig: &mut Rig,
	document: Document,
	verify: bool,
) -> oneshot::Receiver<Result<(), Invalid>> {
	let (tx, rx) = oneshot::channel();
	let (reply, answer) = oneshot::channel();
	rig.stack
		.commands
		.send(driver::Command::Apply {
			document,
			verify,
			reply,
		})
		.unwrap();
	tokio::spawn(async move {
		let _ = tx.send(answer.await.unwrap());
	});
	rx
}

#[tokio::test(start_paused = true)]
async fn a_wired_candidate_is_established_on_carrier_address_and_gateway() {
	let mut rig = Rig::wired().await;
	rig.answers("192.0.2.1");
	rig.see([carrier(true)]).await;

	let answer = applying(&mut rig, document(json!({"attachments": [dynamic()]})));
	idle().await;
	assert_eq!(rig.states(), [json!({"is": "verifying"})]);
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;

	assert_eq!(answer.await.unwrap(), Ok(()));
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);
	assert_eq!(
		*rig.gateway.asked.lock().unwrap(),
		[(
			"eth0".to_owned(),
			"192.0.2.10".parse().unwrap(),
			"192.0.2.1".parse().unwrap()
		)]
	);
	assert!(rig.calls().contains(&"reload networkd".to_owned()));
	assert!(
		rig.read("network/50-bliti-eth0.network")
			.contains("DHCP=ipv4")
	);
}

#[tokio::test(start_paused = true)]
async fn a_wrong_key_fails_at_association() {
	let mut rig = Rig::wireless().await;
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -55);
	rig.iwd.joins.lock().unwrap().insert(
		"clinic".into(),
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);

	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("association"));
	assert_eq!(failed.at, "$['attachments'][0]");
	assert_eq!(
		failed.reason,
		"\"clinic\" refused the connection; the passphrase is most likely wrong"
	);
	assert!(
		rig.iwd
			.calls
			.lock()
			.unwrap()
			.contains(&"connect wld0 clinic".to_owned())
	);
	assert_eq!(rig.states()[0]["reached"], "association");
}

#[tokio::test(start_paused = true)]
async fn a_network_out_of_range_fails_at_carrier() {
	let mut rig = Rig::wireless().await;
	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
	assert!(
		!rig.iwd
			.calls
			.lock()
			.unwrap()
			.iter()
			.any(|call| call.starts_with("connect")),
		"nothing out of range is joined"
	);
}

#[tokio::test(start_paused = true)]
async fn no_lease_fails_at_addressing() {
	let mut rig = Rig::wired().await;
	rig.see([carrier(true)]).await;
	let answer = applying(&mut rig, document(json!({"attachments": [dynamic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("addressing"));
	assert_eq!(failed.at, "$['attachments'][0]");
	assert!(failed.reason.contains("no address"), "{}", failed.reason);
}

#[tokio::test(start_paused = true)]
async fn no_route_fails_at_gateway() {
	let mut rig = Rig::wired().await;
	rig.see([carrier(true)]).await;
	let answer = applying(&mut rig, document(json!({"attachments": [dynamic()]})));
	idle().await;
	rig.see([leased("eth0", "192.0.2.10")]).await;
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("gateway"));
	assert!(failed.reason.contains("no gateway"), "{}", failed.reason);
}

#[tokio::test(start_paused = true)]
async fn a_gateway_that_does_not_answer_fails_at_gateway() {
	let mut rig = Rig::wired().await;
	rig.see([carrier(true)]).await;
	let answer = applying(
		&mut rig,
		document(json!({"attachments": [fixed("192.0.2.10/24", "192.0.2.1")]})),
	);
	idle().await;
	rig.see([configured("192.0.2.10")]).await;
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("gateway"));
	assert!(
		failed.reason.contains("did not answer"),
		"{}",
		failed.reason
	);
}

/// Two sites' statics on one port: the proposal holds at whichever site the device is at (LINK).
#[tokio::test(start_paused = true)]
async fn the_static_whose_gateway_answers_is_the_site() {
	let mut rig = Rig::wired().await;
	rig.answers("198.51.100.1");
	rig.see([carrier(true)]).await;
	let answer = applying(
		&mut rig,
		document(json!({"attachments": [
			fixed("192.0.2.10/24", "192.0.2.1"),
			fixed("198.51.100.10/24", "198.51.100.1"),
		]})),
	);
	idle().await;
	rig.see([configured("192.0.2.10")]).await;
	// The first site's gateway is silent, so the second's static is tried next.
	tokio::time::sleep(Duration::from_secs(5)).await;
	rig.see([configured("198.51.100.10")]).await;

	assert_eq!(answer.await.unwrap(), Ok(()));
	let states = rig.states();
	assert_eq!(states[0]["is"], "unavailable");
	assert_eq!(states[0]["reached"], "gateway");
	assert_eq!(states[1], json!({"is": "default-route"}));
	assert!(
		rig.read("network/50-bliti-eth0.network")
			.contains("Address=198.51.100.10/24")
	);
}

#[tokio::test(start_paused = true)]
async fn restore_returns_to_the_recorded_document() {
	let mut rig = Rig::wired().await;
	rig.answers("192.0.2.1");
	rig.answers("198.51.100.1");
	rig.see([carrier(true)]).await;
	let recorded = document(json!({"attachments": [dynamic()]}));
	rig.stack.restore(&recorded).await.unwrap();
	rig.see([
		leased("eth0", "198.51.100.10"),
		routed("eth0", "198.51.100.1"),
	])
	.await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);

	let proposal = document(json!({"attachments": [fixed("192.0.2.10/24", "192.0.2.1")]}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([configured("192.0.2.10")]).await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("network/50-bliti-eth0.network")
			.contains("Address=192.0.2.10/24")
	);

	rig.stack.restore(&recorded).await.unwrap();
	let network = rig.read("network/50-bliti-eth0.network");
	assert!(network.contains("DHCP=ipv4"), "{network}");
	assert!(!network.contains("Address="), "{network}");
	assert_eq!(
		rig.states(),
		[json!({"is": "verifying"})],
		"what the link held under the static is not taken for a lease"
	);
	// The lease the recorded candidate held before is announced again once networkd takes it up.
	rig.see([
		leased("eth0", "198.51.100.10"),
		routed("eth0", "198.51.100.1"),
	])
	.await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);
}

#[tokio::test(start_paused = true)]
async fn states_are_published_as_candidates_change() {
	let mut rig = Rig::wired().await;
	rig.answers("192.0.2.1");
	let mut states = rig.stack.states();
	assert_eq!(*states.borrow_and_update(), None, "nothing configured yet");

	rig.stack
		.restore(&document(json!({"attachments": [dynamic()]})))
		.await
		.unwrap();
	assert_eq!(
		states.borrow_and_update().clone().unwrap()[0]["reached"],
		"carrier",
		"no carrier yet"
	);
	rig.see([carrier(true)]).await;
	assert!(states.has_changed().unwrap());
	assert_eq!(
		states.borrow_and_update().clone(),
		Some(vec![json!({"is": "verifying"})])
	);
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
	assert_eq!(
		states.borrow_and_update().clone(),
		Some(vec![json!({"is": "default-route"})])
	);
	rig.see([carrier(false)]).await;
	assert_eq!(
		states.borrow_and_update().clone().unwrap()[0]["is"],
		"unavailable"
	);
}

/// A shared-channel radio's hotspot follows its station onto each channel it joins (HOT).
#[tokio::test(start_paused = true)]
async fn the_hotspot_follows_the_station_onto_a_new_channel() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -55);
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("hostapd.conf").contains("\nchannel=1\n"),
		"{}",
		rig.read("hostapd.conf")
	);

	rig.system.lock().unwrap().clear();
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Connected(joined("clinic", 2462)),
	}])
	.await;
	idle().await;
	assert!(
		rig.read("hostapd.conf").contains("\nchannel=11\n"),
		"{}",
		rig.read("hostapd.conf")
	);
	assert_eq!(rig.calls(), ["hostapd Restart"]);
}

#[tokio::test(start_paused = true)]
async fn a_scan_answers_each_access_point_but_the_devices_own() {
	let heard = |bssid: [u8; 6], ssid: &[u8]| {
		let mut elements = vec![0, ssid.len() as u8];
		elements.extend_from_slice(ssid);
		AccessPoint::read(bssid, 2437, -4800, 0, &elements)
	};
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio()],
		heard: vec![
			heard([2, 0, 0, 0, 0, 1], b"clinic"),
			heard([2, 0, 0, 0, 0, 2], b""),
			heard([2, 0, 0, 0, 0, 3], b"bliti"),
		],
		addresses: BTreeMap::from([("ap0".into(), "02:00:00:00:00:03".into())]),
		..FakeAir::default()
	})
	.await;
	let entries = rig.stack.scan(None).await.unwrap();
	assert_eq!(
		entries,
		[
			json!({
				"interface": "wld0", "bssid": "02:00:00:00:00:01", "ssid": "clinic",
				"hidden": false, "security": ["open"], "band": "2.4ghz", "channel": 6,
				"channel-width": 20, "signal": -48,
			}),
			json!({
				"interface": "wld0", "bssid": "02:00:00:00:00:02", "ssid": null,
				"hidden": true, "security": ["open"], "band": "2.4ghz", "channel": 6,
				"channel-width": 20, "signal": -48,
			}),
		]
	);
	assert_eq!(*rig.iwd.calls.lock().unwrap(), ["scan wld0"]);
	assert!(rig.stack.scan(Some("wlan9")).await.is_err());
}

#[tokio::test(start_paused = true)]
async fn a_survey_counts_networks_and_busy_time_per_usable_channel() {
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio()],
		heard: vec![AccessPoint::read([2, 0, 0, 0, 0, 1], 2412, -6000, 0, &[])],
		surveyed: vec![Surveyed {
			frequency: 2412,
			active: 200,
			busy: 50,
		}],
		..FakeAir::default()
	})
	.await;
	let spectrum = rig.stack.survey(None).await.unwrap().unwrap();
	assert_eq!(
		Json::Object(spectrum),
		json!({"channels": [
			{"interface": "wld0", "band": "2.4ghz", "channel": 1, "networks": 1, "busy": 0.25},
			{"interface": "wld0", "band": "2.4ghz", "channel": 6, "networks": 0, "busy": 0.0},
			{"interface": "wld0", "band": "2.4ghz", "channel": 11, "networks": 0, "busy": 0.0},
		]})
	);
}

#[tokio::test(start_paused = true)]
async fn wps_passes_on_the_pin_and_answers_the_joined_document() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	*rig.iwd.wps.lock().unwrap() = Some(Ok(joined("clinic", 2437)));
	*rig.iwd.passphrase.lock().unwrap() = Some("correct horse".into());
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -50);
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2437)));
	rig.see([
		Observation::Station {
			interface: "wld0".into(),
			station: Station::Connected(joined("clinic", 2437)),
		},
		leased("wld0", "192.0.2.10"),
		routed("wld0", "192.0.2.1"),
	])
	.await;

	let base = json!({"attachments": [dynamic(), clinic()]});
	rig.stack.restore(&document(base.clone())).await.unwrap();
	let Json::Object(base) = base else {
		unreachable!()
	};
	let (pin, generated) = oneshot::channel();
	let joined = rig.stack.wps("pin", None, &base, pin).await.unwrap();
	assert_eq!(generated.await.unwrap(), "12345670");
	assert_eq!(
		Json::Object(joined),
		json!({"attachments": [
			{
				"kind": "wireless", "label": "clinic", "verify": true, "ssid": "clinic",
				"security": {"kind": "psk", "passphrase": "correct horse"}
			},
			dynamic(),
		]}),
		"the joined network first, and the candidate it replaces gone"
	);
	assert!(
		rig.iwd
			.calls
			.lock()
			.unwrap()
			.contains(&"start-pin wld0 12345670".to_owned())
	);
	assert_eq!(rig.states()[0], json!({"is": "default-route"}));
}

#[tokio::test(start_paused = true)]
async fn a_wps_join_that_finds_nothing_fails_at_carrier() {
	let mut rig = Rig::wireless().await;
	let (pin, _) = oneshot::channel();
	let failed = rig
		.stack
		.wps("push-button", Some("wld0"), &Map::new(), pin)
		.await
		.unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
	assert!(
		failed.reason.contains("push-button mode"),
		"{}",
		failed.reason
	);
}

#[tokio::test(start_paused = true)]
async fn check_refuses_what_the_renderer_would() {
	let rig = Rig::wireless().await;
	let short = document(json!({"attachments": [{
		"kind": "wireless", "label": "x", "verify": true, "ssid": "x",
		"security": {"kind": "psk", "passphrase": "short"}
	}]}));
	let refused = rig.stack.check(&short).unwrap_err();
	assert_eq!(refused.reached, None);
	assert!(refused.at.contains("passphrase"), "{}", refused.at);
	assert!(
		rig.stack
			.check(&document(json!({"attachments": [clinic()]})))
			.is_ok()
	);
}

#[tokio::test(start_paused = true)]
async fn an_sae_candidate_joined_by_anything_else_fails_at_association() {
	let mut rig = Rig::wireless().await;
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -55);
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));
	let mut sae = clinic();
	sae["security"]["kind"] = json!("sae");
	let answer = applying(&mut rig, document(json!({"attachments": [sae]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("association"));
	assert!(failed.reason.contains("not by SAE"), "{}", failed.reason);
	assert!(
		rig.iwd
			.calls
			.lock()
			.unwrap()
			.contains(&"disconnect wld0".to_owned())
	);
}

/// A candidate that failed on an interface left with nothing up is tried again, backing off (LINK).
#[tokio::test(start_paused = true)]
async fn a_failed_candidate_is_tried_again_where_nothing_else_is_up() {
	let mut rig = Rig::wired().await;
	rig.answers("192.0.2.1");
	rig.see([carrier(true)]).await;
	rig.stack
		.restore(&document(json!({"attachments": [dynamic()]})))
		.await
		.unwrap();
	tokio::time::sleep(driver::LEASE).await;
	idle().await;
	assert_eq!(rig.states()[0]["reached"], "addressing");

	tokio::time::sleep(driver::RETRY).await;
	idle().await;
	assert_eq!(rig.states(), [json!({"is": "verifying"})]);
	rig.see([leased("eth0", "192.0.2.10"), routed("eth0", "192.0.2.1")])
		.await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);
}

/// What is joined and run is reported as NFO has it, channels as the radio is on them (NFO).
#[tokio::test(start_paused = true)]
async fn what_is_joined_and_run_is_reported() {
	let on = |frequency| Operating {
		frequency,
		width: Some(20),
	};
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio()],
		operating: BTreeMap::from([("wld0".into(), on(2412)), ("ap0".into(), on(2412))]),
		clients: 2,
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.iwd.heard.lock().unwrap().insert("clinic".into(), -55);
	rig.iwd
		.joins
		.lock()
		.unwrap()
		.insert("clinic".into(), Ok(joined("clinic", 2412)));
	let report = rig.stack.report();
	let entries = |slow| {
		let report = report.clone();
		tokio::task::spawn_blocking(move || report.entries(7, slow, |name| json!({"name": name})))
	};
	assert!(
		entries(true).await.unwrap().is_empty(),
		"nothing joined or run"
	);

	let answer = applying(
		&mut rig,
		document(json!({
			"attachments": [clinic()],
			"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
		})),
	);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));

	let entries_now: Vec<(String, Json, Map<String, Json>)> = entries(true)
		.await
		.unwrap()
		.into_iter()
		.map(|entry| (entry.name, entry.value.unwrap_or_default(), entry.traits))
		.collect();
	let channel = json!({"number": 1, "band": "2.4ghz", "width": 20});
	assert_eq!(entries_now[0].0, "wireless-network");
	assert_eq!(entries_now[0].1, "clinic");
	assert_eq!(entries_now[0].2["interface"], json!({"name": "wld0"}));
	assert_eq!(entries_now[0].2["security"], "psk");
	assert_eq!(entries_now[0].2["channel"], channel);
	assert_eq!(entries_now[1].0, "hotspot");
	assert_eq!(entries_now[1].1, "bliti");
	assert_eq!(entries_now[1].2["channel"], channel);
	assert_eq!(entries_now[2].0, "hotspot-clients");
	assert_eq!(entries_now[2].1, json!(2.0));

	let fast = entries(false).await.unwrap();
	assert_eq!(fast.len(), 1, "only the client count is taken every tick");
	assert_eq!(fast[0].name, "hotspot-clients");
}
