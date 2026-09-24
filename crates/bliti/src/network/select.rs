//! Candidate selection and verification (LINK): which candidates are up, on which interfaces.
//!
//! Pure and event-driven. A [`Selector`] is fed the [`Event`]s a caller observes on the running
//! system and answers each with the [`Decision`] now in force and the [`Change`]s from the last one,
//! which are what the caller (re)applies. It holds no timers and polls nothing (LINK): a failed
//! candidate is tried again when it becomes available again, or when the caller sends
//! [`Event::Retry`], which is where a caller wanting a backoff schedules one.
//!
//! Each candidate brought up on an interface is an [`Attempt`]. The caller verifies it through the
//! [`Stage`]s and reports back against the attempt, so a report from an attempt already superseded,
//! by carrier dropping and returning say, is recognised as stale and ignored.

use std::collections::{BTreeMap, BTreeSet};

use bliti_core::channel::config::{
	Attachment, AttachmentKind, Document, Hotspot, Invalid, Segment,
};

use super::render::{Channel, Selection};

pub use self::check::check;

mod check;
mod place;
#[cfg(test)]
mod tests;

/// What selection runs on: the interfaces a candidate may be brought up on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardware {
	/// The wired interfaces a candidate may name.
	pub wired: Vec<String>,
	/// The wireless radios, in the order a tie between them is broken.
	pub radios: Vec<Radio>,
}

/// One wireless radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Radio {
	/// The interface its wireless client runs on, which is how a document names the radio (WLAN,
	/// HOT).
	pub station: String,
	/// How it runs an access point beside a wireless client, or `None` where it cannot run one.
	pub access_point: Option<Alongside>,
}

/// How a radio runs an access point beside a wireless client (HOT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alongside {
	/// Both at once, each on its own channel.
	Independent,
	/// Both at once, on the one channel the client is associated on.
	SharedChannel,
	/// Only one of the two at a time.
	OneAtATime,
}

/// A verification stage of LINK, in the order a candidate passes them.
///
/// Carrier is observed rather than reported: a wired candidate has it when its interface does, and
/// a wireless one when its radio hears the network. A candidate is tried only once it has carrier, so
/// an attempt starts at association (wireless) or addressing (wired).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
	/// The interface has carrier.
	Carrier,
	/// A wireless interface has associated.
	Association,
	/// An address is held.
	Addressing,
	/// The gateway answers.
	Gateway,
}

impl Stage {
	/// The stage as `reached` carries it on the wire.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::Carrier => "carrier",
			Self::Association => "association",
			Self::Addressing => "addressing",
			Self::Gateway => "gateway",
		}
	}

	/// A candidate's verification that stopped at this stage, at the part of the document named by `at`,
	/// a Normalized Path made with [`bliti_core::channel::config::path`].
	pub fn failed(self, at: impl Into<String>, reason: impl Into<String>) -> Invalid {
		Invalid {
			at: at.into(),
			reason: reason.into(),
			reached: Some(self.as_str().to_owned()),
		}
	}
}

/// One candidate brought up on one interface, which the caller verifies and reports back against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attempt(u64);

#[cfg(test)]
impl Attempt {
	/// An attempt made up by a test that needs one without a selector.
	pub fn test(n: u64) -> Self {
		Self(n)
	}
}

/// Something observed on the running system. Each is a reason to select again (LINK).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
	/// A wired interface gained or lost carrier. Ignored for an interface that is not wired.
	Carrier {
		/// The interface.
		interface: String,
		/// Whether it has carrier now.
		up: bool,
	},
	/// A radio hears a network, or hears it at a new signal.
	InRange {
		/// The radio's station interface.
		interface: String,
		/// The network.
		ssid: String,
		/// How well it is heard, in dBm.
		signal: i32,
	},
	/// A radio no longer hears a network.
	OutOfRange {
		/// The radio's station interface.
		interface: String,
		/// The network.
		ssid: String,
	},
	/// An attempt passed the stage it was at.
	Passed {
		/// The attempt.
		attempt: Attempt,
		/// The stage passed.
		stage: Stage,
	},
	/// An attempt failed at a stage: one being verified that did not get past it, or one established
	/// that stopped verifying there.
	Failed {
		/// The attempt.
		attempt: Attempt,
		/// The stage it stopped at.
		stage: Stage,
		/// The member of the candidate at fault, as a path below it, or empty for the candidate as
		/// a whole.
		member: &'static [Segment<'static>],
		/// What was observed, in the device's own words.
		reason: String,
	},
	/// The channel a radio's wireless client is on, or `None` once it is on none.
	StationChannel {
		/// The radio's station interface.
		interface: String,
		/// The channel.
		channel: Option<Channel>,
	},
	/// Make a failed candidate eligible again without waiting for it to become available anew.
	Retry {
		/// The candidate, by index into `attachments`.
		candidate: usize,
	},
}

