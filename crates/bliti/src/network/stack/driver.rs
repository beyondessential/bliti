//! The task driving the running system: the selector, fed by observations, commands and its own
//! timers, and each decision rendered and applied as it changes.
//!
//! Applying runs on the blocking pool while the task goes on taking observations. What an apply
//! brings up is not asked anything until that apply is in place: a wireless candidate is joined, and
//! a wired one's addressing deadline started, once the files it needs are written.
//!
//! A proposal is answered once nothing brought up is still being verified and every scan it asked
//! for is in. It is applied where any candidate is established, which is what lets a document
//! carrying two sites' static candidates be applied at either site; with none established it fails
//! at the candidate that got furthest, the first of those in the ordering (CFG).
//!
//! Timers are deadlines on one attempt's stage, and retries of a failed candidate on an interface
//! left with nothing established, each backed off. Nothing is polled (LINK).

use std::{
	cmp::Reverse,
	collections::{BTreeMap, BTreeSet},
	sync::Arc,
	time::Duration,
};

use bliti_core::channel::config::{Attachment, AttachmentKind, Document, Invalid, Segment, path};
use serde_json::Value as Json;
use tokio::sync::{mpsc, oneshot, watch};

use super::{
	Shared,
	verify::{Check, Links},
};
use crate::network::{
	apply::{self, Changes, System},
	observe::{Joined, Observation, Station, render_channel},
	probe::RadioInfo,
	render,
	select::{Attempt, Change, Event, Link, Selector, Stage, State},
	session::entry,
};

/// How long a wireless candidate may take to associate.
pub(super) const ASSOCIATE: Duration = Duration::from_secs(90);

/// How long a dynamic candidate may take to hold an address once it has carrier or has associated.
/// networkd's DHCP client retries within this.
pub(super) const LEASE: Duration = Duration::from_secs(45);

/// How long a static candidate may take to hold its configured address.
pub(super) const CONFIGURE: Duration = Duration::from_secs(10);

/// How long a dynamic candidate holding an address may take to be offered a default route.
pub(super) const ROUTE: Duration = Duration::from_secs(10);

/// The first wait before a failed candidate is tried again, doubled each time it fails again.
pub(super) const RETRY: Duration = Duration::from_secs(30);

/// The longest wait before a failed candidate is tried again.
pub(super) const RETRY_MAX: Duration = Duration::from_secs(15 * 60);

/// What the backend asks of the task.
pub(super) enum Command {
	/// Configure a proposal and answer once it is verified.
	Apply {
		document: Document,
		reply: oneshot::Sender<Result<(), Invalid>>,
	},
	/// Configure the recorded document and answer once it is applied.
	Restore {
		document: Document,
		reply: oneshot::Sender<anyhow::Result<()>>,
	},
	/// Leave a radio's wireless client alone while WPS runs on it, or take it back.
	Hold {
		station: String,
		held: bool,
		reply: oneshot::Sender<()>,
	},
}

/// What the task hears back from what it started.
enum Internal {
	Applied {
		system: Box<dyn System + Send>,
		result: Result<Changes, apply::Error>,
		/// The render this applied.
		taken: u64,
		/// The attempts it brought up.
		attempts: Vec<Attempt>,
		/// The hotspot it brought up, by SSID.
		hotspot: Option<String>,
	},
	Reprobed(anyhow::Result<Vec<RadioInfo>>),
	Scanned {
		interface: String,
		result: Result<BTreeMap<String, i32>, String>,
	},
	Associated {
		attempt: Attempt,
		result: Result<Joined, String>,
	},
	Deadline {
		attempt: Attempt,
		timer: u64,
	},
	Probed {
		attempt: Attempt,
		result: Result<(), String>,
	},
	Retry {
		candidate: usize,
		generation: u64,
	},
}

/// A command waiting on its answer, and the render it waits to see applied.
enum Pending {
	Apply(oneshot::Sender<Result<(), Invalid>>),
	Restore(oneshot::Sender<anyhow::Result<()>>),
}

