//! The backend that configures the network: the selector, renderer and applier joined, driven by
//! what the observers see.
//!
//! [`Stack`] is what a configuration session holds. Everything that changes the running system goes
//! through one [`driver`] task, which owns the selector and the applier's [`System`] and is fed
//! observations, commands and its own timers. A proposal is configured on the selector, rendered for
//! the selector's decision, applied on the blocking pool, and then verified from what is observed
//! until no candidate brought up is still being verified. Every change the selector makes after that
//! is rendered and applied again the same way, with or without a session open (LINK).
//!
//! The renderer drives one radio, so a device with several is run on its first, and states
//! capabilities for that one alone: offering a radio nothing here can drive would be offering a
//! setting the device cannot carry out (NET).

use std::{
	collections::BTreeMap,
	path::PathBuf,
	sync::{Arc, Mutex, PoisonError},
};

use bliti_core::channel::config::{Document, Invalid, path};
use serde_json::{Map, Value as Json};
use tokio::sync::{mpsc, oneshot, watch};

use super::{
	apply::System,
	observe::{Air, Gateway, Iwd, Observation},
	probe::{self, RadioInfo},
	render, select,
	session::{Backend, Inert},
};

pub use self::report::Report;

mod acts;
mod driver;
mod report;
#[cfg(test)]
mod tests;
mod verify;

/// What the backend has the system do beyond applying files.
#[derive(Clone)]
pub struct Platform {
	/// iwd, for joining, scanning and WPS.
	pub iwd: Arc<dyn Iwd>,
	/// The radios, over nl80211.
	pub air: Arc<dyn Air>,
	/// The gateway probe.
	pub gateway: Arc<dyn Gateway>,
}

/// What the backend runs on.
#[derive(Debug, Clone)]
pub struct Config {
	/// The wired interfaces.
	pub wired: Vec<String>,
	/// Where each backend reads the files bliti renders.
	pub paths: render::Paths,
	/// Where the applier keeps its record of what it put in place.
	pub state: PathBuf,
	/// The access point interface bliti creates for the hotspot, as the iwd drop-in names it.
	pub access_point: String,
}

/// The network backend: a handle on the task driving the running system.
pub struct Stack {
	shared: Arc<Shared>,
	commands: mpsc::UnboundedSender<driver::Command>,
	states: watch::Receiver<Option<Vec<Json>>>,
}

/// What the backend and its task both read.
struct Shared {
	config: Config,
	platform: Platform,
	/// What selection runs on.
	select: select::Hardware,
	/// What rendering runs on.
	render: render::Hardware,
	/// The radio driven, as last probed, and the capabilities stated from it.
	probed: Mutex<Probed>,
	/// What is joined and run, for NFO.
	report: Report,
}

struct Probed {
	radios: Vec<RadioInfo>,
	capabilities: Map<String, Json>,
}

impl Shared {
	fn probed(&self) -> std::sync::MutexGuard<'_, Probed> {
		self.probed.lock().unwrap_or_else(PoisonError::into_inner)
	}

	/// Take in what the radios can do, as probed now.
	fn reprobed(&self, radios: Vec<RadioInfo>) {
		let radios = driven(radios);
		let capabilities =
			probe::capabilities(&radios, &self.config.wired, &probe::Backend::stack());
		*self.probed() = Probed {
			radios,
			capabilities,
		};
	}

	fn radios(&self) -> Vec<RadioInfo> {
		self.probed().radios.clone()
	}

	/// Whether each radio surveys, as first probed. It does not change with the regulatory domain,
	/// so a probe again need not ask.
	fn surveyed(&self) -> BTreeMap<String, bool> {
		self.probed()
			.radios
			.iter()
			.map(|radio| (radio.station.clone(), radio.survey))
			.collect()
	}
}

/// The radios the renderer drives, of those probed: the first.
fn driven(mut radios: Vec<RadioInfo>) -> Vec<RadioInfo> {
	if radios.len() > 1 {
		let ignored: Vec<&str> = radios[1..]
			.iter()
			.map(|radio| radio.station.as_str())
			.collect();
		tracing::warn!(
			driven = radios[0].station,
			?ignored,
			"more than one radio, and the renderer drives one; the rest are left alone"
		);
		radios.truncate(1);
	}
	radios
}

impl Stack {
	/// Probe the radios and start the task driving the running system, fed by `observations`.
	///
	/// Nothing is applied until the session restores the recorded configuration.
	pub async fn start(
		config: Config,
		platform: Platform,
		system: Box<dyn System + Send>,
		observations: mpsc::UnboundedReceiver<Observation>,
	) -> anyhow::Result<Self> {
		let radios = driven(platform.air.radios(BTreeMap::new()).await?);
		let radio = radios.first();
		let select = probe::select_hardware(&radios, &config.wired);
		let render = probe::render_hardware(
			radio,
			&config.wired,
			&config.access_point,
			config.paths.clone(),
		);
		let capabilities = probe::capabilities(&radios, &config.wired, &probe::Backend::stack());
		let report = Report::new(platform.air.clone(), config.access_point.clone());
		let shared = Arc::new(Shared {
			config,
			platform,
			select,
			render,
			probed: Mutex::new(Probed {
				radios,
				capabilities,
			}),
			report,
		});
		let (states_tx, states) = watch::channel(None);
		let (commands, receiver) = mpsc::unbounded_channel();
		let driver = driver::Driver::new(shared.clone(), system, states_tx);
		tokio::spawn(driver.run(receiver, observations));
		Ok(Self {
			shared,
			commands,
			states,
		})
	}

