//! iwd over the system bus: what each station is joined to and hears, and what the backend has it
//! do.
//!
//! The members used are those of iwd's D-Bus API documents (`doc/station-api.txt`,
//! `doc/network-api.txt`, `doc/device-api.txt`, `doc/wsc-api.txt` and
//! `doc/station-diagnostic-api.txt` in iwd's tree): `Station.State`, `Station.Scanning`,
//! `Station.ConnectedNetwork`, `Station.Scan`, `Station.GetOrderedNetworks`, `Station.Disconnect`,
//! `Station.ConnectHiddenNetwork`, `Network.Connect` with `Name`, `Type` and `Device`, `Device.Name`,
//! `StationDiagnostic.GetDiagnostics` for `Frequency` and `Security`, and `SimpleConfiguration`'s
//! `PushButton`, `GeneratePin`, `StartPin` and `Cancel`, and `AgentManager.RegisterAgent` for the
//! [`agent`].

use std::{
	collections::{BTreeMap, HashMap},
	path::PathBuf,
	sync::Arc,
	time::Duration,
};

use dbus::{
	Message, Path as ObjectPath,
	arg::{PropMap, RefArg},
	message::MatchRule,
	nonblock::{
		Proxy, SyncConnection,
		stdintf::org_freedesktop_dbus::{ObjectManager as _, Properties as _},
	},
};
use futures::{StreamExt as _, future::BoxFuture};
use tokio::sync::mpsc;

use super::{Joined, Observation, Station, Target, WpsFailed};
use crate::network::render;

mod agent;

const SERVICE: &str = "net.connman.iwd";
const STATION: &str = "net.connman.iwd.Station";
const NETWORK: &str = "net.connman.iwd.Network";
const DEVICE: &str = "net.connman.iwd.Device";
const DIAGNOSTIC: &str = "net.connman.iwd.StationDiagnostic";
const WPS: &str = "net.connman.iwd.SimpleConfiguration";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

/// How long an ordinary call waits for its reply.
const CALL: Duration = Duration::from_secs(10);

/// How long joining may take. iwd answers `Network.Connect` once associated, or once it gives up.
const CONNECT: Duration = Duration::from_secs(90);

/// How long a scan may take.
const SCAN: Duration = Duration::from_secs(30);

/// How long a WPS join may take: its walk time is two minutes.
const WALK: Duration = Duration::from_secs(150);

/// iwd, over the system bus.
#[derive(Clone)]
pub(super) struct Iwd {
	bus: Arc<SyncConnection>,
	/// iwd's state directory, where it keeps what a WPS join yielded.
	paths: render::Paths,
}

type Objects = HashMap<ObjectPath<'static>, HashMap<String, PropMap>>;

fn failed(error: dbus::Error) -> String {
	match (error.name(), error.message()) {
		(Some(name), Some(message)) => format!("{message} ({name})"),
		(Some(name), None) => name.to_owned(),
		(None, Some(message)) => message.to_owned(),
		(None, None) => "iwd did not say why".to_owned(),
	}
}

fn string(props: &PropMap, key: &str) -> Option<String> {
	props.get(key)?.0.as_str().map(str::to_owned)
}

impl Iwd {
	/// Connect to the system bus, and watch iwd, sending what is observed on `observations`.
	pub(super) async fn start(
		paths: render::Paths,
		observations: mpsc::UnboundedSender<Observation>,
	) -> anyhow::Result<Self> {
		let (resource, bus) = dbus_tokio::connection::new_system_sync()?;
		// The watch matches every signal iwd sends, and a scan matches its station's too. dbus hands a
		// signal to the first match alone unless told otherwise, which starves every match but one.
		bus.set_signal_match_mode(true);
		tokio::spawn(async move {
			let error = resource.await;
			tracing::error!(%error, "lost the system bus; iwd is no longer observed");
		});
		agent::serve(&bus, paths.clone());
		let iwd = Self { bus, paths };
		iwd.clone().watch(observations).await?;
		Ok(iwd)
	}