/// The candidate an interface brings up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
	/// The candidate, by index into `attachments`.
	pub candidate: usize,
	/// This attempt at it, which verification reports back against.
	pub attempt: Attempt,
}

/// Where a candidate stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
	/// Established, and the highest such in the ordering, so carrying the default route.
	DefaultRoute,
	/// Established.
	Up,
	/// Brought up, and being verified at `at`.
	Verifying {
		/// The stage it is waiting to pass.
		at: Stage,
	},
	/// Available, but not tried: every interface it could go on carries a candidate above it, or
	/// the hotspot.
	Standby,
	/// Not available: it failed at `reached`, or has not reached it (no carrier, out of range).
	Unavailable {
		/// The stage it stopped at.
		reached: Stage,
		/// The member of the candidate at fault, as a path below it, or empty for the candidate as
		/// a whole.
		member: &'static [Segment<'static>],
		/// What was observed, in the device's own words.
		reason: String,
	},
}

/// The radio the hotspot runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
	/// The radio's station interface.
	pub radio: String,
	/// The channel it operates on.
	pub channel: HotspotChannel,
}

/// The channel the hotspot operates on (HOT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotspotChannel {
	/// Its own, as the document sets it or the device chooses.
	Own,
	/// The channel of the wireless client its shared-channel radio carries.
	Follows {
		/// The candidate that radio carries.
		candidate: usize,
		/// The channel that candidate is associated on, `None` until it is.
		channel: Option<Channel>,
	},
}

/// What should be up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Decision {
	/// The candidate each interface brings up, by interface. An interface absent brings up none.
	pub links: BTreeMap<String, Link>,
	/// The radio the hotspot runs on, where the document carries one.
	pub hotspot: Option<Placement>,
	/// The candidate carrying the default route: the highest established one (LINK).
	pub default_route: Option<usize>,
	/// Each candidate's state, positionally matching `attachments`.
	pub states: Vec<State>,
}

/// A part of the decision that differs from the last one, carrying the new value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
	/// An interface brings up another candidate, another attempt at one, or none.
	Link {
		/// The interface.
		interface: String,
		/// What it brings up now.
		link: Option<Link>,
	},
	/// The hotspot moved, stopped, started, or changed channel.
	Hotspot(Option<Placement>),
	/// Another candidate carries the default route, or none does.
	DefaultRoute(Option<usize>),
	/// A candidate's state changed.
	State {
		/// The candidate, by index into `attachments`.
		candidate: usize,
		/// Its state now.
		state: State,
	},
}

/// The answer to an event: the decision in force and what changed to get there.
#[derive(Debug)]
pub struct Update<'a> {
	/// What should be up now.
	#[cfg_attr(
		not(test),
		expect(
			dead_code,
			reason = "the backend reads the decision from the selector once it has followed the changes"
		)
	)]
	pub decision: &'a Decision,
	/// What differs from the decision before, in the order links, hotspot, default route, states.
	pub changes: Vec<Change>,
}

/// Selects among a document's candidates on some hardware, from what is observed.
#[derive(Debug, Clone)]
pub struct Selector {
	hardware: Hardware,
	document: Document,
	/// Wired interfaces with carrier.
	carrier: BTreeSet<String>,
	/// The signal each radio hears each network at, by (radio, ssid).
	heard: BTreeMap<(String, String), i32>,
	/// The channel each radio's wireless client is on.
	channels: BTreeMap<String, Channel>,
	/// Candidates that failed, with the stage, the member at fault and why, until they become
	/// available again.
	failures: BTreeMap<usize, (Stage, &'static [Segment<'static>], String)>,
	/// What each interface brings up, and how far verifying it has got.
	held: BTreeMap<String, Held>,
	next_attempt: u64,
	decision: Decision,
}

/// An attempt held on an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Held {
	link: Link,
	/// The stage it waits to pass, or `None` once established.
	at: Option<Stage>,
}

impl Hardware {
	fn radio(&self, station: &str) -> Option<&Radio> {
		self.radios.iter().find(|radio| radio.station == station)
	}

	/// The radios a wireless candidate may go on: the one it names, else every one.
	fn radios_for<'a>(&'a self, interface: Option<&'a str>) -> impl Iterator<Item = &'a Radio> {
		self.radios
			.iter()
			.filter(move |radio| interface.is_none_or(|name| radio.station == name))
	}

	/// The radios the hotspot may run on: the one it names where that can run one, else every one
	/// that can.
	fn hotspot_radios<'a>(&'a self, hotspot: &'a Hotspot) -> impl Iterator<Item = &'a Radio> {
		self.radios_for(hotspot.interface.as_deref())
			.filter(|radio| radio.access_point.is_some())
	}
}

