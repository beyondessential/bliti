//! The system, iwd, radios and gateway a [`Stack`] runs on in a test, answering as they are told.

use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	net::IpAddr,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use futures::future::BoxFuture;

use crate::network::{
	apply::{Hostapd, System},
	observe::{Joined, Operating, Surveyed, Target, WpsFailed, bss::AccessPoint},
	probe::RadioInfo,
	render,
};
/// A directory under the system's temporary directory, removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
	pub fn new() -> Self {
		let dir = std::env::temp_dir().join(format!("bliti-stack-{:016x}", rand::random::<u64>()));
		fs::create_dir(&dir).unwrap();
		Self(dir)
	}

	pub fn paths(&self) -> render::Paths {
		let root = &self.0;
		render::Paths {
			networkd: root.join("network"),
			iwd_state: root.join("iwd"),
			iwd_config: root.join("iwd-config/main.conf"),
			hostapd: root.join("hostapd.conf"),
			modprobe: root.join("modprobe.d/bliti-regdom.conf"),
			resolved: root.join("dns-delegate.d"),
		}
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

/// What the fake system was asked, in order.
pub type Calls = Arc<Mutex<Vec<String>>>;

/// A system recording what it is asked, and failing the calls it is told fail.
pub struct FakeSystem {
	pub calls: Calls,
	pub failing: Arc<Mutex<BTreeSet<String>>>,
}

impl FakeSystem {
	fn call(&mut self, call: String) -> anyhow::Result<()> {
		self.calls.lock().unwrap().push(call.clone());
		if self.failing.lock().unwrap().contains(&call) {
			anyhow::bail!("{call} failed");
		}
		Ok(())
	}
}

impl System for FakeSystem {
	fn set_regulatory_domain(&mut self, domain: &str) -> anyhow::Result<()> {
		self.call(format!("regdom {domain}"))
	}

	fn create_access_point(&mut self, radio: &str, interface: &str) -> anyhow::Result<()> {
		self.call(format!("create {interface} on {radio}"))
	}

	fn delete_access_point(&mut self, interface: &str) -> anyhow::Result<()> {
		self.call(format!("delete {interface}"))
	}

	fn hostapd(&mut self, action: Hostapd) -> anyhow::Result<()> {
		self.call(format!("hostapd {action:?}"))
	}

	fn restart_iwd(&mut self) -> anyhow::Result<()> {
		self.call("restart iwd".into())
	}

	fn reload_networkd(&mut self) -> anyhow::Result<()> {
		self.call("reload networkd".into())
	}

	fn reload_resolved(&mut self) -> anyhow::Result<()> {
		self.call("reload resolved".into())
	}
}

/// An iwd that answers as it is told to.
#[derive(Default)]
pub struct FakeIwd {
	/// How joining each SSID goes.
	pub joins: Mutex<BTreeMap<String, Result<Joined, String>>>,
	/// What every scan hears.
	pub heard: Mutex<BTreeMap<String, i32>>,
	/// How long a scan takes.
	pub scan_takes: Mutex<Duration>,
	/// How WPS goes.
	pub wps: Mutex<Option<Result<Joined, WpsFailed>>>,
	/// What a WPS join left behind.
	pub passphrase: Mutex<Option<String>>,
	pub calls: Mutex<Vec<String>>,
}

impl FakeIwd {
	fn called(&self, call: String) {
		self.calls.lock().unwrap().push(call);
	}

	fn wps_result(&self) -> Result<Joined, WpsFailed> {
		self.wps.lock().unwrap().clone().unwrap_or(Err(WpsFailed {
			found: false,
			reason: "no access point in push-button mode".into(),
		}))
	}
}

impl crate::network::observe::Iwd for FakeIwd {
	fn connect(
		&self,
		station: &str,
		target: &Target,
	) -> BoxFuture<'static, Result<Joined, String>> {
		self.called(format!("connect {station} {}", target.ssid));
		let result = self
			.joins
			.lock()
			.unwrap()
			.get(&target.ssid)
			.cloned()
			.unwrap_or_else(|| Err(format!("{station} does not hear {:?}", target.ssid)));
		Box::pin(async move { result })
	}

	fn disconnect(&self, station: &str) -> BoxFuture<'static, Result<(), String>> {
		self.called(format!("disconnect {station}"));
		Box::pin(async { Ok(()) })
	}

	fn scan(&self, station: &str) -> BoxFuture<'static, Result<BTreeMap<String, i32>, String>> {
		self.called(format!("scan {station}"));
		let heard = self.heard.lock().unwrap().clone();
		let takes = *self.scan_takes.lock().unwrap();
		Box::pin(async move {
			tokio::time::sleep(takes).await;
			Ok(heard)
		})
	}

	fn push_button(&self, station: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>> {
		self.called(format!("push-button {station}"));
		let result = self.wps_result();
		Box::pin(async move { result })
	}

	fn generate_pin(&self, _station: &str) -> BoxFuture<'static, Result<String, String>> {
		Box::pin(async { Ok("12345670".to_owned()) })
	}

	fn start_pin(&self, station: &str, pin: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>> {
		self.called(format!("start-pin {station} {pin}"));
		let result = self.wps_result();
		Box::pin(async move { result })
	}

	fn cancel_wps(&self, station: &str) -> BoxFuture<'static, ()> {
		self.called(format!("cancel-wps {station}"));
		Box::pin(async {})
	}

	fn passphrase(&self, _ssid: &str) -> BoxFuture<'static, Result<String, String>> {
		let passphrase = self.passphrase.lock().unwrap().clone();
		Box::pin(async move { passphrase.ok_or_else(|| "a raw key".to_owned()) })
	}
}