pub(super) struct Driver {
	shared: Arc<Shared>,
	selector: Selector,
	document: Document,
	/// Whether a document has been configured, before which nothing is rendered.
	configured: bool,
	/// Taken while an apply runs.
	system: Option<Box<dyn System + Send>>,
	states: watch::Sender<Option<Vec<Json>>>,
	internal: mpsc::UnboundedSender<Internal>,
	inbox: Option<mpsc::UnboundedReceiver<Internal>>,
	/// Bumped on every configure, so a retry scheduled for another document is recognised.
	generation: u64,
	/// Renders asked for, the last started, and the last applied, as counts.
	wanted: u64,
	taken: u64,
	done: u64,
	reprobing: bool,
	/// Scans a pending proposal waits on.
	scanning: usize,
	pending: Option<(Pending, u64)>,
	links: Links,
	heard: BTreeMap<String, BTreeMap<String, i32>>,
	stations: BTreeMap<String, Station>,
	checks: BTreeMap<Attempt, Check>,
	/// The candidate each interface last brought up.
	last: BTreeMap<String, Attachment>,
	/// Stations WPS is running on.
	held: BTreeSet<String>,
	/// How many times in a row each candidate has been retried.
	backoff: BTreeMap<usize, u32>,
	/// Candidates that failed and are to be tried again once nothing is pending.
	failed: BTreeSet<usize>,
}

impl Driver {
	pub(super) fn new(
		shared: Arc<Shared>,
		system: Box<dyn System + Send>,
		states: watch::Sender<Option<Vec<Json>>>,
	) -> Self {
		let document = Document {
			attachments: Vec::new(),
			hotspot: None,
			regulatory_domain: None,
		};
		let selector = Selector::new(shared.select.clone(), document.clone())
			.expect("an empty document is carried out on any hardware");
		let (internal, inbox) = mpsc::unbounded_channel();
		Self {
			shared,
			selector,
			document,
			configured: false,
			system: Some(system),
			states,
			internal,
			inbox: Some(inbox),
			generation: 0,
			wanted: 0,
			taken: 0,
			done: 0,
			reprobing: false,
			scanning: 0,
			pending: None,
			links: Links::default(),
			heard: BTreeMap::new(),
			stations: BTreeMap::new(),
			checks: BTreeMap::new(),
			last: BTreeMap::new(),
			held: BTreeSet::new(),
			backoff: BTreeMap::new(),
			failed: BTreeSet::new(),
		}
	}

	pub(super) async fn run(
		mut self,
		mut commands: mpsc::UnboundedReceiver<Command>,
		mut observations: mpsc::UnboundedReceiver<Observation>,
	) {
		let mut inbox = self.inbox.take().expect("taken once");
		let mut observing = true;
		loop {
			tokio::select! {
				command = commands.recv() => match command {
					Some(command) => self.command(command),
					None => return,
				},
				observation = observations.recv(), if observing => match observation {
					Some(observation) => self.observe(observation),
					None => {
						tracing::error!("the network observers stopped; nothing more will be observed");
						observing = false;
					}
				},
				Some(internal) = inbox.recv() => self.internal(internal),
			}
			self.kick();
			self.settle();
		}
	}

	fn command(&mut self, command: Command) {
		match command {
			Command::Apply { document, reply } => match self.configure(document) {
				Ok(()) => {
					self.pending = Some((Pending::Apply(reply), self.wanted));
					self.scan_for_pending();
				}
				Err(invalid) => {
					let _ = reply.send(Err(invalid));
				}
			},
			Command::Restore { document, reply } => match self.configure(document) {
				Ok(()) => self.pending = Some((Pending::Restore(reply), self.wanted)),
				Err(invalid) => {
					let _ = reply.send(Err(anyhow::anyhow!(
						"{} (at {})",
						invalid.reason,
						invalid.at
					)));
				}
			},
			Command::Hold {
				station,
				held,
				reply,
			} => {
				if held {
					self.held.insert(station);
				} else {
					self.held.remove(&station);
				}
				let _ = reply.send(());
			}
		}
	}

