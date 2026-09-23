use std::{
	fs,
	os::unix::fs::{MetadataExt as _, PermissionsExt as _},
};

use bliti_core::channel::config::Document;
use serde_json::{Value as Json, json};

use super::{
	super::render::{self, Band, Channel, PUBLIC, SECRET, Selection},
	*,
};

/// A directory under the system's temporary directory, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
	fn new() -> Self {
		let dir = std::env::temp_dir().join(format!("bliti-apply-{:016x}", rand::random::<u64>()));
		fs::create_dir(&dir).unwrap();
		Self(dir)
	}
}

impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
	Regdom(String),
	CreateAp(String, String),
	DeleteAp(String),
	Hostapd(Hostapd),
	RestartIwd,
	ReloadNetworkd,
	ReloadResolved,
}

/// A [`System`] that records what it is asked, and fails the calls it is told to.
#[derive(Default)]
struct Fake {
	calls: Vec<Call>,
	failing: Vec<Call>,
}

impl Fake {
	fn call(&mut self, call: Call) -> anyhow::Result<()> {
		let fails = self.failing.contains(&call);
		self.calls.push(call);
		if fails {
			anyhow::bail!("the unit is masked");
		}
		Ok(())
	}

	fn take(&mut self) -> Vec<Call> {
		std::mem::take(&mut self.calls)
	}
}

impl System for Fake {
	fn set_regulatory_domain(&mut self, domain: &str) -> anyhow::Result<()> {
		self.call(Call::Regdom(domain.into()))
	}

	fn create_access_point(&mut self, radio: &str, interface: &str) -> anyhow::Result<()> {
		self.call(Call::CreateAp(radio.into(), interface.into()))
	}

	fn delete_access_point(&mut self, interface: &str) -> anyhow::Result<()> {
		self.call(Call::DeleteAp(interface.into()))
	}

	fn hostapd(&mut self, action: Hostapd) -> anyhow::Result<()> {
		self.call(Call::Hostapd(action))
	}

	fn restart_iwd(&mut self) -> anyhow::Result<()> {
		self.call(Call::RestartIwd)
	}

	fn reload_networkd(&mut self) -> anyhow::Result<()> {
		self.call(Call::ReloadNetworkd)
	}

	fn reload_resolved(&mut self) -> anyhow::Result<()> {
		self.call(Call::ReloadResolved)
	}
}

/// A device rooted in a scratch directory, with the state it has been rendered in.
struct Device {
	scratch: Scratch,
	hardware: Hardware,
	selection: Selection,
	system: Fake,
}

impl Device {
	fn new() -> Self {
		let scratch = Scratch::new();
		let root = &scratch.0;
		let hardware = Hardware {
			wired: vec!["eth0".into()],
			station: Some("wlan0".into()),
			access_point: Some("ap0".into()),
			shared_channel: false,
			paths: Paths {
				networkd: root.join("etc/systemd/network"),
				iwd_state: root.join("var/lib/iwd"),
				iwd_config: root.join("etc/iwd/main.conf"),
				hostapd: root.join("etc/hostapd/bliti.conf"),
				modprobe: root.join("etc/modprobe.d/bliti-regdom.conf"),
				resolved: root.join("etc/systemd/dns-delegate.d"),
			},
		};
		Self {
			scratch,
			hardware,
			selection: Selection {
				active: vec![0],
				station_channel: None,
			},
			system: Fake::default(),
		}
	}

	fn state(&self) -> PathBuf {
		self.scratch.0.join("run/bliti/network")
	}

	fn path(&self, below: &str) -> PathBuf {
		self.scratch.0.join(below)
	}

	fn render(&self, document: &Json) -> Rendered {
		let Json::Object(map) = document else {
			panic!("a document is an object")
		};
		let document = Document::parse(map).unwrap();
		render::render(&document, &self.hardware, &self.selection).unwrap()
	}

