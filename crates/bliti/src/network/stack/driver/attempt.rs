//! Verifying one attempt through the stages of LINK: joining a wireless candidate, then its
//! addressing and its gateway, each against a deadline.

use std::time::Duration;

use super::{ASSOCIATE, CONFIGURE, Driver, Internal, LEASE, ROUTE};
use crate::network::{
	observe::{Joined, NetworkType, Station, render_channel},
	select::{Attempt, Event, Stage},
	stack::verify::{Addressing, Joining},
};

/// Why a join failed, in iwd's words except where they give an operator nothing to go on: iwd
/// answers a key-based network refusing the handshake with a bare `Failed`, which is most often a
/// wrong passphrase.
pub(in crate::network::stack) fn refused(joining: &Joining, reason: String) -> String {
	if joining.target.kind == NetworkType::Psk && reason.contains("net.connman.iwd.Failed") {
		tracing::info!(reason, "iwd could not join");
		return format!(
			"{:?} refused the connection; the passphrase is most likely wrong",
			joining.target.ssid
		);
	}
	reason
}

impl Driver {
	pub(super) fn arm(&mut self, attempt: Attempt) {
		let Some(check) = self.checks.get_mut(&attempt) else {
			return;
		};
		if check.armed {
			return;
		}
		check.armed = true;
		match check.at {
			// Joined from a scan older than the proposal, a network just gone would be tried.
			Some(Stage::Association) if self.scanning.contains_key(&check.interface) => {
				check.awaiting_scan = true;
			}
			Some(Stage::Association) => self.associate(attempt),
			Some(Stage::Addressing) => {
				self.address_deadline(attempt);
				self.evaluate(attempt);
			}
			_ => self.evaluate(attempt),
		}
	}

	pub(super) fn associate(&mut self, attempt: Attempt) {
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

	pub(super) fn associated(&mut self, attempt: Attempt, result: Result<Joined, String>) {
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
				reason: refused(&joining, reason),
			}),
		}
	}

	/// Move an attempt on as far as what is observed of its link takes it.
	pub(super) fn evaluate(&mut self, attempt: Attempt) {
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

	pub(super) fn probed(&mut self, attempt: Attempt, result: Result<(), String>) {
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
			Addressing::Dynamic => LEASE,
			Addressing::Static { .. } => CONFIGURE,
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

	pub(super) fn deadline_passed(&mut self, attempt: Attempt, timer: u64) {
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
					Addressing::Dynamic => format!(
						"{interface} was leased no address within {} seconds",
						LEASE.as_secs()
					),
					Addressing::Static { .. } => format!(
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
}