	/// Select among a document's candidates, keeping what has been observed.
	fn configure(&mut self, document: Document) -> Result<(), Invalid> {
		let changes = self.selector.configure(document.clone())?.changes;
		self.document = document;
		self.configured = true;
		self.generation += 1;
		self.backoff.clear();
		self.failed.clear();
		// Rendered whether or not anything changed, so the running system is made to match.
		self.wanted += 1;

		// Candidates carried over keep their attempts but may have moved in the ordering.
		let links = self.selector.decision().links.clone();
		self.checks
			.retain(|attempt, _| links.values().any(|link| link.attempt == *attempt));
		for link in links.values() {
			if let Some(check) = self.checks.get_mut(&link.attempt) {
				check.candidate = link.candidate;
			}
		}

		self.follow(changes);
		self.publish();
		Ok(())
	}

	fn scan_for_pending(&mut self) {
		let wireless = self
			.document
			.attachments
			.iter()
			.any(|attachment| matches!(attachment.kind, AttachmentKind::Wireless(_)));
		if !wireless {
			return;
		}
		for radio in self.shared.radios() {
			if !radio.scan || self.held.contains(&radio.station) {
				continue;
			}
			self.scanning += 1;
			let iwd = self.shared.platform.iwd.clone();
			let internal = self.internal.clone();
			tokio::spawn(async move {
				let result = iwd.scan(&radio.station).await;
				let _ = internal.send(Internal::Scanned {
					interface: radio.station,
					result,
				});
			});
		}
	}

	fn observe(&mut self, observation: Observation) {
		match observation {
			Observation::Carrier { interface, up } => self.feed(Event::Carrier { interface, up }),
			Observation::Address { .. } | Observation::Route { .. } => {
				let Some(interface) = self.links.observe(&observation) else {
					return;
				};
				let attempts: Vec<Attempt> = self
					.checks
					.values_mut()
					.filter(|check| check.interface == interface)
					.map(|check| {
						check.announced(&observation);
						check.attempt
					})
					.collect();
				for attempt in attempts {
					self.evaluate(attempt);
				}
			}
			Observation::Heard {
				interface,
				networks,
			} => self.heard(interface, networks),
			Observation::Station { interface, station } => self.station(interface, station),
		}
	}

	/// What a radio hears now, as the selector's in-range and out-of-range events.
	fn heard(&mut self, interface: String, networks: BTreeMap<String, i32>) {
		let before = self.heard.insert(interface.clone(), networks.clone());
		let before = before.unwrap_or_default();
		for ssid in before.keys().filter(|ssid| !networks.contains_key(*ssid)) {
			self.feed(Event::OutOfRange {
				interface: interface.clone(),
				ssid: ssid.clone(),
			});
		}
		for (ssid, signal) in networks {
			if before.get(&ssid) != Some(&signal) {
				self.feed(Event::InRange {
					interface: interface.clone(),
					ssid,
					signal,
				});
			}
		}
	}

	fn station(&mut self, interface: String, station: Station) {
		self.stations.insert(interface.clone(), station.clone());
		match &station {
			Station::Connected(joined) => {
				self.shared.report.station(&interface, Some(joined.clone()))
			}
			Station::Disconnected => self.shared.report.station(&interface, None),
			Station::Busy => {}
		}
		if self.held.contains(&interface) {
			return;
		}
		// The attempt on this radio that has associated; one still associating is decided by its
		// connect call, since iwd passes through several states on the way.
		let Some(check) = self.checks.values().find(|check| {
			check.interface == interface
				&& check.armed
				&& check.joining.is_some()
				&& check.at.is_none_or(|at| at > Stage::Association)
		}) else {
			return;
		};
		let attempt = check.attempt;
		let ssid = check
			.joining
			.as_ref()
			.map(|joining| joining.target.ssid.clone())
			.unwrap_or_default();
		match station {
			Station::Connected(joined) if joined.ssid == ssid => self.feed(Event::StationChannel {
				interface,
				channel: joined.frequency.and_then(render_channel),
			}),
			Station::Connected(_) | Station::Busy => {}
			Station::Disconnected => self.feed(Event::Failed {
				attempt,
				stage: Stage::Association,
				reason: format!("{interface} was disassociated from {ssid:?}"),
			}),
		}
	}