	fn try_apply(&mut self, document: &Json) -> Result<Changes, Error> {
		let rendered = self.render(document);
		apply(&rendered, &self.hardware, &self.state(), &mut self.system)
	}

	fn apply(&mut self, document: &Json) -> Changes {
		self.try_apply(document).unwrap()
	}
}

/// A wired candidate, a wireless one and a hotspot: every backend has a file.
fn full() -> Json {
	json!({
		"regulatory-domain": "NZ",
		"attachments": [
			{ "kind": "wired-dynamic", "label": "wall", "verify": true, "interface": "eth0", "nameservers": ["1.1.1.1"] },
			wireless("Clinic", "a long passphrase")
		],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	})
}

fn wireless(ssid: &str, passphrase: &str) -> Json {
	json!({
		"kind": "wireless", "label": ssid, "verify": true, "ssid": ssid,
		"security": { "kind": "psk", "passphrase": passphrase }
	})
}

fn written(paths: &[PathBuf]) -> Vec<Change> {
	paths.iter().cloned().map(Change::Written).collect()
}

fn mode(path: &Path) -> u32 {
	fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// Every file lands with its contents and its mode, and nothing temporary is left beside it.
#[test]
fn files_land_with_their_modes() {
	let mut device = Device::new();
	let rendered = device.render(&full());
	device.apply(&full());

	for file in &rendered.files {
		assert_eq!(fs::read_to_string(&file.path).unwrap(), file.contents);
		assert_eq!(mode(&file.path), file.mode, "{:?}", file.path);
	}
	assert_eq!(mode(&device.path("var/lib/iwd/Clinic.psk")), SECRET);
	assert_eq!(mode(&device.path("etc/hostapd/bliti.conf")), SECRET);
	assert_eq!(
		mode(&device.path("etc/systemd/network/50-bliti-eth0.network")),
		PUBLIC
	);
	assert_eq!(mode(&device.state().join("applied.json")), SECRET);

	for dir in [
		"etc/systemd/network",
		"var/lib/iwd",
		"etc/iwd",
		"etc/hostapd",
		"run/bliti/network",
	] {
		for entry in fs::read_dir(device.path(dir)).unwrap() {
			let name = entry.unwrap().file_name();
			assert!(
				!name.to_string_lossy().ends_with(".bliti-tmp"),
				"{name:?} left in {dir}"
			);
		}
	}
}

/// A secret is never readable by others, whatever the umask would have allowed.
#[test]
fn a_secret_is_written_with_its_mode_from_the_start() {
	let scratch = Scratch::new();
	let path = scratch.0.join("dir/secret");
	files::write_atomically(&path, b"hunter2", SECRET).unwrap();
	assert_eq!(mode(&path), SECRET);
	files::write_atomically(&path, b"hunter3", PUBLIC).unwrap();
	assert_eq!(mode(&path), PUBLIC);
	assert_eq!(fs::read_to_string(&path).unwrap(), "hunter3");
}

/// The first apply writes everything and has each backend pick it up, regulatory domain first, then
/// the hotspot's interface and hostapd, then the station, then networkd, then resolved.
#[test]
fn the_first_apply_picks_everything_up_in_order() {
	let mut device = Device::new();
	let changes = device.apply(&full());
	assert_eq!(
		device.system.take(),
		[
			Call::Regdom("NZ".into()),
			Call::CreateAp("wlan0".into(), "ap0".into()),
			Call::Hostapd(Hostapd::Start),
			Call::RestartIwd,
			Call::ReloadNetworkd,
			Call::ReloadResolved,
		]
	);
	assert_eq!(
		changes,
		Changes {
			regdom: written(&[device.path("etc/modprobe.d/bliti-regdom.conf")]),
			hostapd: written(&[device.path("etc/hostapd/bliti.conf")]),
			iwd: written(&[
				device.path("etc/iwd/main.conf"),
				device.path("var/lib/iwd/Clinic.psk"),
			]),
			networkd: written(&[
				device.path("etc/systemd/network/50-bliti-ap0.network"),
				device.path("etc/systemd/network/50-bliti-eth0.network"),
			]),
			resolved: written(&[
				device.path("etc/systemd/dns-delegate.d/50-bliti-eth0.dns-delegate")
			]),
		}
	);
}

/// An unset domain is the world domain at runtime too.
#[test]
fn an_unset_domain_is_the_world() {
	let mut device = Device::new();
	device.selection.active.clear();
	device.apply(&json!({ "attachments": [] }));
	assert_eq!(device.system.take()[0], Call::Regdom("00".into()));
}

/// A render the same as the last one writes nothing and calls nothing.
#[test]
fn an_unchanged_render_does_nothing() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();
	let record = device.state().join("applied.json");
	let inodes = |device: &Device| -> Vec<u64> {
		device
			.render(&full())
			.files
			.iter()
			.map(|file| file.path.clone())
			.chain([record.clone()])
			.map(|path| fs::metadata(path).unwrap().ino())
			.collect()
	};
	let before = inodes(&device);

	let changes = device.apply(&full());
	assert!(changes.is_empty(), "{changes:?}");
	assert_eq!(device.system.take(), []);

	let after = inodes(&device);
	assert_eq!(before, after, "a file was replaced");
}