	fn proxy<'a>(
		&self,
		path: impl Into<ObjectPath<'a>>,
		timeout: Duration,
	) -> Proxy<'a, Arc<SyncConnection>> {
		Proxy::new(SERVICE, path, timeout, self.bus.clone())
	}

	async fn objects(&self) -> Result<Objects, String> {
		self.proxy("/", CALL)
			.get_managed_objects()
			.await
			.map_err(failed)
	}

	/// The station object whose device is `name`.
	async fn station(&self, name: &str) -> Result<ObjectPath<'static>, String> {
		self.objects()
			.await?
			.into_iter()
			.find(|(_, interfaces)| {
				interfaces.contains_key(STATION)
					&& interfaces
						.get(DEVICE)
						.and_then(|device| string(device, "Name"))
						.is_some_and(|device| device == name)
			})
			.map(|(path, _)| path)
			.ok_or_else(|| format!("iwd has no station on {name}"))
	}

	/// What the station at `path` is joined to.
	async fn joined(&self, path: &ObjectPath<'static>) -> Result<Joined, String> {
		let proxy = self.proxy(path.clone(), CALL);
		let network: ObjectPath<'static> = proxy
			.get(STATION, "ConnectedNetwork")
			.await
			.map_err(failed)?;
		let ssid: String = self
			.proxy(network, CALL)
			.get(NETWORK, "Name")
			.await
			.map_err(failed)?;
		// Diagnostics are experimental in iwd; a join is not refused for want of them.
		let diagnostics: PropMap = proxy
			.method_call(DIAGNOSTIC, "GetDiagnostics", ())
			.await
			.map(|(diagnostics,): (PropMap,)| diagnostics)
			.unwrap_or_default();
		Ok(Joined {
			ssid,
			frequency: diagnostics
				.get("Frequency")
				.and_then(|value| value.0.as_u64())
				.and_then(|frequency| u32::try_from(frequency).ok()),
			security: string(&diagnostics, "Security"),
		})
	}

	/// What the station at `path` heard in its latest scan, by SSID at the strongest signal.
	async fn ordered(&self, path: &ObjectPath<'static>) -> Result<BTreeMap<String, i32>, String> {
		let (networks,): (Vec<(ObjectPath<'static>, i16)>,) = self
			.proxy(path.clone(), CALL)
			.method_call(STATION, "GetOrderedNetworks", ())
			.await
			.map_err(failed)?;
		let mut heard = BTreeMap::new();
		for (network, signal) in networks {
			let ssid: String = self
				.proxy(network, CALL)
				.get(NETWORK, "Name")
				.await
				.map_err(failed)?;
			let dbm = i32::from(signal) / 100;
			heard
				.entry(ssid)
				.and_modify(|best: &mut i32| *best = (*best).max(dbm))
				.or_insert(dbm);
		}
		Ok(heard)
	}

	/// The station at `path` as it stands.
	async fn state(&self, path: &ObjectPath<'static>, state: &str) -> Station {
		match state {
			"connected" => match self.joined(path).await {
				Ok(joined) => Station::Connected(joined),
				Err(reason) => {
					tracing::warn!(%path, reason, "connected, but iwd would not say to what");
					Station::Busy
				}
			},
			"disconnected" => Station::Disconnected,
			_ => Station::Busy,
		}
	}

	/// Watch iwd's stations: their state as it changes, and what they hear after every scan.
	async fn watch(self, observations: mpsc::UnboundedSender<Observation>) -> anyhow::Result<()> {
		let signals = self
			.bus
			.add_match(
				MatchRule::new()
					.with_sender(SERVICE)
					.with_type(dbus::MessageType::Signal),
			)
			.await?;
		let (signals, mut incoming) = signals.msg_stream();
		let owner = self
			.bus
			.add_match(
				MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
					.with_sender("org.freedesktop.DBus"),
			)
			.await?;
		let (owner, mut owners) = owner.msg_stream();

		let mut names: HashMap<ObjectPath<'static>, String> = HashMap::new();
		agent::register(&self.bus).await;
		self.resync(&mut names, &observations).await;
		tokio::spawn(async move {
			// Held for as long as the watch runs; dropping a match removes it.
			let _matches = (signals, owner);
			loop {
				tokio::select! {
					Some(message) = incoming.next() => {
						self.signal(message, &mut names, &observations).await;
					}
					Some(message) = owners.next() => {
						let Ok((name, _, new)) = message.read3::<String, String, String>() else {
							continue;
						};
						if name != SERVICE {
							continue;
						}
						// iwd restarting takes every station with it.
						for interface in std::mem::take(&mut names).into_values() {
							let _ = observations.send(Observation::Station {
								interface: interface.clone(),
								station: Station::Disconnected,
							});
							let _ = observations.send(Observation::Heard {
								interface,
								networks: BTreeMap::new(),
							});
						}
						if !new.is_empty() {
							agent::register(&self.bus).await;
							self.resync(&mut names, &observations).await;
						}
					}
					else => break,
				}
				if observations.is_closed() {
					return;
				}
			}
			tracing::error!("iwd's signals stopped; wireless stations are no longer observed");
		});
		Ok(())
	}

	/// Read every station afresh.
	async fn resync(
		&self,
		names: &mut HashMap<ObjectPath<'static>, String>,
		observations: &mpsc::UnboundedSender<Observation>,
	) {
		let objects = match self.objects().await {
			Ok(objects) => objects,
			Err(reason) => {
				tracing::warn!(
					reason,
					"iwd is not answering; wireless stations are observed once it does"
				);
				return;
			}
		};
		for (path, interfaces) in objects {
			self.added(path, &interfaces, names, observations).await;
		}
	}

	/// Take in an object iwd has, where it is a station.
	async fn added(
		&self,
		path: ObjectPath<'static>,
		interfaces: &HashMap<String, PropMap>,
		names: &mut HashMap<ObjectPath<'static>, String>,
		observations: &mpsc::UnboundedSender<Observation>,
	) {
		let (Some(station), Some(device)) = (interfaces.get(STATION), interfaces.get(DEVICE))
		else {
			return;
		};
		let Some(interface) = string(device, "Name") else {
			return;
		};
		names.insert(path.clone(), interface.clone());
		let state = string(station, "State").unwrap_or_default();
		let station = self.state(&path, &state).await;
		let _ = observations.send(Observation::Station {
			interface: interface.clone(),
			station,
		});
		match self.ordered(&path).await {
			Ok(networks) => {
				let _ = observations.send(Observation::Heard {
					interface,
					networks,
				});
			}
			Err(reason) => tracing::warn!(interface, reason, "cannot read what the station hears"),
		}
	}

	async fn signal(
		&self,
		message: Message,
		names: &mut HashMap<ObjectPath<'static>, String>,
		observations: &mpsc::UnboundedSender<Observation>,
	) {
		let (Some(interface), Some(member)) = (message.interface(), message.member()) else {
			return;
		};
		match (&*interface, &*member) {
			(PROPERTIES, "PropertiesChanged") => {
				let Some(path) = message.path().map(|path| path.into_static()) else {
					return;
				};
				let Ok((of, changed)) = message.read2::<String, PropMap>() else {
					return;
				};
				let Some(name) = names.get(&path).cloned() else {
					return;
				};
				if of != STATION {
					return;
				}
				if let Some(state) = string(&changed, "State") {
					let station = self.state(&path, &state).await;
					let _ = observations.send(Observation::Station {
						interface: name.clone(),
						station,
					});
				}
				let scanned = changed
					.get("Scanning")
					.and_then(|value| value.0.as_u64())
					.is_some_and(|scanning| scanning == 0);
				if scanned && let Ok(networks) = self.ordered(&path).await {
					let _ = observations.send(Observation::Heard {
						interface: name,
						networks,
					});
				}
			}
			(OBJECT_MANAGER, "InterfacesAdded") => {
				let Ok((path, interfaces)) =
					message.read2::<ObjectPath<'static>, HashMap<String, PropMap>>()
				else {
					return;
				};
				self.added(path, &interfaces, names, observations).await;
			}
			(OBJECT_MANAGER, "InterfacesRemoved") => {
				let Ok((path, interfaces)) = message.read2::<ObjectPath<'static>, Vec<String>>()
				else {
					return;
				};
				if interfaces.iter().any(|interface| interface == STATION)
					&& let Some(interface) = names.remove(&path)
				{
					let _ = observations.send(Observation::Station {
						interface: interface.clone(),
						station: Station::Disconnected,
					});
					let _ = observations.send(Observation::Heard {
						interface,
						networks: BTreeMap::new(),
					});
				}
			}
			_ => {}
		}
	}

	async fn connect_to(&self, station: &str, target: &Target) -> Result<Joined, String> {
		let path = self.station(station).await?;
		let network = self.objects().await?.into_iter().find(|(_, interfaces)| {
			interfaces.get(NETWORK).is_some_and(|network| {
				string(network, "Name").as_deref() == Some(&target.ssid)
					&& string(network, "Type").as_deref() == Some(target.kind.as_iwd())
					&& network
						.get("Device")
						.and_then(|device| device.0.as_str())
						.is_some_and(|device| device == &*path)
			})
		});
		match network {
			Some((network, _)) => self
				.proxy(network, CONNECT)
				.method_call::<(), _, _, _>(NETWORK, "Connect", ())
				.await
				.map_err(failed)?,
			None if target.hidden => self
				.proxy(path.clone(), CONNECT)
				.method_call::<(), _, _, _>(
					STATION,
					"ConnectHiddenNetwork",
					(target.ssid.as_str(),),
				)
				.await
				.map_err(failed)?,
			None => return Err(format!("{station} does not hear {:?}", target.ssid)),
		}
		self.joined(&path).await
	}

	async fn scan_on(&self, station: &str) -> Result<BTreeMap<String, i32>, String> {
		let path = self.station(station).await?;
		let rule = MatchRule::new_signal(PROPERTIES, "PropertiesChanged")
			.with_sender(SERVICE)
			.with_path(path.clone());
		let matched = self.bus.add_match(rule).await.map_err(failed)?;
		let (matched, mut changes) = matched.msg_stream();
		let scanning = self
			.proxy(path.clone(), CALL)
			.method_call::<(), _, _, _>(STATION, "Scan", ())
			.await;
		match scanning {
			Ok(()) => {}
			// A scan already under way is one to wait for. iwd is also busy while it connects, and then
			// no scan is coming: what it already hears is the answer.
			Err(error) if error.name() == Some("net.connman.iwd.Busy") => {
				let under_way: bool = self
					.proxy(path.clone(), CALL)
					.get(STATION, "Scanning")
					.await
					.map_err(failed)?;
				if !under_way {
					drop(matched);
					return self.ordered(&path).await;
				}
			}
			Err(error) => return Err(failed(error)),
		}
		let finished = async {
			while let Some(message) = changes.next().await {
				let Ok((of, changed)) = message.read2::<String, PropMap>() else {
					continue;
				};
				let done = changed
					.get("Scanning")
					.and_then(|value| value.0.as_u64())
					.is_some_and(|scanning| scanning == 0);
				if of == STATION && done {
					return;
				}
			}
		};
		let waited = tokio::time::timeout(SCAN, finished).await;
		drop(matched);
		if waited.is_err() {
			return Err(format!(
				"the scan did not finish within {} seconds",
				SCAN.as_secs()
			));
		}
		self.ordered(&path).await
	}

	async fn wps(
		&self,
		station: &str,
		method: &'static str,
		pin: Option<String>,
	) -> Result<Joined, WpsFailed> {
		let path = self.station(station).await.map_err(|reason| WpsFailed {
			found: false,
			reason,
		})?;
		let proxy = self.proxy(path.clone(), WALK);
		let joined = match pin {
			Some(pin) => proxy.method_call::<(), _, _, _>(WPS, method, (pin,)).await,
			None => proxy.method_call::<(), _, _, _>(WPS, method, ()).await,
		};
		joined.map_err(|error| {
			let unfound = matches!(
				error.name(),
				Some(
					"net.connman.iwd.SimpleConfiguration.NotReachable"
						| "net.connman.iwd.SimpleConfiguration.WalkTimerExpired"
				)
			);
			WpsFailed {
				found: !unfound,
				reason: failed(error),
			}
		})?;
		self.joined(&path).await.map_err(|reason| WpsFailed {
			found: true,
			reason,
		})
	}
}

impl super::Iwd for Iwd {
	fn connect(
		&self,
		station: &str,
		target: &Target,
	) -> BoxFuture<'static, Result<Joined, String>> {
		let (iwd, station, target) = (self.clone(), station.to_owned(), target.clone());
		Box::pin(async move { iwd.connect_to(&station, &target).await })
	}

	fn disconnect(&self, station: &str) -> BoxFuture<'static, Result<(), String>> {
		let (iwd, station) = (self.clone(), station.to_owned());
		Box::pin(async move {
			let path = iwd.station(&station).await?;
			match iwd
				.proxy(path, CALL)
				.method_call::<(), _, _, _>(STATION, "Disconnect", ())
				.await
			{
				Err(error) if error.name() != Some("net.connman.iwd.NotConnected") => {
					Err(failed(error))
				}
				_ => Ok(()),
			}
		})
	}

	fn scan(&self, station: &str) -> BoxFuture<'static, Result<BTreeMap<String, i32>, String>> {
		let (iwd, station) = (self.clone(), station.to_owned());
		Box::pin(async move { iwd.scan_on(&station).await })
	}

	fn push_button(&self, station: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>> {
		let (iwd, station) = (self.clone(), station.to_owned());
		Box::pin(async move { iwd.wps(&station, "PushButton", None).await })
	}

	fn generate_pin(&self, station: &str) -> BoxFuture<'static, Result<String, String>> {
		let (iwd, station) = (self.clone(), station.to_owned());
		Box::pin(async move {
			let path = iwd.station(&station).await?;
			let (pin,): (String,) = iwd
				.proxy(path, CALL)
				.method_call(WPS, "GeneratePin", ())
				.await
				.map_err(failed)?;
			Ok(pin)
		})
	}

	fn start_pin(&self, station: &str, pin: &str) -> BoxFuture<'static, Result<Joined, WpsFailed>> {
		let (iwd, station, pin) = (self.clone(), station.to_owned(), pin.to_owned());
		Box::pin(async move { iwd.wps(&station, "StartPin", Some(pin)).await })
	}

	fn cancel_wps(&self, station: &str) -> BoxFuture<'static, ()> {
		let (iwd, station) = (self.clone(), station.to_owned());
		Box::pin(async move {
			if let Ok(path) = iwd.station(&station).await {
				let _ = iwd
					.proxy(path, CALL)
					.method_call::<(), _, _, _>(WPS, "Cancel", ())
					.await;
			}
		})
	}

	fn passphrase(&self, ssid: &str) -> BoxFuture<'static, Result<String, String>> {
		let file: PathBuf = render::known_psk(&self.paths, ssid);
		Box::pin(async move {
			let contents = std::fs::read_to_string(&file).map_err(|error| {
				format!("iwd left nothing readable at {}: {error}", file.display())
			})?;
			render::known_passphrase(&contents).ok_or_else(|| {
				"the access point handed over a raw key rather than a passphrase, which a candidate \
				 cannot carry"
					.to_owned()
			})
		})
	}
}