#[derive(Default)]
pub struct FakeAir {
	pub radios: Vec<RadioInfo>,
	pub heard: Vec<AccessPoint>,
	pub surveyed: Vec<Surveyed>,
	pub addresses: BTreeMap<String, String>,
	pub operating: BTreeMap<String, Operating>,
	pub clients: usize,
}

impl crate::network::observe::Air for FakeAir {
	fn radios(&self) -> BoxFuture<'static, anyhow::Result<Vec<RadioInfo>>> {
		let radios = self.radios.clone();
		Box::pin(async move { Ok(radios) })
	}

	fn access_points(
		&self,
		_station: &str,
	) -> BoxFuture<'static, Result<Vec<AccessPoint>, String>> {
		let heard = self.heard.clone();
		Box::pin(async move { Ok(heard) })
	}

	fn survey(&self, _station: &str) -> BoxFuture<'static, Result<Vec<Surveyed>, String>> {
		let surveyed = self.surveyed.clone();
		Box::pin(async move { Ok(surveyed) })
	}

	fn address(&self, interface: &str) -> Option<String> {
		self.addresses.get(interface).cloned()
	}

	fn operating(&self, interface: &str) -> BoxFuture<'static, Result<Option<Operating>, String>> {
		let operating = self.operating.get(interface).copied();
		Box::pin(async move { Ok(operating) })
	}

	fn clients(&self, _interface: &str) -> BoxFuture<'static, Result<usize, String>> {
		let clients = self.clients;
		Box::pin(async move { Ok(clients) })
	}
}

/// A gateway probe answering for the gateways it is told answer.
#[derive(Default)]
pub struct FakeGateway {
	pub answering: Mutex<Vec<IpAddr>>,
	pub asked: Mutex<Vec<(String, IpAddr, IpAddr)>>,
}

impl crate::network::observe::Gateway for FakeGateway {
	fn probe(
		&self,
		interface: &str,
		source: IpAddr,
		gateway: IpAddr,
	) -> BoxFuture<'static, Result<(), String>> {
		self.asked
			.lock()
			.unwrap()
			.push((interface.to_owned(), source, gateway));
		let answers = self.answering.lock().unwrap().contains(&gateway);
		let interface = interface.to_owned();
		Box::pin(async move {
			if answers {
				Ok(())
			} else {
				Err(format!(
					"the gateway {gateway} did not answer on {interface}"
				))
			}
		})
	}
}
