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
	let (tx, rx) = oneshot::channel();
	let (reply, answer) = oneshot::channel();
	rig.stack
		.commands
		.send(driver::Command::Apply { document, reply })
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
	assert_eq!(failed.at, "$['attachments'][0]['security']['passphrase']");
	assert_eq!(
		failed.reason,
		"\"clinic\" refused the connection; the passphrase is most likely wrong"
	);
	let asked = rig.asked();
	let joined = asked.iter().position(|call| call == "connect wld0 clinic");
	assert!(
		joined.is_some_and(|at| asked[at..].contains(&"scan wld0".to_owned())),
		"the radio is scanned after the refusal: {asked:?}"
	);
	assert_eq!(
		rig.states()[0],
		json!({
			"is": "unavailable",
			"reached": "association",
			"reason": "\"clinic\" refused the connection; the passphrase is most likely wrong",
		}),
		"the state names no member"
	);
}

/// Found on a device: with the access point switched off, iwd joined from what it heard while it
/// was on, and the refusal read as a wrong passphrase.
#[tokio::test(start_paused = true)]
async fn a_refusal_from_a_network_gone_fails_at_carrier() {
	let mut rig = Rig::wireless().await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);
	rig.iwd.gone_on_join.lock().unwrap().insert("clinic".into());

	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.at, "$['attachments'][0]");
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
	assert_eq!(failed.reason, "\"clinic\" is out of range");
	assert_eq!(rig.states()[0]["reached"], "carrier");
}

#[tokio::test(start_paused = true)]
async fn a_network_out_of_range_fails_at_carrier() {
	let mut rig = Rig::wireless().await;
	let answer = applying(&mut rig, document(json!({"attachments": [clinic()]})));
	let failed = answer.await.unwrap().unwrap_err();
	assert_eq!(failed.reached.as_deref(), Some("carrier"));
	assert_eq!(failed.at, "$['attachments'][0]");
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
async fn a_probe_after_the_country_changes_asks_no_survey() {
	let air = FakeAir {
		radios: vec![radio()],
		..FakeAir::default()
	};
	let probed = air.probed.clone();
	let mut rig = Rig::new(air).await;
	rig.stack
		.restore(&document(
			json!({"attachments": [dynamic()], "regulatory-domain": "NZ"}),
		))
		.await
		.unwrap();
	idle().await;
	assert_eq!(
		*probed.lock().unwrap(),
		[BTreeMap::new(), BTreeMap::from([("wld0".to_owned(), true)])],
		"a survey retunes the radio across every channel, so it is asked once, at start"
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

/// Found on a device: a wired candidate proposed again under another name failed at its gateway,
/// since its lease was taken for one left by some other configuration and never announced again.
#[tokio::test(start_paused = true)]
async fn renaming_a_candidate_keeps_what_its_link_holds() {
	let mut rig = Rig::wired().await;
	rig.answers("198.51.100.1");
	rig.answers("198.51.100.1");
	rig.see([carrier(true)]).await;
	rig.stack
		.restore(&document(json!({"attachments": [dynamic()]})))
		.await
		.unwrap();
	rig.see([
		leased("eth0", "198.51.100.10"),
		routed("eth0", "198.51.100.1"),
	])
	.await;
	assert_eq!(rig.states(), [json!({"is": "default-route"})]);

	let mut renamed = dynamic();
	renamed["label"] = json!("Office");
	let answer = applying(&mut rig, document(json!({"attachments": [renamed]})));
	assert_eq!(answer.await.unwrap(), Ok(()));
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

/// A shared-channel radio's hotspot starts once its station has joined, on the station's channel,
/// rather than holding the radio on a channel of its own that the station could then join only on.
#[tokio::test(start_paused = true)]
async fn the_hotspot_waits_for_its_station_to_join() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2462)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	let hostapd: Vec<String> = rig
		.calls()
		.into_iter()
		.filter(|call| call.starts_with("hostapd"))
		.collect();
	assert_eq!(hostapd, ["hostapd Start"]);
	assert!(
		rig.read("hostapd.conf").contains("\nchannel=11\n"),
		"{}",
		rig.read("hostapd.conf")
	);
}

/// brcmfmac takes the station off its network as the hotspot starts beside it, and the station
/// joins again on the same channel rather than the candidate failing.
#[tokio::test(start_paused = true)]
async fn a_station_knocked_off_as_the_hotspot_starts_joins_again() {
	let mut rig = Rig::wireless().await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 2412)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	assert!(rig.calls().contains(&"hostapd Start".to_owned()));
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Disconnected,
	}])
	.await;
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	let joins = rig
		.asked()
		.iter()
		.filter(|call| call.starts_with("connect"))
		.count();
	assert_eq!(joins, 2, "{:?}", rig.asked());

	// A later drop, with the hotspot running as rendered, is a drop.
	let states = rig.stack.states();
	rig.see([Observation::Station {
		interface: "wld0".into(),
		station: Station::Disconnected,
	}])
	.await;
	assert_eq!(states.borrow().clone().unwrap()[0]["is"], "unavailable");
}