	fn internal(&mut self, internal: Internal) {
		match internal {
			Internal::Applied {
				system,
				result,
				taken,
				attempts,
				hotspot,
			} => {
				self.system = Some(system);
				self.done = taken;
				match result {
					Ok(changes) => {
						self.shared.report.hotspot(hotspot);
						if !changes.regdom.is_empty() {
							self.reprobe();
						}
					}
					Err(error) => {
						tracing::error!(%error, "applying the network configuration failed");
						let at = self.at_fault(&error);
						self.failed(Stage::Carrier.failed(at, error.to_string()));
					}
				}
				for attempt in attempts {
					self.arm(attempt);
				}
			}
			Internal::Reprobed(result) => {
				self.reprobing = false;
				match result {
					Ok(radios) => self.shared.reprobed(radios),
					Err(error) => {
						tracing::warn!(%error, "could not probe the radios again after the regulatory domain changed");
					}
				}
			}
			Internal::Scanned { interface, result } => {
				self.scanning = self.scanning.saturating_sub(1);
				match result {
					Ok(networks) => self.heard(interface, networks),
					Err(reason) => tracing::warn!(interface, reason, "scanning failed"),
				}
			}
			Internal::Associated { attempt, result } => self.associated(attempt, result),
			Internal::Deadline { attempt, timer } => self.deadline_passed(attempt, timer),
			Internal::Probed { attempt, result } => self.probed(attempt, result),
			Internal::Retry {
				candidate,
				generation,
			} => {
				if generation == self.generation && self.worth_retrying(candidate) {
					tracing::info!(candidate, "trying a failed candidate again");
					self.feed(Event::Retry { candidate });
				}
			}
		}
	}

	/// Take in an event and follow what the selector changed.
	fn feed(&mut self, event: Event) {
		let changes = self.selector.handle(event).changes;
		self.follow(changes);
	}

	fn follow(&mut self, changes: Vec<Change>) {
		let mut states = false;
		for change in changes {
			match change {
				Change::Link { interface, link } => {
					self.wanted += 1;
					self.checks.retain(|attempt, check| {
						check.interface != interface
							|| link.is_some_and(|link| link.attempt == *attempt)
					});
					match link {
						Some(link) if !self.checks.contains_key(&link.attempt) => {
							self.begin(interface, link);
						}
						Some(_) => {}
						None if self.is_radio(&interface) && !self.held.contains(&interface) => {
							self.disconnect(interface);
						}
						None => {}
					}
				}
				Change::Hotspot(_) => self.wanted += 1,
				// Route metrics follow the ordering, so the default route needs nothing applied.
				Change::DefaultRoute(_) => {}
				Change::State { candidate, state } => {
					states = true;
					match state {
						State::DefaultRoute | State::Up => {
							self.backoff.remove(&candidate);
						}
						State::Unavailable { reached, .. } if reached != Stage::Carrier => {
							self.failed.insert(candidate);
						}
						_ => {}
					}
				}
			}
		}
		if states {
			self.publish();
		}
	}

	/// Start verifying an attempt, once the render bringing it up is applied.
	fn begin(&mut self, interface: String, link: Link) {
		let at = match self.selector.decision().states.get(link.candidate) {
			Some(State::Verifying { at }) => *at,
			_ => Stage::Addressing,
		};
		let attachment = &self.document.attachments[link.candidate];
		let stale = match self.last.insert(interface.clone(), attachment.clone()) {
			Some(previous) if previous != *attachment => self.links.held(&interface),
			_ => BTreeSet::new(),
		};
		let check = Check::new(
			link.attempt,
			link.candidate,
			attachment,
			interface,
			at,
			stale,
		);
		self.checks.insert(link.attempt, check);
	}