/// iwd rewriting its own known-network file is not a change bliti undoes.
#[test]
fn a_file_iwd_rewrote_is_not_drift() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();
	let clinic = device.path("var/lib/iwd/Clinic.psk");
	let rewritten = format!(
		"{}PreSharedKey=0123456789abcdef\n",
		fs::read_to_string(&clinic).unwrap()
	);
	fs::write(&clinic, &rewritten).unwrap();

	assert!(device.apply(&full()).is_empty());
	assert_eq!(device.system.take(), []);
	assert_eq!(fs::read_to_string(&clinic).unwrap(), rewritten);
}

/// A file that has gone from disk is put back, even where the record holds it.
#[test]
fn a_missing_file_is_written_again() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();
	let eth0 = device.path("etc/systemd/network/50-bliti-eth0.network");
	fs::remove_file(&eth0).unwrap();

	let changes = device.apply(&full());
	assert_eq!(changes.networkd, written(std::slice::from_ref(&eth0)));
	assert_eq!(device.system.take(), [Call::ReloadNetworkd]);
	assert!(eth0.exists());
}

/// Files bliti owns that the render no longer holds go; everyone else's stay.
#[test]
fn stale_files_go_and_foreign_ones_stay() {
	let mut device = Device::new();
	let stale = [
		device.path("etc/systemd/network/50-bliti-eth9.network"),
		device.path("var/lib/iwd/Old Network.psk"),
	];
	let foreign = [
		device.path("etc/systemd/network/10-netplan-eth0.network"),
		device.path("etc/systemd/network/50-bliti-eth0.netdev"),
		device.path("var/lib/iwd/.known_network.freq"),
		device.path("var/lib/iwd/hotspot"),
	];
	for path in stale.iter().chain(&foreign) {
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(path, "someone's").unwrap();
	}

	let changes = device.apply(&full());
	for path in &stale {
		assert!(!path.exists(), "{path:?} kept");
	}
	for path in &foreign {
		assert_eq!(fs::read_to_string(path).unwrap(), "someone's");
	}
	assert!(changes.iwd.contains(&Change::Removed(stale[1].clone())));
	assert!(
		changes
			.networkd
			.contains(&Change::Removed(stale[0].clone()))
	);
}

/// A candidate dropped from the document takes its file with it.
#[test]
fn a_dropped_candidate_is_removed() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut without = full();
	without["attachments"].as_array_mut().unwrap().pop();
	let changes = device.apply(&without);
	let clinic = device.path("var/lib/iwd/Clinic.psk");
	assert_eq!(
		changes,
		Changes {
			iwd: vec![Change::Removed(clinic.clone())],
			..Changes::default()
		}
	);
	assert!(!clinic.exists());
	assert_eq!(device.system.take(), []);
}