/// iwd's diagnostics can answer without a frequency. The channel is then read from nl80211, and a
/// station reported joined with none leaves the hotspot where it is.
#[tokio::test(start_paused = true)]
async fn a_join_reported_without_a_channel_takes_it_from_the_radio() {
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio()],
		operating: BTreeMap::from([(
			"wld0".into(),
			Operating {
				frequency: 2412,
				width: Some(20),
			},
		)]),
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	let unknown = Joined {
		frequency: None,
		..joined("clinic", 2412)
	};
	rig.hears("clinic", Ok(unknown.clone()));

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
		station: Station::Connected(unknown),
	}])
	.await;
	idle().await;
	assert_eq!(rig.calls(), Vec::<String>::new());
}

/// A client joined on a channel no access point may start on leaves a shared-channel radio's hotspot
/// no channel to use, which fails the proposal at the hotspot, saying why.
#[tokio::test(start_paused = true)]
async fn a_hotspot_cannot_follow_its_station_onto_a_radar_channel() {
	let mut radio = radio();
	radio.bands.insert(
		Band::Five,
		BandInfo {
			channels: vec![Channel {
				number: 140,
				frequency: 5700,
				max_width: 20,
				no_ir: true,
				radar: true,
			}],
			widths: vec![20],
		},
	);
	let mut rig = Rig::new(FakeAir {
		radios: vec![radio],
		..FakeAir::default()
	})
	.await;
	rig.answers("192.0.2.1");
	rig.hears("clinic", Ok(joined("clinic", 5700)));

	let proposal = document(json!({
		"attachments": [clinic()],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	rig.see([leased("wld0", "192.0.2.10"), routed("wld0", "192.0.2.1")])
		.await;
	let refused = answer.await.unwrap().unwrap_err();
	assert_eq!(refused.at, "$['hotspot']");
	assert!(refused.reason.contains("channel 140"), "{}", refused.reason);
	assert!(refused.reason.contains("radar"), "{}", refused.reason);
	assert_eq!(refused.reached, None);
	assert!(!rig.calls().iter().any(|call| call.starts_with("hostapd")));
}

/// Where its station cannot join, the hotspot stops waiting and runs on a channel of its own.
#[tokio::test(start_paused = true)]
async fn the_hotspot_runs_where_its_station_does_not_join() {
	let mut rig = Rig::wireless().await;
	rig.hears(
		"clinic",
		Err("Operation failed (net.connman.iwd.Failed)".into()),
	);

	let proposal = document(json!({
		"attachments": [{
			"kind": "wireless", "label": "clinic", "verify": false, "ssid": "clinic",
			"security": {"kind": "psk", "passphrase": "correct horse"}
		}],
		"hotspot": {"ssid": "bliti", "passphrase": "read me aloud"},
	}));
	let answer = applying(&mut rig, proposal);
	idle().await;
	assert_eq!(answer.await.unwrap(), Ok(()));
	assert!(
		rig.read("hostapd.conf").contains("\nchannel=6\n"),
		"{}",
		rig.read("hostapd.conf")
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
				"hidden": false, "security": ["open"], "band": "2ghz", "channel": 6,
				"channel-width": 20, "signal": -48,
			}),
			json!({
				"interface": "wld0", "bssid": "02:00:00:00:00:02", "ssid": null,
				"hidden": true, "security": ["open"], "band": "2ghz", "channel": 6,
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
			{"interface": "wld0", "band": "2ghz", "channel": 1, "networks": 1, "busy": 0.25},
			{"interface": "wld0", "band": "2ghz", "channel": 6, "networks": 0, "busy": 0.0},
			{"interface": "wld0", "band": "2ghz", "channel": 11, "networks": 0, "busy": 0.0},
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
	let joined = rig
		.stack
		.wps(&asked("pin", None, None), &base, pin)
		.await
		.unwrap();
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

fn asked(method: &str, interface: Option<&str>, ssid: Option<&str>) -> Wps {
	Wps {
		method: method.to_owned(),
		interface: interface.map(ToOwned::to_owned),
		ssid: ssid.map(ToOwned::to_owned),
	}
}

/// Have WPS hand over `ssid`'s credentials, and joining it answer as a network in range would.
async fn wps_yields(rig: &Rig, ssid: &str) {
	rig.answers("192.0.2.1");
	*rig.iwd.wps.lock().unwrap() = Some(Ok(joined(ssid, 2437)));
	*rig.iwd.passphrase.lock().unwrap() = Some("correct horse".into());
	rig.hears(ssid, Ok(joined(ssid, 2437)));
	rig.see([
		Observation::Station {
			interface: "wld0".into(),
			station: Station::Connected(joined(ssid, 2437)),
		},
		leased("wld0", "192.0.2.10"),
		routed("wld0", "192.0.2.1"),
	])
	.await;
}

#[tokio::test(start_paused = true)]
async fn wps_for_a_named_network_joins_it_where_it_is_what_was_handed_over() {
	let mut rig = Rig::wireless().await;
	wps_yields(&rig, "clinic").await;
	let (pin, _) = oneshot::channel();
	let joined = rig
		.stack
		.wps(
			&asked("push-button", None, Some("clinic")),
			&Map::new(),
			pin,
		)
		.await
		.unwrap();
	assert_eq!(joined["attachments"][0]["ssid"], "clinic");
	assert!(rig.iwd.known.lock().unwrap().contains("clinic"));
	assert!(!rig.asked().iter().any(|call| call.starts_with("forget")));
}

/// iwd's WPS takes no network, so what it hands over for another is forgotten rather than joined
/// (WLAN), refused at the act's `ssid` with no stage reached (CFG).
#[tokio::test(start_paused = true)]
async fn wps_for_a_named_network_forgets_credentials_for_another() {
	let mut rig = Rig::wireless().await;
	rig.stack
		.restore(&document(json!({"attachments": [dynamic()]})))
		.await
		.unwrap();
	wps_yields(&rig, "office").await;
	let before = rig.calls();
	let Json::Object(base) = json!({"attachments": [dynamic()]}) else {
		unreachable!()
	};
	let (pin, _) = oneshot::channel();
	let refused = rig
		.stack
		.wps(
			&asked("push-button", Some("wld0"), Some("clinic")),
			&base,
			pin,
		)
		.await
		.unwrap_err();
	assert_eq!(refused.at, "$['ssid']");
	assert_eq!(refused.reached, None);
	assert!(
		refused.reason.contains("\"office\"") && refused.reason.contains("\"clinic\""),
		"{}",
		refused.reason
	);
	let asked = rig.asked();
	let wps = asked.iter().position(|call| call == "push-button wld0");
	let forgot = asked.iter().position(|call| call == "forget office");
	assert!(wps.is_some() && forgot > wps, "{asked:?}");
	assert!(
		rig.iwd.known.lock().unwrap().is_empty(),
		"nothing of them kept"
	);
	assert_eq!(rig.calls(), before, "nothing was applied");
}

#[tokio::test(start_paused = true)]
async fn a_wps_join_that_finds_nothing_fails_at_carrier() {
	let mut rig = Rig::wireless().await;
	let (pin, _) = oneshot::channel();
	let failed = rig
		.stack
		.wps(&asked("push-button", Some("wld0"), None), &Map::new(), pin)
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
	let channel = json!({"number": 1, "band": "2ghz", "width": 20});
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