impl Stage {
	/// The stage after this one for a candidate, or `None` after the last.
	fn next(self, wireless: bool) -> Option<Self> {
		match self {
			Self::Carrier if wireless => Some(Self::Association),
			Self::Carrier | Self::Association => Some(Self::Addressing),
			Self::Addressing => Some(Self::Gateway),
			Self::Gateway => None,
		}
	}
}

fn is_wireless(attachment: &Attachment) -> bool {
	matches!(attachment.kind, AttachmentKind::Wireless(_))
}

impl Selector {
	/// Select among `document`'s candidates on `hardware`, with nothing yet observed: no carrier,
	/// no network in range. Refuses a document selection could not carry out, as [`check`] does.
	pub fn new(hardware: Hardware, document: Document) -> Result<Self, Invalid> {
		check(&document, &hardware)?;
		let mut selector = Self {
			hardware,
			document,
			carrier: BTreeSet::new(),
			heard: BTreeMap::new(),
			channels: BTreeMap::new(),
			failures: BTreeMap::new(),
			held: BTreeMap::new(),
			next_attempt: 0,
			decision: Decision::default(),
		};
		selector.settle();
		Ok(selector)
	}

	/// What should be up now.
	pub fn decision(&self) -> &Decision {
		&self.decision
	}

	/// Select among another document's candidates, keeping what has been observed.
	///
	/// A candidate carried over unchanged keeps its attempt and its failure, wherever it now sits in
	/// the ordering, so the same document changes nothing. Every state is reported, since the
	/// positions they match have changed.
	pub fn configure(&mut self, document: Document) -> Result<Update<'_>, Invalid> {
		check(&document, &self.hardware)?;
		if document == self.document {
			return Ok(Update {
				decision: &self.decision,
				changes: Vec::new(),
			});
		}

		let mut taken = vec![false; document.attachments.len()];
		let moved: Vec<Option<usize>> = self
			.document
			.attachments
			.iter()
			.map(|old| {
				let new = (0..document.attachments.len())
					.find(|&new| !taken[new] && document.attachments[new] == *old)?;
				taken[new] = true;
				Some(new)
			})
			.collect();
		let moved = |old: usize| moved.get(old).copied().flatten();

