//! What verifying one attempt looks for at each stage of LINK, from what is observed of its link.
//!
//! Pure: the driver keeps the time and asks the system; this says, from the addresses and default
//! routes observed on a link, whether an attempt's addressing stage has passed and which gateway its
//! gateway stage asks.

use std::{
	collections::{BTreeMap, BTreeSet},
	net::IpAddr,
};

use bliti_core::channel::config::{Attachment, AttachmentKind, Security};
use ipnet::IpNet;

use crate::network::{
	observe::{NetworkType, Observation, Target},
	select::{Attempt, Stage},
};

/// What is observed of each link's addresses and default routes.
#[derive(Debug, Default)]
pub(super) struct Links {
	/// Each link's addresses of global scope, each with whether it has a lifetime.
	addresses: BTreeMap<String, BTreeMap<IpAddr, bool>>,
	/// Each link's default gateways.
	gateways: BTreeMap<String, BTreeSet<IpAddr>>,
}

impl Links {
	/// Take in an address or route observation, returning the interface it concerns. Anything else is
	/// not this one's to take.
	pub(super) fn observe(&mut self, observation: &Observation) -> Option<String> {
		match observation {
			Observation::Address {
				interface,
				address,
				dynamic,
				present,
			} => {
				let held = self.addresses.entry(interface.clone()).or_default();
				if *present {
					held.insert(*address, *dynamic);
				} else {
					held.remove(address);
				}
				Some(interface.clone())
			}
			Observation::Route {
				interface,
				gateway,
				present,
			} => {
				let held = self.gateways.entry(interface.clone()).or_default();
				if *present {
					held.insert(*gateway);
				} else {
					held.remove(gateway);
				}
				Some(interface.clone())
			}
			_ => None,
		}
	}

	/// Everything a link holds now, addresses and gateways both, which is what a new attempt takes to
	/// be left over from the candidate before it.
	pub(super) fn held(&self, interface: &str) -> BTreeSet<IpAddr> {
		let addresses = self
			.addresses
			.get(interface)
			.into_iter()
			.flat_map(|a| a.keys());
		let gateways = self.gateways.get(interface).into_iter().flatten();
		addresses.chain(gateways).copied().collect()
	}

	fn addresses(&self, interface: &str) -> impl Iterator<Item = (IpAddr, bool)> + '_ {
		self.addresses
			.get(interface)
			.into_iter()
			.flat_map(|held| held.iter().map(|(address, dynamic)| (*address, *dynamic)))
	}

	fn gateways(&self, interface: &str) -> impl Iterator<Item = IpAddr> + '_ {
		self.gateways.get(interface).into_iter().flatten().copied()
	}
}

/// How a candidate is addressed, which is what its addressing and gateway stages look for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Addressing {
	/// Leased or autoconfigured: any address with a lifetime, and the default route's gateway.
	Dynamic,
	/// Configured: one of these addresses, and this gateway.
	Static {
		addresses: Vec<IpAddr>,
		gateway: Option<IpAddr>,
	},
}

impl Addressing {
	fn of(attachment: &Attachment) -> Self {
		match &attachment.kind {
			AttachmentKind::WiredStatic {
				addresses, gateway, ..
			} => Self::Static {
				addresses: addresses
					.iter()
					.filter_map(|address| address.parse::<IpNet>().ok())
					.map(|net| net.addr())
					.collect(),
				gateway: gateway.parse().ok(),
			},
			AttachmentKind::Wireless(_) | AttachmentKind::WiredDynamic { .. } => Self::Dynamic,
		}
	}
}

/// The network a wireless candidate joins, and whether it has to be joined by SAE (WLAN).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Joining {
	pub(super) target: Target,
	pub(super) sae: bool,
}

/// One attempt being verified, or established and watched for stopping.
#[derive(Debug, Clone)]
pub(super) struct Check {
	pub(super) attempt: Attempt,
	pub(super) candidate: usize,
	pub(super) interface: String,
	/// What it joins, for a wireless candidate.
	pub(super) joining: Option<Joining>,
	pub(super) addressing: Addressing,
	/// The stage it waits to pass, `None` once established.
	pub(super) at: Option<Stage>,
	/// Whether the render bringing it up has been applied, before which nothing is asked of it.
	pub(super) armed: bool,
	/// Addresses and gateways the link held when it started, left over from another candidate, and
	/// not to be taken as this one's until they are announced again.
	pub(super) stale: BTreeSet<IpAddr>,
	/// Whether a gateway probe is out.
	pub(super) probing: bool,
	/// Which of its deadlines is current, so a superseded one is recognised when it fires.
	pub(super) timer: u64,
}

impl Check {
	/// A check on `attempt` at `candidate` on `interface`, waiting at `at`.
	pub(super) fn new(
		attempt: Attempt,
		candidate: usize,
		attachment: &Attachment,
		interface: String,
		at: Stage,
		stale: BTreeSet<IpAddr>,
	) -> Self {
		let joining = match &attachment.kind {
			AttachmentKind::Wireless(wireless) => Some(Joining {
				target: Target {
					ssid: wireless.ssid.clone(),
					kind: match wireless.security {
						Security::Enterprise { .. } => NetworkType::Enterprise,
						Security::Psk { .. } | Security::Sae { .. } | Security::PskSae { .. } => {
							NetworkType::Psk
						}
					},
					hidden: wireless.hidden == Some(true),
				},
				sae: matches!(wireless.security, Security::Sae { .. }),
			}),
			_ => None,
		};
		Self {
			attempt,
			candidate,
			interface,
			joining,
			addressing: Addressing::of(attachment),
			at: Some(at),
			armed: false,
			stale,
			probing: false,
			timer: 0,
		}
	}