/// A new known network needs nothing of iwd, which watches its directory.
#[test]
fn a_known_network_is_picked_up_by_iwd_itself() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut more = full();
	more["attachments"]
		.as_array_mut()
		.unwrap()
		.push(wireless("Guest", "another long one"));
	let changes = device.apply(&more);
	assert_eq!(
		changes,
		Changes {
			iwd: written(&[device.path("var/lib/iwd/Guest.psk")]),
			..Changes::default()
		}
	);
	assert_eq!(device.system.take(), []);
}

/// A wired change reloads networkd and touches nothing else.
#[test]
fn a_networkd_change_only_reloads_networkd() {
	let mut base = full();
	base["attachments"][0]
		.as_object_mut()
		.unwrap()
		.remove("nameservers");
	let mut device = Device::new();
	device.apply(&base);
	device.system.take();

	let mut static_ = base;
	static_["attachments"][0] = json!({
		"kind": "wired-static", "label": "lab", "verify": true, "interface": "eth0",
		"addresses": ["10.1.0.5/24"], "gateway": "10.1.0.1"
	});
	let changes = device.apply(&static_);
	assert_eq!(
		changes,
		Changes {
			networkd: written(&[device.path("etc/systemd/network/50-bliti-eth0.network")]),
			..Changes::default()
		}
	);
	assert_eq!(device.system.take(), [Call::ReloadNetworkd]);
}

/// A hotspot change restarts hostapd and touches nothing else.
#[test]
fn a_hostapd_change_only_restarts_hostapd() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut other = full();
	other["hotspot"]["passphrase"] = json!("a different one");
	let changes = device.apply(&other);
	assert_eq!(
		changes,
		Changes {
			hostapd: written(&[device.path("etc/hostapd/bliti.conf")]),
			..Changes::default()
		}
	);
	assert_eq!(device.system.take(), [Call::Hostapd(Hostapd::Restart)]);
}

/// A domain change sets the domain first, and each file carrying it is picked up in order.
#[test]
fn a_domain_change_reaches_every_backend_carrying_it() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut other = full();
	other["regulatory-domain"] = json!("AU");
	let changes = device.apply(&other);
	assert!(changes.networkd.is_empty());
	assert_eq!(
		device.system.take(),
		[
			Call::Regdom("AU".into()),
			Call::Hostapd(Hostapd::Restart),
			Call::RestartIwd,
		]
	);
}

/// Taking the hotspot away stops hostapd before its interface goes.
#[test]
fn a_dropped_hotspot_stops_hostapd_and_deletes_its_interface() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut without = full();
	without.as_object_mut().unwrap().remove("hotspot");
	let changes = device.apply(&without);
	assert_eq!(
		changes.hostapd,
		[Change::Removed(device.path("etc/hostapd/bliti.conf"))]
	);
	assert_eq!(
		device.system.take(),
		[
			Call::Hostapd(Hostapd::Stop),
			Call::DeleteAp("ap0".into()),
			Call::ReloadNetworkd,
		]
	);
}

/// On a shared-channel radio the station moving channel restarts hostapd on the new one.
#[test]
fn a_channel_change_restarts_hostapd() {
	let mut device = Device::new();
	device.hardware.shared_channel = true;
	device.selection.station_channel = Some(Channel {
		band: Band::TwoPointFour,
		number: 1,
	});
	device.apply(&full());
	device.system.take();

	device.selection.station_channel = Some(Channel {
		band: Band::TwoPointFour,
		number: 11,
	});
	let changes = device.apply(&full());
	assert_eq!(
		changes,
		Changes {
			hostapd: written(&[device.path("etc/hostapd/bliti.conf")]),
			..Changes::default()
		}
	);
	assert_eq!(device.system.take(), [Call::Hostapd(Hostapd::Restart)]);
	assert!(
		fs::read_to_string(device.path("etc/hostapd/bliti.conf"))
			.unwrap()
			.contains("channel=11\n")
	);
}