	/// Start on the device's own stack: nl80211, rtnetlink, iwd, and systemd and networkd over D-Bus.
	#[cfg(target_os = "linux")]
	pub async fn linux(wired: Vec<String>) -> anyhow::Result<Self> {
		use anyhow::Context as _;

		let (platform, observations) = super::observe::linux(render::Paths::system())
			.await
			.context("watching the network")?;
		let system = tokio::task::spawn_blocking(super::apply::Linux::new)
			.await?
			.context("connecting the applier to the system bus")?;
		let config = Config {
			wired,
			paths: render::Paths::system(),
			state: super::apply::STATE.into(),
			access_point: ACCESS_POINT.into(),
		};
		Self::start(config, platform, Box::new(system), observations).await
	}

	/// What the backend knows of the wireless networks joined and the hotspot run, for NFO.
	pub fn report(&self) -> Report {
		self.shared.report.clone()
	}

	/// Send the task a command and wait for its answer.
	async fn ask<T>(
		&self,
		command: impl FnOnce(oneshot::Sender<T>) -> driver::Command,
	) -> Option<T> {
		let (reply, answer) = oneshot::channel();
		self.commands.send(command(reply)).ok()?;
		answer.await.ok()
	}
}

/// The access point interface bliti creates beside the station, as `services/bliti-iwd-dropin.conf`
/// keeps iwd off it.
#[cfg(target_os = "linux")]
const ACCESS_POINT: &str = "ap0";

/// Why a request to the task went unanswered.
const STOPPED: &str = "the network backend has stopped";

impl Backend for Stack {
	fn capabilities(&self) -> Map<String, Json> {
		self.shared.probed().capabilities.clone()
	}

	fn check(&self, document: &Document) -> Result<(), Invalid> {
		select::check(document, &self.shared.select)?;
		match render::render(document, &self.shared.render, &render::Selection::default()) {
			Err(render::Error::Invalid(invalid)) => Err(invalid),
			Err(render::Error::Selection(_)) | Ok(_) => Ok(()),
		}
	}

	fn states(&self) -> watch::Receiver<Option<Vec<Json>>> {
		self.states.clone()
	}

	async fn apply(&mut self, document: &Document) -> Result<(), Invalid> {
		let document = document.clone();
		self.ask(|reply| driver::Command::Apply { document, reply })
			.await
			.unwrap_or_else(|| {
				Err(Invalid {
					at: path(&[]),
					reason: STOPPED.to_owned(),
					reached: None,
				})
			})
	}

	async fn restore(&mut self, document: &Document) -> anyhow::Result<()> {
		let document = document.clone();
		self.ask(|reply| driver::Command::Restore { document, reply })
			.await
			.unwrap_or_else(|| Err(anyhow::anyhow!(STOPPED)))
	}

	async fn scan(&mut self, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		acts::scan(&self.shared, interface).await
	}

	async fn survey(
		&mut self,
		interface: Option<&str>,
	) -> Result<Option<Map<String, Json>>, Invalid> {
		acts::survey(&self.shared, interface).await
	}

	async fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		base: &Map<String, Json>,
		pin: oneshot::Sender<String>,
	) -> Result<Map<String, Json>, Invalid> {
		acts::wps(self, method, interface, base, pin).await
	}
}

/// The backend the daemon was started with.
pub enum Chosen {
	/// Configuring nothing.
	Inert(Inert),
	/// Configuring the network.
	Stack(Box<Stack>),
}

impl Chosen {
	/// What the backend reports for NFO, where it configures anything to report on.
	pub fn report(&self) -> Option<Report> {
		match self {
			Self::Inert(_) => None,
			Self::Stack(stack) => Some(stack.report()),
		}
	}
}

impl Backend for Chosen {
	fn capabilities(&self) -> Map<String, Json> {
		match self {
			Self::Inert(backend) => backend.capabilities(),
			Self::Stack(backend) => backend.capabilities(),
		}
	}

	fn check(&self, document: &Document) -> Result<(), Invalid> {
		match self {
			Self::Inert(backend) => backend.check(document),
			Self::Stack(backend) => backend.check(document),
		}
	}

	fn states(&self) -> watch::Receiver<Option<Vec<Json>>> {
		match self {
			Self::Inert(backend) => backend.states(),
			Self::Stack(backend) => backend.states(),
		}
	}

	async fn apply(&mut self, document: &Document) -> Result<(), Invalid> {
		match self {
			Self::Inert(backend) => backend.apply(document).await,
			Self::Stack(backend) => backend.apply(document).await,
		}
	}

	async fn restore(&mut self, document: &Document) -> anyhow::Result<()> {
		match self {
			Self::Inert(backend) => backend.restore(document).await,
			Self::Stack(backend) => backend.restore(document).await,
		}
	}

	async fn scan(&mut self, interface: Option<&str>) -> Result<Vec<Json>, Invalid> {
		match self {
			Self::Inert(backend) => backend.scan(interface).await,
			Self::Stack(backend) => backend.scan(interface).await,
		}
	}

	async fn survey(
		&mut self,
		interface: Option<&str>,
	) -> Result<Option<Map<String, Json>>, Invalid> {
		match self {
			Self::Inert(backend) => backend.survey(interface).await,
			Self::Stack(backend) => backend.survey(interface).await,
		}
	}

	async fn wps(
		&mut self,
		method: &str,
		interface: Option<&str>,
		base: &Map<String, Json>,
		pin: oneshot::Sender<String>,
	) -> Result<Map<String, Json>, Invalid> {
		match self {
			Self::Inert(backend) => backend.wps(method, interface, base, pin).await,
			Self::Stack(backend) => backend.wps(method, interface, base, pin).await,
		}
	}
}