	/// Take in that the link announced `item` again, so it is this attempt's now.
	pub(super) fn announced(&mut self, observation: &Observation) {
		match observation {
			Observation::Address {
				address,
				present: true,
				..
			} => {
				self.stale.remove(address);
			}
			Observation::Route {
				gateway,
				present: true,
				..
			} => {
				self.stale.remove(gateway);
			}
			_ => {}
		}
	}

	/// The addresses the link holds that pass this candidate's addressing stage.
	pub(super) fn held(&self, links: &Links) -> Vec<IpAddr> {
		links
			.addresses(&self.interface)
			.filter(|(address, dynamic)| match &self.addressing {
				Addressing::Dynamic => *dynamic && !self.stale.contains(address),
				Addressing::Static { addresses, .. } => addresses.contains(address),
			})
			.map(|(address, _)| address)
			.collect()
	}

	/// The gateway to ask and the address to ask it from, where both are known: a configured
	/// gateway, or else a default route's, preferring IPv4.
	pub(super) fn gateway(&self, links: &Links) -> Option<(IpAddr, IpAddr)> {
		let held = self.held(links);
		let from = |gateway: IpAddr| {
			held.iter()
				.find(|address| address.is_ipv4() == gateway.is_ipv4())
				.map(|source| (*source, gateway))
		};
		match &self.addressing {
			Addressing::Static { gateway, .. } => from((*gateway)?),
			Addressing::Dynamic => {
				let mut gateways: Vec<IpAddr> = links
					.gateways(&self.interface)
					.filter(|gateway| !self.stale.contains(gateway))
					.collect();
				gateways.sort_by_key(|gateway| gateway.is_ipv6());
				gateways.into_iter().find_map(from)
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use serde_json::json;

	use super::*;

	fn attachment(json: serde_json::Value) -> Attachment {
		let document = bliti_core::channel::config::Document::parse(
			json!({"attachments": [json]}).as_object().unwrap(),
		)
		.unwrap();
		document.attachments.into_iter().next().unwrap()
	}

	fn address(links: &mut Links, address: &str, dynamic: bool) {
		links.observe(&Observation::Address {
			interface: "eth0".into(),
			address: address.parse().unwrap(),
			dynamic,
			present: true,
		});
	}

	fn route(links: &mut Links, gateway: &str) {
		links.observe(&Observation::Route {
			interface: "eth0".into(),
			gateway: gateway.parse().unwrap(),
			present: true,
		});
	}

	#[test]
	fn a_dynamic_candidate_takes_a_leased_address_and_the_routes_gateway() {
		let dynamic =
			attachment(json!({"kind": "wired-dynamic", "label": "e", "interface": "eth0"}));
		let mut links = Links::default();
		address(&mut links, "192.0.2.9", false);
		let check = Check::new(
			Attempt::test(1),
			0,
			&dynamic,
			"eth0".into(),
			Stage::Addressing,
			BTreeSet::new(),
		);
		assert!(
			check.held(&links).is_empty(),
			"a configured address is no lease"
		);
		address(&mut links, "198.51.100.7", true);
		address(&mut links, "2001:db8::7", true);
		assert_eq!(check.held(&links).len(), 2);
		assert_eq!(check.gateway(&links), None, "no default route yet");
		route(&mut links, "fe80::1");
		route(&mut links, "198.51.100.1");
		assert_eq!(
			check.gateway(&links),
			Some((
				"198.51.100.7".parse().unwrap(),
				"198.51.100.1".parse().unwrap()
			)),
			"IPv4 is asked first"
		);
	}

	#[test]
	fn what_another_candidate_left_is_not_taken_until_announced_again() {
		let dynamic =
			attachment(json!({"kind": "wired-dynamic", "label": "e", "interface": "eth0"}));
		let mut links = Links::default();
		address(&mut links, "198.51.100.7", true);
		let mut check = Check::new(
			Attempt::test(1),
			0,
			&dynamic,
			"eth0".into(),
			Stage::Addressing,
			links.held("eth0"),
		);
		assert!(check.held(&links).is_empty());
		let again = Observation::Address {
			interface: "eth0".into(),
			address: "198.51.100.7".parse().unwrap(),
			dynamic: true,
			present: true,
		};
		links.observe(&again);
		check.announced(&again);
		assert_eq!(check.held(&links).len(), 1);
	}

	#[test]
	fn a_static_candidate_takes_its_own_address_and_gateway() {
		let fixed = attachment(json!({
			"kind": "wired-static", "label": "s", "interface": "eth0",
			"addresses": ["192.0.2.9/24"], "gateway": "192.0.2.1"
		}));
		let mut links = Links::default();
		let check = Check::new(
			Attempt::test(1),
			0,
			&fixed,
			"eth0".into(),
			Stage::Addressing,
			BTreeSet::new(),
		);
		address(&mut links, "198.51.100.7", true);
		assert!(check.held(&links).is_empty(), "not its address");
		address(&mut links, "192.0.2.9", false);
		assert_eq!(
			check.gateway(&links),
			Some(("192.0.2.9".parse().unwrap(), "192.0.2.1".parse().unwrap()))
		);
	}
}