/// A failing call is reported with its backend, stops the backends after it, and is tried again on
/// the next apply.
#[test]
fn a_failing_call_names_its_backend_and_is_retried() {
	let mut device = Device::new();
	device.system.failing = vec![Call::Hostapd(Hostapd::Start)];

	let error = device.try_apply(&full()).unwrap_err();
	assert!(
		matches!(
			error,
			Error::System {
				backend: Backend::Hostapd,
				..
			}
		),
		"{error:?}"
	);
	assert_eq!(error.to_string(), "hostapd: the unit is masked");
	assert_eq!(
		device.system.take(),
		[
			Call::Regdom("NZ".into()),
			Call::CreateAp("wlan0".into(), "ap0".into()),
			Call::Hostapd(Hostapd::Start),
		]
	);

	device.system.failing.clear();
	let changes = device.apply(&full());
	assert!(changes.regdom.is_empty());
	assert_eq!(
		device.system.take(),
		[
			Call::CreateAp("wlan0".into(), "ap0".into()),
			Call::Hostapd(Hostapd::Start),
			Call::RestartIwd,
			Call::ReloadNetworkd,
			Call::ReloadResolved,
		]
	);
}

/// networkd failing to reload is networkd's failure, and not swallowed.
#[test]
fn a_failing_reload_names_networkd() {
	let mut device = Device::new();
	device.system.failing = vec![Call::ReloadNetworkd];
	let error = device.try_apply(&full()).unwrap_err();
	assert!(matches!(
		error,
		Error::System {
			backend: Backend::Networkd,
			..
		}
	));
	assert!(error.to_string().starts_with("systemd-networkd: "));
}

/// A file the renderer put somewhere bliti does not own is refused before anything is written.
#[test]
fn a_foreign_rendered_file_is_refused() {
	let mut device = Device::new();
	let mut rendered = device.render(&full());
	let foreign = device.path("etc/passwd");
	rendered.files.push(render::File {
		path: foreign.clone(),
		contents: String::new(),
		mode: PUBLIC,
	});
	let error = apply(
		&rendered,
		&device.hardware,
		&device.state(),
		&mut device.system,
	)
	.unwrap_err();
	assert!(matches!(error, Error::Foreign(path) if path == foreign));
	assert_eq!(device.system.take(), []);
	assert!(!device.path("etc/iwd").exists());
}

/// A record that cannot be read costs one apply of everything, not a failure.
#[test]
fn an_unreadable_record_applies_afresh() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();
	fs::write(device.state().join("applied.json"), "not json").unwrap();

	let changes = device.apply(&full());
	assert_eq!(changes.iwd.len(), 2);
	assert_eq!(device.system.take().len(), 6);
}

#[test]
fn the_domain_is_read_from_the_modprobe_file() {
	assert_eq!(
		regulatory_domain("# header\noptions cfg80211 ieee80211_regdom=NZ\n"),
		Some("NZ")
	);
	assert_eq!(regulatory_domain("# nothing\n"), None);
}

/// A dynamic link's own resolvers live in a delegate, which only resolved picks up, and dropping them
/// takes the delegate away and has resolved read that too.
#[test]
fn resolvers_of_its_own_are_resolved_s_to_pick_up() {
	let mut device = Device::new();
	device.apply(&full());
	device.system.take();

	let mut without = full();
	without["attachments"][0]
		.as_object_mut()
		.unwrap()
		.remove("nameservers");
	let changes = device.apply(&without);
	assert_eq!(
		changes.resolved,
		[Change::Removed(device.path(
			"etc/systemd/dns-delegate.d/50-bliti-eth0.dns-delegate"
		))]
	);
	assert_eq!(
		device.system.take(),
		[Call::ReloadNetworkd, Call::ReloadResolved]
	);
}
