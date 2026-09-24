//! Placing candidates on interfaces, and the hotspot on a radio (LINK, HOT).

use std::{cmp::Reverse, collections::BTreeMap};

use bliti_core::channel::config::{AttachmentKind, Wireless};

use super::{Alongside, HotspotChannel, Placement, Radio, Selector, Stage, State};

/// Where the candidates go.
pub(super) struct Placed {
	/// The candidate each interface brings up.
	pub(super) links: BTreeMap<String, usize>,
	/// The state of every candidate not brought up.
	pub(super) idle: BTreeMap<usize, State>,
}

/// Unavailable at `reached`, with no one member at fault.
fn unavailable(reached: Stage, reason: String) -> State {
	State::Unavailable {
		reached,
		member: &[],
		reason,
	}
}

impl Selector {
	/// Place each candidate in order on an interface carrying none above it, so each interface
	/// carries the highest candidate available on it.
	pub(super) fn place(&self) -> Placed {
		let mut links = BTreeMap::new();
		let mut idle = BTreeMap::new();
		for (rank, attachment) in self.document.attachments.iter().enumerate() {
			let placed = match &attachment.kind {
				AttachmentKind::WiredDynamic { interface }
				| AttachmentKind::WiredStatic { interface, .. } => self.wired(rank, interface, &links),
				AttachmentKind::Wireless(wireless) => self.wireless(rank, wireless, &links),
			};
			match placed {
				Ok(interface) => {
					links.insert(interface, rank);
				}
				Err(state) => {
					idle.insert(rank, state);
				}
			}
		}
		Placed { links, idle }
	}

	/// Why the candidate at `rank` is not to be tried, where it failed and has not become available
	/// since.
	fn failed(&self, rank: usize) -> Result<(), State> {
		match self.failures.get(&rank) {
			Some((reached, member, reason)) => Err(State::Unavailable {
				reached: *reached,
				member,
				reason: reason.clone(),
			}),
			None => Ok(()),
		}
	}

	fn wired(
		&self,
		rank: usize,
		interface: &str,
		links: &BTreeMap<String, usize>,
	) -> Result<String, State> {
		if !self.carrier.contains(interface) {
			return Err(unavailable(
				Stage::Carrier,
				format!("{interface} has no carrier"),
			));
		}
		self.failed(rank)?;
		if links.contains_key(interface) {
			return Err(State::Standby);
		}
		Ok(interface.to_owned())
	}

	/// A wireless candidate goes on the radio it names, or else on any radio hearing it that carries
	/// nothing above it, preferring the one it is already on and then the one hearing it best.
	///
	/// A hidden network cannot be heard by name, so it is taken to be in range of every radio it may
	/// go on, heard worse than any network that is; one that is not fails at association.
	fn wireless(
		&self,
		rank: usize,
		wireless: &Wireless,
		links: &BTreeMap<String, usize>,
	) -> Result<String, State> {
		let hidden = wireless.hidden == Some(true);
		let heard: Vec<(&Radio, Option<i32>)> = self
			.hardware
			.radios_for(wireless.interface.as_deref())
			.filter_map(|radio| {
				match self
					.heard
					.get(&(radio.station.clone(), wireless.ssid.clone()))
				{
					Some(&signal) => Some((radio, Some(signal))),
					None if hidden => Some((radio, None)),
					None => None,
				}
			})
			.collect();
		if heard.is_empty() {
			let ssid = &wireless.ssid;
			return Err(unavailable(
				Stage::Carrier,
				match &wireless.interface {
					Some(interface) => format!("{ssid:?} is out of range of {interface}"),
					None => format!("{ssid:?} is out of range"),
				},
			));
		}
		self.failed(rank)?;

		let free: Vec<(&Radio, Option<i32>)> = heard
			.into_iter()
			.filter(|(radio, _)| {
				!links.contains_key(&radio.station) && self.spares_hotspot(radio, links)
			})
			.collect();
		let current = self
			.held
			.iter()
			.find(|(_, held)| held.link.candidate == rank)
			.map(|(interface, _)| interface);
		if let Some(current) = current
			&& free.iter().any(|(radio, _)| radio.station == *current)
		{
			return Ok(current.clone());
		}
		free.iter()
			.min_by_key(|(_, signal)| Reverse(*signal))
			.map(|(radio, _)| radio.station.clone())
			.ok_or(State::Standby)
	}

	/// Whether a wireless candidate taking `radio` leaves the hotspot a radio to run on (HOT). Only a
	/// one-at-a-time radio can take one away.
	fn spares_hotspot(&self, radio: &Radio, links: &BTreeMap<String, usize>) -> bool {
		let Some(hotspot) = &self.document.hotspot else {
			return true;
		};
		if radio.access_point != Some(Alongside::OneAtATime) {
			return true;
		}
		self.hardware
			.hotspot_hosts(&self.document, hotspot)
			.any(|other| {
				other.station != radio.station
					&& !(other.access_point == Some(Alongside::OneAtATime)
						&& links.contains_key(&other.station))
			})
	}

	/// The hotspot goes on the radio it names, or else on a radio carrying no wireless candidate,
	/// then one running both independently, then one sharing its client's channel. It stays where it
	/// is among the best of those, so a candidate coming up elsewhere does not move it. One choosing
	/// its own channel goes on no shared-channel radio a candidate may take (HOT).
	pub(super) fn place_hotspot(&self) -> Option<Placement> {
		let hotspot = self.document.hotspot.as_ref()?;
		let current = self.decision.hotspot.as_ref().map(|placed| &placed.radio);
		let options: Vec<(&Radio, u8)> = self
			.hardware
			.hotspot_hosts(&self.document, hotspot)
			.filter_map(|radio| {
				let tier = match (self.held.contains_key(&radio.station), radio.access_point?) {
					(false, _) => 0,
					(true, Alongside::Independent) => 1,
					(true, Alongside::SharedChannel) => 2,
					(true, Alongside::OneAtATime) => return None,
				};
				Some((radio, tier))
			})
			.collect();
		let best = options.iter().map(|&(_, tier)| tier).min()?;
		let mut best = options
			.into_iter()
			.filter(|&(_, tier)| tier == best)
			.map(|(radio, _)| radio);
		let first = best.clone().next()?;
		let radio = best
			.find(|radio| Some(&radio.station) == current)
			.unwrap_or(first);

		let channel = match self.held.get(&radio.station) {
			Some(held) if radio.access_point == Some(Alongside::SharedChannel) => {
				HotspotChannel::Follows {
					candidate: held.link.candidate,
					channel: self.associated(&radio.station),
				}
			}
			_ => HotspotChannel::Own,
		};
		Some(Placement {
			radio: radio.station.clone(),
			channel,
		})
	}
}