		self.held
			.retain(|_, held| match moved(held.link.candidate) {
				Some(new) => {
					held.link.candidate = new;
					true
				}
				None => false,
			});
		self.failures = std::mem::take(&mut self.failures)
			.into_iter()
			.filter_map(|(old, failure)| Some((moved(old)?, failure)))
			.collect();
		self.document = document;
		self.decision.states.clear();
		let changes = self.settle();
		Ok(Update {
			decision: &self.decision,
			changes,
		})
	}

	/// Take in an observation and select again.
	pub fn handle(&mut self, event: Event) -> Update<'_> {
		match event {
			Event::Carrier { interface, up } => self.carrier(interface, up),
			Event::InRange {
				interface,
				ssid,
				signal,
			} => self.in_range(interface, ssid, signal),
			Event::OutOfRange { interface, ssid } => {
				self.heard.remove(&(interface, ssid));
			}
			Event::Passed { attempt, stage } => self.passed(attempt, stage),
			Event::Failed {
				attempt,
				stage,
				member,
				reason,
			} => {
				if let Some(held) = self.held.values().find(|held| held.link.attempt == attempt) {
					self.failures
						.insert(held.link.candidate, (stage, member, reason));
				}
			}
			Event::StationChannel { interface, channel } => {
				if self.hardware.radio(&interface).is_some() {
					match channel {
						Some(channel) => self.channels.insert(interface, channel),
						None => self.channels.remove(&interface),
					};
				}
			}
			Event::Retry { candidate } => {
				self.failures.remove(&candidate);
			}
		}
		let changes = self.settle();
		Update {
			decision: &self.decision,
			changes,
		}
	}

	/// The renderer's selection, where the hardware has at most one radio, which is all the renderer
	/// drives. `None` where it has several.
	pub fn selection(&self) -> Option<Selection> {
		if self.hardware.radios.len() > 1 {
			return None;
		}
		let mut active: Vec<usize> = self
			.decision
			.links
			.values()
			.map(|link| link.candidate)
			.collect();
		active.sort_unstable();
		Some(Selection {
			active,
			station_channel: self
				.hardware
				.radios
				.first()
				.and_then(|radio| self.associated(&radio.station)),
			hotspot_waits: matches!(
				self.decision.hotspot,
				Some(Placement {
					channel: HotspotChannel::Follows { channel: None, .. },
					..
				})
			),
		})
	}

	fn carrier(&mut self, interface: String, up: bool) {
		if !self.hardware.wired.contains(&interface) {
			return;
		}
		if !up {
			self.carrier.remove(&interface);
			return;
		}
		if self.carrier.insert(interface.clone()) {
			// Carrier returning makes every candidate on the interface available anew.
			let on_it: Vec<usize> = self
				.document
				.attachments
				.iter()
				.enumerate()
				.filter(|(_, attachment)| match &attachment.kind {
					AttachmentKind::WiredDynamic { interface: on }
					| AttachmentKind::WiredStatic { interface: on, .. } => *on == interface,
					AttachmentKind::Wireless(_) => false,
				})
				.map(|(rank, _)| rank)
				.collect();
			for rank in on_it {
				self.failures.remove(&rank);
			}
		}
	}

	fn in_range(&mut self, interface: String, ssid: String, signal: i32) {
		if self.hardware.radio(&interface).is_none() {
			return;
		}
		let key = (interface, ssid);
		if self.heard.insert(key.clone(), signal).is_some() {
			return;
		}
		// A network coming into range on a radio makes every candidate for it that the radio could
		// carry available anew.
		let (radio, ssid) = key;
		let anew: Vec<usize> = self
			.document
			.attachments
			.iter()
			.enumerate()
			.filter(|(_, attachment)| match &attachment.kind {
				AttachmentKind::Wireless(wireless) => {
					wireless.ssid == ssid
						&& wireless.interface.as_ref().is_none_or(|pin| *pin == radio)
				}
				_ => false,
			})
			.map(|(rank, _)| rank)
			.collect();
		for rank in anew {
			self.failures.remove(&rank);
		}
	}

	fn passed(&mut self, attempt: Attempt, stage: Stage) {
		let document = &self.document;
		if let Some(held) = self
			.held
			.values_mut()
			.find(|held| held.link.attempt == attempt)
			&& held.at == Some(stage)
		{
			held.at = stage.next(is_wireless(&document.attachments[held.link.candidate]));
		}
	}

	/// The channel the wireless client on `radio` is associated on, once it is.
	fn associated(&self, radio: &str) -> Option<Channel> {
		let held = self.held.get(radio)?;
		if held.at.is_some_and(|at| at <= Stage::Association) {
			return None;
		}
		self.channels.get(radio).copied()
	}

	/// Select again, and report what differs from the decision before.
	fn settle(&mut self) -> Vec<Change> {
		let placed = self.place();

		let mut held = BTreeMap::new();
		for (interface, &candidate) in &placed.links {
			let kept = self
				.held
				.get(interface)
				.filter(|old| old.link.candidate == candidate)
				.copied();
			let fresh = kept.unwrap_or_else(|| {
				self.next_attempt += 1;
				let first = Stage::Carrier.next(is_wireless(&self.document.attachments[candidate]));
				Held {
					link: Link {
						candidate,
						attempt: Attempt(self.next_attempt),
					},
					at: first,
				}
			});
			held.insert(interface.clone(), fresh);
		}
		// A radio's channel is its client's, and a client not carried over is gone.
		self.channels.retain(|radio, _| {
			self.held
				.get(radio)
				.is_some_and(|old| held.get(radio) == Some(old))
		});
		self.held = held;

		let default_route = self
			.held
			.values()
			.filter(|held| held.at.is_none())
			.map(|held| held.link.candidate)
			.min();
		let mut states = placed.idle;
		for held in self.held.values() {
			let state = match held.at {
				Some(at) => State::Verifying { at },
				None if Some(held.link.candidate) == default_route => State::DefaultRoute,
				None => State::Up,
			};
			states.insert(held.link.candidate, state);
		}

		let decision = Decision {
			links: self
				.held
				.iter()
				.map(|(interface, held)| (interface.clone(), held.link))
				.collect(),
			hotspot: self.place_hotspot(),
			default_route,
			states: states.into_values().collect(),
		};
		let changes = diff(&self.decision, &decision);
		self.decision = decision;
		changes
	}
}

fn diff(old: &Decision, new: &Decision) -> Vec<Change> {
	let mut changes = Vec::new();
	let interfaces: BTreeSet<&String> = old.links.keys().chain(new.links.keys()).collect();
	for interface in interfaces {
		let link = new.links.get(interface).copied();
		if old.links.get(interface).copied() != link {
			changes.push(Change::Link {
				interface: interface.clone(),
				link,
			});
		}
	}
	if old.hotspot != new.hotspot {
		changes.push(Change::Hotspot(new.hotspot.clone()));
	}
	if old.default_route != new.default_route {
		changes.push(Change::DefaultRoute(new.default_route));
	}
	for (candidate, state) in new.states.iter().enumerate() {
		if old.states.get(candidate) != Some(state) {
			changes.push(Change::State {
				candidate,
				state: state.clone(),
			});
		}
	}
	changes
}
