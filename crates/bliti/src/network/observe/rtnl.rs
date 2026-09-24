//! Carrier, addresses and default routes, from rtnetlink.
//!
//! Subscribed to the link, address and route groups before anything is dumped, so nothing that
//! changes while the dump is read is missed; the dump says how things stand at start, and the groups
//! say each change after.

use std::{
	collections::HashMap,
	net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use anyhow::Context as _;
use futures::{StreamExt as _, TryStreamExt as _};
use netlink_packet_route::{
	RouteNetlinkMessage,
	address::{AddressAttribute, AddressFlags, AddressMessage, AddressScope},
	link::{LinkAttribute, LinkFlags, LinkMessage},
	route::{RouteAddress, RouteAttribute, RouteMessage, RouteType},
};
use rtnetlink::{MulticastGroup, RouteMessageBuilder, packet_core::NetlinkPayload};
use tokio::sync::mpsc;

use super::Observation;

/// The main routing table, which networkd puts a link's default route in.
const MAIN: u32 = 254;

/// Interface names by index, as links are announced.
type Names = HashMap<u32, String>;

/// Watch every link, sending what is observed on `observations` until it closes.
pub(super) async fn watch(observations: mpsc::UnboundedSender<Observation>) -> anyhow::Result<()> {
	let (connection, handle, mut messages) = rtnetlink::new_multicast_connection(&[
		MulticastGroup::Link,
		MulticastGroup::Ipv4Ifaddr,
		MulticastGroup::Ipv6Ifaddr,
		MulticastGroup::Ipv4Route,
		MulticastGroup::Ipv6Route,
	])
	.context("cannot subscribe to rtnetlink")?;
	tokio::spawn(connection);

	let mut names = Names::new();
	let send = |observation| {
		let _ = observations.send(observation);
	};
	let mut links = handle.link().get().execute();
	while let Some(message) = links.try_next().await.context("cannot list links")? {
		link(&message, &mut names, true).into_iter().for_each(send);
	}
	let mut addresses = handle.address().get().execute();
	while let Some(message) = addresses
		.try_next()
		.await
		.context("cannot list addresses")?
	{
		address(&message, &names, true).into_iter().for_each(send);
	}
	for query in [
		RouteMessageBuilder::<Ipv4Addr>::new().build(),
		RouteMessageBuilder::<Ipv6Addr>::new().build(),
	] {
		let mut routes = handle.route().get(query).execute();
		while let Some(message) = routes.try_next().await.context("cannot list routes")? {
			route(&message, &names, true).into_iter().for_each(send);
		}
	}

	tokio::spawn(async move {
		while let Some((message, _)) = messages.next().await {
			let NetlinkPayload::InnerMessage(message) = message.payload else {
				continue;
			};
			let observed = match message {
				RouteNetlinkMessage::NewLink(message) => link(&message, &mut names, true),
				RouteNetlinkMessage::DelLink(message) => link(&message, &mut names, false),
				RouteNetlinkMessage::NewAddress(message) => address(&message, &names, true),
				RouteNetlinkMessage::DelAddress(message) => address(&message, &names, false),
				RouteNetlinkMessage::NewRoute(message) => route(&message, &names, true),
				RouteNetlinkMessage::DelRoute(message) => route(&message, &names, false),
				_ => None,
			};
			if let Some(observation) = observed
				&& observations.send(observation).is_err()
			{
				return;
			}
		}
		tracing::error!("rtnetlink stopped sending; carrier and addresses are no longer observed");
	});
	Ok(())
}

/// A link's carrier, keeping its name for the addresses and routes that name it by index.
fn link(message: &LinkMessage, names: &mut Names, present: bool) -> Option<Observation> {
	let index = message.header.index;
	let named = message
		.attributes
		.iter()
		.find_map(|attribute| match attribute {
			LinkAttribute::IfName(name) => Some(name.clone()),
			_ => None,
		});
	let interface = match named {
		Some(name) => {
			names.insert(index, name.clone());
			name
		}
		None => names.get(&index)?.clone(),
	};
	if !present {
		names.remove(&index);
	}
	Some(Observation::Carrier {
		interface,
		up: present && message.header.flags.contains(LinkFlags::LowerUp),
	})
}

/// An address of global scope, once it is no longer tentative.
fn address(message: &AddressMessage, names: &Names, present: bool) -> Option<Observation> {
	if message.header.scope != AddressScope::Universe {
		return None;
	}
	let interface = names.get(&message.header.index)?.clone();
	let mut address = None;
	let mut flags = AddressFlags::empty();
	for attribute in &message.attributes {
		match attribute {
			AddressAttribute::Local(local) => address = Some(*local),
			AddressAttribute::Address(peer) if address.is_none() => address = Some(*peer),
			AddressAttribute::Flags(value) => flags = *value,
			_ => {}
		}
	}
	if present && flags.contains(AddressFlags::Tentative) {
		return None;
	}
	Some(Observation::Address {
		interface,
		address: address?,
		dynamic: !flags.contains(AddressFlags::Permanent),
		present,
	})
}

/// A default route of the main table through a gateway.
fn route(message: &RouteMessage, names: &Names, present: bool) -> Option<Observation> {
	if message.header.destination_prefix_length != 0 || message.header.kind != RouteType::Unicast {
		return None;
	}
	let mut table = u32::from(message.header.table);
	let (mut gateway, mut interface) = (None, None);
	for attribute in &message.attributes {
		match attribute {
			RouteAttribute::Table(value) => table = *value,
			RouteAttribute::Gateway(RouteAddress::Inet(v4)) => gateway = Some(IpAddr::V4(*v4)),
			RouteAttribute::Gateway(RouteAddress::Inet6(v6)) => gateway = Some(IpAddr::V6(*v6)),
			RouteAttribute::Oif(index) => interface = names.get(index).cloned(),
			_ => {}
		}
	}
	if table != MAIN {
		return None;
	}
	Some(Observation::Route {
		interface: interface?,
		gateway: gateway?,
		present,
	})
}
