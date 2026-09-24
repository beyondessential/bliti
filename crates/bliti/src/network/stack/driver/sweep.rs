//! Scanning for a wireless candidate ranked above the one carrying the default route while its
//! network is out of range, which is the one change nothing announces (LINK). A candidate turned
//! off is not looked for.
//!
//! A radio is swept while it is free to carry such a candidate: it carries nothing ranked above it,
//! WPS is not running on it, and it can scan. Where nothing carries the default route, every
//! wireless candidate out of range counts. The first scan waits as long as a retry does, and each
//! after it waits twice as long, up to the longest; a radio with nothing left to look for stops at
//! once, and starts again from the first wait. What a sweep hears is taken in as any scan's is.

use std::collections::BTreeSet;

use bliti_core::channel::config::AttachmentKind;

use super::{Driver, Internal, Scanner, backoff};

/// One radio's sweep.
#[derive(Debug)]
pub(super) struct Sweep {
	/// Recognises what this sweep started, among sweeps since stopped.
	token: u64,
	/// Scans made so far.
	tries: u32,
}

impl Driver {
	/// Start sweeping each radio with something to look for, and stop sweeping the rest.
	pub(super) fn sweep(&mut self) {
		let wanted = self.sweeping();
		self.sweeps.retain(|radio, _| wanted.contains(radio));
		for radio in wanted {
			if self.sweeps.contains_key(&radio) {
				continue;
			}
			self.swept += 1;
			tracing::info!(radio, "looking for a network ranked above the one in force");
			self.sweeps.insert(
				radio.clone(),
				Sweep {
					token: self.swept,
					tries: 0,
				},
			);
			self.sweep_later(radio);
		}
	}

	/// The radios free to carry a wireless candidate above the default route that is out of range.
	fn sweeping(&self) -> BTreeSet<String> {
		let mut radios = BTreeSet::new();
		if !self.configured {
			return radios;
		}
		let decision = self.selector.decision();
		let above = decision
			.default_route
			.unwrap_or(self.document.attachments.len());
		let scanning = self.shared.radios();
		for (rank, attachment) in self.document.attachments[..above].iter().enumerate() {
			let AttachmentKind::Wireless(wireless) = &attachment.kind else {
				continue;
			};
			if !attachment.enabled {
				continue;
			}
			// A hidden network is taken to be in range, since it cannot be heard by name.
			if wireless.hidden == Some(true) {
				continue;
			}
			let could = || {
				scanning.iter().filter(|radio| {
					wireless
						.interface
						.as_ref()
						.is_none_or(|pin| *pin == radio.station)
				})
			};
			let heard = could().any(|radio| {
				self.heard
					.get(&radio.station)
					.is_some_and(|networks| networks.contains_key(&wireless.ssid))
			});
			if heard {
				continue;
			}
			let free = could().filter(|radio| {
				radio.scan
					&& !self.held.contains(&radio.station)
					&& decision
						.links
						.get(&radio.station)
						.is_none_or(|link| link.candidate > rank)
			});
			radios.extend(free.map(|radio| radio.station.clone()));
		}
		radios
	}

	/// Wait out a sweep's backoff before its next scan.
	fn sweep_later(&mut self, radio: String) {
		let Some(sweep) = self.sweeps.get_mut(&radio) else {
			return;
		};
		let wait = backoff(sweep.tries);
		sweep.tries += 1;
		let token = sweep.token;
		let internal = self.internal.clone();
		tokio::spawn(async move {
			tokio::time::sleep(wait).await;
			let _ = internal.send(Internal::Sweep { radio, token });
		});
	}

	/// Scan for a sweep whose wait is over, where it has not stopped since.
	pub(super) fn sweep_now(&mut self, radio: String, token: u64) {
		if self
			.sweeps
			.get(&radio)
			.is_some_and(|sweep| sweep.token == token)
			&& !self.held.contains(&radio)
		{
			self.scan(radio, Scanner::Sweep(token));
		}
	}

	/// Take in that a sweep's scan is in, and wait for the next where it goes on.
	pub(super) fn swept_on(&mut self, radio: &str, token: u64) {
		if self
			.sweeps
			.get(radio)
			.is_some_and(|sweep| sweep.token == token)
		{
			self.sweep_later(radio.to_owned());
		}
	}

	/// Take in that a pending proposal's scan of `radio` is in, letting the joins waiting on it go
	/// ahead once none is left.
	pub(super) fn scanned_for_pending(&mut self, radio: &str) {
		let Some(left) = self.scanning.get_mut(radio) else {
			return;
		};
		*left = left.saturating_sub(1);
		if *left > 0 {
			return;
		}
		self.scanning.remove(radio);
		if self.hotspot_scanning.as_deref() == Some(radio) {
			self.wanted += 1;
			self.kick();
		}
		let waiting: Vec<_> = self
			.checks
			.values_mut()
			.filter(|check| check.interface == radio && check.awaiting_scan)
			.map(|check| {
				check.awaiting_scan = false;
				check.attempt
			})
			.collect();
		for attempt in waiting {
			self.associate(attempt);
		}
	}
}
