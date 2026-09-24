//! What a verified proposal comes to (CFG, "When a proposal fails").
//!
//! A proposal is judged on the candidates it adds or changes that carry `verify` true. A candidate
//! is added or changed where it equals no candidate of the configuration running before it, each of
//! that configuration's candidates matching at most one, as the selector carries a candidate over.
//! A candidate that only moved in the ordering is carried over, and so has already been judged.
//! Each is judged on the interface it goes on: a wired candidate's, the radio a wireless one names,
//! else the radio it was brought up on, else, where it never was, any radio it could go on.

use std::cmp::Reverse;

use bliti_core::channel::config::{AttachmentKind, Document, Invalid, Segment, path};

use super::Driver;
use crate::network::select::{Decision, State};

/// A candidate a proposal adds or changes, and the interfaces it is judged on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::network::stack) struct Judged {
	pub(in crate::network::stack) candidate: usize,
	/// Where any of these carries an established candidate, it holds.
	pub(in crate::network::stack) interfaces: Vec<String>,
}

/// The candidates of `proposal` that `running` does not carry.
pub(in crate::network::stack) fn changed(running: &Document, proposal: &Document) -> Vec<usize> {
	let mut taken = vec![false; running.attachments.len()];
	proposal
		.attachments
		.iter()
		.enumerate()
		.filter(|(_, attachment)| {
			let kept = running
				.attachments
				.iter()
				.enumerate()
				.find(|(old, before)| !taken[*old] && *before == *attachment);
			match kept {
				Some((old, _)) => {
					taken[old] = true;
					false
				}
				None => true,
			}
		})
		.map(|(candidate, _)| candidate)
		.collect()
}

/// Applied where every candidate judged holds; else failed at the one of those that do not which
/// passed the most stages, the first in the ordering among equals, at the member of it at fault
/// where its failure names one.
pub(in crate::network::stack) fn verdict(
	decision: &Decision,
	judged: &[Judged],
) -> Result<(), Invalid> {
	let established = |interface: &String| {
		decision.links.get(interface).is_some_and(|link| {
			matches!(
				decision.states.get(link.candidate),
				Some(State::Up | State::DefaultRoute)
			)
		})
	};
	let furthest = judged
		.iter()
		.filter(|judged| !judged.interfaces.iter().any(established))
		.map(|judged| {
			let failure = match decision.states.get(judged.candidate) {
				Some(State::Unavailable {
					reached,
					member,
					reason,
				}) => Some((*reached, *member, reason)),
				_ => None,
			};
			(judged.candidate, failure)
		})
		.max_by_key(|(candidate, failure)| {
			(failure.map(|(reached, ..)| reached), Reverse(*candidate))
		});
	let Some((candidate, failure)) = furthest else {
		return Ok(());
	};
	let at = |member: &[Segment<'_>]| {
		let mut segments = vec![Segment::Name("attachments"), Segment::Index(candidate)];
		segments.extend_from_slice(member);
		path(&segments)
	};
	Err(match failure {
		Some((reached, member, reason)) => reached.failed(at(member), reason.clone()),
		None => Invalid {
			at: at(&[]),
			reason: "no interface it could go on was free to bring it up".to_owned(),
			reached: None,
		},
	})
}

impl Driver {
	/// Those of `candidates` carrying `verify` true, each with the interfaces it is judged on.
	pub(super) fn judged(&self, candidates: &[usize]) -> Vec<Judged> {
		let decision = self.selector.decision();
		candidates
			.iter()
			.filter_map(|&candidate| {
				let attachment = self.document.attachments.get(candidate)?;
				if !attachment.verify {
					return None;
				}
				let interfaces = match &attachment.kind {
					AttachmentKind::WiredDynamic { interface }
					| AttachmentKind::WiredStatic { interface, .. } => vec![interface.clone()],
					AttachmentKind::Wireless(wireless) => match &wireless.interface {
						Some(interface) => vec![interface.clone()],
						None => decision
							.links
							.iter()
							.find(|(_, link)| link.candidate == candidate)
							.map(|(interface, _)| interface)
							.or_else(|| self.placed.get(&candidate))
							.map_or_else(
								|| {
									self.shared
										.select
										.radios
										.iter()
										.map(|radio| radio.station.clone())
										.collect()
								},
								|radio| vec![radio.clone()],
							),
					},
				};
				Some(Judged {
					candidate,
					interfaces,
				})
			})
			.collect()
	}
}