	fn arm(&mut self, attempt: Attempt) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		if check.armed {
			return;
		}
		check.armed = true;
		match check.at {
			Some(Stage::Association) => self.associate(attempt),
			Some(Stage::Addressing) => {
				self.address_deadline(attempt);
				self.evaluate(attempt);
			}
			_ => self.evaluate(attempt),
		}
	}

	fn associate(&mut self, attempt: Attempt) {
		let Some(check) = self.checks.get(&attempt) else {
			return;
		};
		let Some(joining) = check.joining.clone() else {
			return;
		};
		let station = check.interface.clone();
		if self.held.contains(&station) {
			return;
		}
		if let Some(Station::Connected(joined)) = self.stations.get(&station)
			&& joined.ssid == joining.target.ssid
		{
			let joined = joined.clone();
			self.associated(attempt, Ok(joined));
			return;
		}
		let iwd = self.shared.platform.iwd.clone();
		let internal = self.internal.clone();
		tokio::spawn(async move {
			let ssid = &joining.target.ssid;
			let result = tokio::time::timeout(ASSOCIATE, iwd.connect(&station, &joining.target))
				.await
				.unwrap_or_else(|_| {
					Err(format!(
						"{ssid:?} did not associate within {} seconds",
						ASSOCIATE.as_secs()
					))
				});
			let _ = internal.send(Internal::Associated { attempt, result });
		});
	}

	fn associated(&mut self, attempt: Attempt, result: Result<Joined, String>) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		if check.at != Some(Stage::Association) {
			return;
		}
		let Some(joining) = check.joining.clone() else {
			return;
		};
		let interface = check.interface.clone();
		match result {
			Ok(joined) if joining.sae && !joined.by_sae() => {
				let reason = format!(
					"{:?} was joined by {}, not by SAE",
					joining.target.ssid,
					joined
						.security
						.as_deref()
						.unwrap_or("a method iwd did not name")
				);
				self.disconnect(interface);
				self.feed(Event::Failed {
					attempt,
					stage: Stage::Association,
					reason,
				});
			}
			Ok(joined) => {
				check.at = Some(Stage::Addressing);
				self.shared.report.station(&interface, Some(joined.clone()));
				self.feed(Event::Passed {
					attempt,
					stage: Stage::Association,
				});
				self.feed(Event::StationChannel {
					interface,
					channel: joined.frequency.and_then(render_channel),
				});
				self.address_deadline(attempt);
				self.evaluate(attempt);
			}
			Err(reason) => self.feed(Event::Failed {
				attempt,
				stage: Stage::Association,
				reason,
			}),
		}
	}

	/// Move an attempt on as far as what is observed of its link takes it.
	fn evaluate(&mut self, attempt: Attempt) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		if !check.armed {
			return;
		}
		let interface = check.interface.clone();
		match check.at {
			Some(Stage::Addressing) => {
				if check.held(&self.links).is_empty() {
					return;
				}
				check.at = Some(Stage::Gateway);
				check.timer += 1;
				self.feed(Event::Passed {
					attempt,
					stage: Stage::Addressing,
				});
				self.deadline(attempt, ROUTE);
				self.evaluate(attempt);
			}
			Some(Stage::Gateway) => {
				if check.probing {
					return;
				}
				let Some((source, gateway)) = check.gateway(&self.links) else {
					return;
				};
				check.probing = true;
				check.timer += 1;
				let probe = self.shared.platform.gateway.clone();
				let internal = self.internal.clone();
				tokio::spawn(async move {
					let result = probe.probe(&interface, source, gateway).await;
					let _ = internal.send(Internal::Probed { attempt, result });
				});
			}
			None => {
				if check.held(&self.links).is_empty() {
					self.feed(Event::Failed {
						attempt,
						stage: Stage::Addressing,
						reason: format!("{interface} lost its address"),
					});
				}
			}
			Some(Stage::Carrier | Stage::Association) => {}
		}
	}

	fn probed(&mut self, attempt: Attempt, result: Result<(), String>) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		check.probing = false;
		if check.at != Some(Stage::Gateway) {
			return;
		}
		match result {
			Ok(()) => {
				check.at = None;
				self.feed(Event::Passed {
					attempt,
					stage: Stage::Gateway,
				});
			}
			Err(reason) => self.feed(Event::Failed {
				attempt,
				stage: Stage::Gateway,
				reason,
			}),
		}
	}

	fn address_deadline(&mut self, attempt: Attempt) {
		let Some(check) = self.checks.get(&attempt) else {
			return;
		};
		let after = match check.addressing {
			super::verify::Addressing::Dynamic => LEASE,
			super::verify::Addressing::Static { .. } => CONFIGURE,
		};
		self.deadline(attempt, after);
	}

	/// Have the attempt's current stage fail unless it passes within `after`.
	fn deadline(&mut self, attempt: Attempt, after: Duration) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		check.timer += 1;
		let timer = check.timer;
		let internal = self.internal.clone();
		tokio::spawn(async move {
			tokio::time::sleep(after).await;
			let _ = internal.send(Internal::Deadline { attempt, timer });
		});
	}

	fn deadline_passed(&mut self, attempt: Attempt, timer: u64) {
		let Some(check) = self.checks.get(&attempt) else {
			return;
		};
		if check.timer != timer || check.probing {
			return;
		}
		let interface = &check.interface;
		let (stage, reason) = match check.at {
			Some(Stage::Addressing) => (
				Stage::Addressing,
				match check.addressing {
					super::verify::Addressing::Dynamic => format!(
						"{interface} was leased no address within {} seconds",
						LEASE.as_secs()
					),
					super::verify::Addressing::Static { .. } => format!(
						"{interface} did not take its address within {} seconds",
						CONFIGURE.as_secs()
					),
				},
			),
			Some(Stage::Gateway) => (
				Stage::Gateway,
				format!("the network on {interface} offered no gateway"),
			),
			_ => return,
		};
		self.feed(Event::Failed {
			attempt,
			stage,
			reason,
		});
	}

	fn disconnect(&self, station: String) {
		let iwd = self.shared.platform.iwd.clone();
		tokio::spawn(async move {
			if let Err(reason) = iwd.disconnect(&station).await {
				tracing::warn!(station, reason, "could not disconnect");
			}
		});
	}

	fn is_radio(&self, interface: &str) -> bool {
		self.shared
			.select
			.radios
			.iter()
			.any(|radio| radio.station == interface)
	}

	/// Schedule trying a failed candidate again, backing off each time it fails again.
	fn retry_later(&mut self, candidate: usize) {
		let tries = self.backoff.entry(candidate).or_default();
		let wait = RETRY.saturating_mul(1 << (*tries).min(16)).min(RETRY_MAX);
		*tries += 1;
		let generation = self.generation;
		let internal = self.internal.clone();
		tokio::spawn(async move {
			tokio::time::sleep(wait).await;
			let _ = internal.send(Internal::Retry {
				candidate,
				generation,
			});
		});
	}

	/// Whether trying `candidate` again could bring something up where nothing is: a candidate
	/// whose every interface carries an established one would only take it down to try (LINK).
	fn worth_retrying(&self, candidate: usize) -> bool {
		let decision = self.selector.decision();
		let established = |interface: &str| {
			decision.links.get(interface).is_some_and(|link| {
				matches!(
					decision.states.get(link.candidate),
					Some(State::Up | State::DefaultRoute)
				)
			})
		};
		match self.document.attachments.get(candidate).map(|a| &a.kind) {
			Some(
				AttachmentKind::WiredDynamic { interface }
				| AttachmentKind::WiredStatic { interface, .. },
			) => !established(interface),
			Some(AttachmentKind::Wireless(wireless)) => self
				.shared
				.select
				.radios
				.iter()
				.filter(|radio| {
					wireless
						.interface
						.as_ref()
						.is_none_or(|pin| *pin == radio.station)
				})
				.any(|radio| !established(&radio.station)),
			None => false,
		}
	}

	fn reprobe(&mut self) {
		self.reprobing = true;
		let air = self.shared.platform.air.clone();
		let internal = self.internal.clone();
		tokio::spawn(async move {
			let _ = internal.send(Internal::Reprobed(air.radios().await));
		});
	}

	/// Render and apply the decision, where it has changed and no apply is running.
	fn kick(&mut self) {
		if !self.configured || self.taken >= self.wanted || self.system.is_none() {
			return;
		}
		self.taken = self.wanted;
		let selection = self.selector.selection().unwrap_or_default();
		let rendered = match render::render(&self.document, &self.shared.render, &selection) {
			Ok(rendered) => rendered,
			Err(error) => {
				self.done = self.taken;
				tracing::error!(%error, "the network configuration does not render");
				self.failed(match error {
					render::Error::Invalid(invalid) => invalid,
					render::Error::Selection(reason) => Invalid {
						at: path(&[]),
						reason,
						reached: None,
					},
				});
				return;
			}
		};
		let Some(mut system) = self.system.take() else {
			return;
		};
		let taken = self.taken;
		let attempts = self
			.selector
			.decision()
			.links
			.values()
			.map(|link| link.attempt)
			.collect();
		let hardware = self.shared.render.clone();
		let hotspot = rendered
			.files
			.iter()
			.any(|file| file.path == hardware.paths.hostapd)
			.then(|| {
				self.document
					.hotspot
					.as_ref()
					.map(|hotspot| hotspot.ssid.clone())
			})
			.flatten();
		let state = self.shared.config.state.clone();
		let internal = self.internal.clone();
		tokio::task::spawn_blocking(move || {
			let result = apply::apply(&rendered, &hardware, &state, &mut *system);
			let _ = internal.send(Internal::Applied {
				system,
				result,
				taken,
				attempts,
				hotspot,
			});
		});
	}

	/// Answer the command waiting, where what it waits on has come about, and once none is, schedule
	/// trying again what failed.
	fn settle(&mut self) {
		self.answer();
		if self.pending.is_none() {
			for candidate in std::mem::take(&mut self.failed) {
				self.retry_later(candidate);
			}
		}
	}

	fn answer(&mut self) {
		let Some((_, needs)) = &self.pending else {
			return;
		};
		if self.done < *needs || self.reprobing {
			return;
		}
		if matches!(self.pending, Some((Pending::Apply(_), _))) {
			let verifying = self
				.selector
				.decision()
				.states
				.iter()
				.any(|state| matches!(state, State::Verifying { .. }));
			let applying = self.system.is_none() || self.taken < self.wanted;
			if verifying || applying || self.scanning > 0 {
				return;
			}
		}
		match self.pending.take() {
			Some((Pending::Apply(reply), _)) => {
				let answer = verdict(&self.selector.decision().states);
				match &answer {
					Ok(()) => tracing::info!("proposal verified"),
					Err(invalid) => {
						tracing::info!(at = invalid.at, reason = invalid.reason, reached = ?invalid.reached, "proposal failed")
					}
				}
				let _ = reply.send(answer);
			}
			Some((Pending::Restore(reply), _)) => {
				let _ = reply.send(Ok(()));
			}
			None => {}
		}
	}

	/// Fail the command waiting, where one is.
	fn failed(&mut self, invalid: Invalid) {
		match self.pending.take() {
			Some((Pending::Apply(reply), _)) => {
				let _ = reply.send(Err(invalid));
			}
			Some((Pending::Restore(reply), _)) => {
				let _ = reply.send(Err(anyhow::anyhow!(invalid.reason)));
			}
			None => {}
		}
	}

	/// The part of the document an apply failure is in.
	fn at_fault(&self, error: &apply::Error) -> String {
		let backend = match error {
			apply::Error::File { backend, .. } | apply::Error::System { backend, .. } => *backend,
			apply::Error::Foreign(_) | apply::Error::Record { .. } => return path(&[]),
		};
		match backend {
			apply::Backend::Hostapd if self.document.hotspot.is_some() => {
				path(&[Segment::Name("hotspot")])
			}
			apply::Backend::Regdom if self.document.regulatory_domain.is_some() => {
				path(&[Segment::Name("regulatory-domain")])
			}
			_ => path(&[]),
		}
	}

	fn publish(&self) {
		let entries = self.selector.decision().states.iter().map(entry).collect();
		self.states.send_replace(Some(entries));
	}
}

/// What a proposal comes to once nothing brought up is being verified: applied where a candidate is
/// established or there are none, else failed at the candidate that got furthest.
pub(super) fn verdict(states: &[State]) -> Result<(), Invalid> {
	if states.is_empty()
		|| states
			.iter()
			.any(|state| matches!(state, State::Up | State::DefaultRoute))
	{
		return Ok(());
	}
	let furthest = states
		.iter()
		.enumerate()
		.filter_map(|(candidate, state)| match state {
			State::Unavailable { reached, reason } => Some((candidate, *reached, reason)),
			_ => None,
		})
		.max_by_key(|(candidate, reached, _)| (*reached, Reverse(*candidate)));
	match furthest {
		Some((candidate, reached, reason)) => Err(reached.failed(
			path(&[Segment::Name("attachments"), Segment::Index(candidate)]),
			reason.clone(),
		)),
		None => Err(Stage::Carrier.failed(
			path(&[Segment::Name("attachments")]),
			"no candidate could be brought up",
		)),
	}
}
