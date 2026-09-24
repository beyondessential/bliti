//! systemd-networkd: addressing, routes and per-link resolvers for each candidate, and the hotspot's
//! address and DHCP server.

use std::{fmt::Write as _, net::IpAddr, str::FromStr};

use bliti_core::channel::config::{Attachment, AttachmentKind, Hotspot, Invalid, Segment};
use ipnet::{IpNet, Ipv4Net};

use super::{File, Hardware, PUBLIC, Paths, candidate_path, header, invalid, invalid_in};

/// What every `.network` bliti renders is named with, and how an applier recognises one.
const PREFIX: &str = "50-bliti-";

/// The range a hotspot hands out where the document sets none, the same on every bliti device (HOT).
///
/// Clear of the ranges home routers, phone hotspots and container runtimes commonly use.
pub const DEFAULT_DHCP_RANGE: &str = "10.41.0.0/24";

/// The metric of the highest-ranked candidate's routes. Each candidate below it adds one.
const METRIC_BASE: u32 = 100;

/// How long a wireless link rides out losing carrier before networkd drops its addresses, which is
/// what roaming between access points looks like.
const ROAMING_GRACE: &str = "3s";

pub(super) fn owns(name: &str) -> bool {
	name.starts_with(PREFIX) && name.ends_with(".network")
}

/// Whether a file in resolved's delegate directory is one bliti renders.
pub(super) fn owns_delegate(name: &str) -> bool {
	name.starts_with(PREFIX) && name.ends_with(".dns-delegate")
}

/// The route metric of the candidate at `rank`. The lowest wins, so of the candidates up the one
/// highest in the ordering carries the default route and the rest stay reachable (LINK).
fn metric(rank: usize) -> u32 {
	METRIC_BASE.saturating_add(u32::try_from(rank).unwrap_or(u32::MAX))
}

/// The files a candidate brings up on its interface, a wireless one on the radio whose station is
/// `station`: its `.network`, and for a dynamic link naming resolvers of its own, the DNS delegate
/// carrying them.
pub(super) fn candidate(
	attachment: &Attachment,
	rank: usize,
	station: Option<&str>,
	hardware: &Hardware,
) -> Result<Vec<File>, Invalid> {
	let nameservers = nameservers(attachment, rank)?;
	let from = candidate_path(rank);
	let (interface, contents) = match &attachment.kind {
		AttachmentKind::Wireless(_) => {
			let Some(station) = station else {
				return Err(invalid_in(rank, &[], "this device has no wireless client"));
			};
			(station, dynamic(&from, station, rank, &nameservers, true))
		}
		AttachmentKind::WiredDynamic { interface } => {
			wired(interface, rank, hardware)?;
			(
				interface.as_str(),
				dynamic(&from, interface, rank, &nameservers, false),
			)
		}
		AttachmentKind::WiredStatic {
			interface,
			addresses,
			gateway,
		} => {
			wired(interface, rank, hardware)?;
			let addresses = addresses
				.iter()
				.enumerate()
				.map(|(i, address)| {
					IpNet::from_str(address).map_err(|_| {
						invalid_in(
							rank,
							&[Segment::Name("addresses"), Segment::Index(i)],
							format!("{address:?} is not an address with its prefix length"),
						)
					})
				})
				.collect::<Result<Vec<_>, _>>()?;
			let gateway = IpAddr::from_str(gateway).map_err(|_| {
				invalid_in(
					rank,
					&[Segment::Name("gateway")],
					format!("{gateway:?} is not an address"),
				)
			})?;
			(
				interface.as_str(),
				fixed(&from, interface, rank, &nameservers, &addresses, gateway),
			)
		}
	};
	let mut files = vec![File {
		path: path(&hardware.paths, interface),
		contents,
		mode: PUBLIC,
	}];
	let dynamic = !matches!(attachment.kind, AttachmentKind::WiredStatic { .. });
	if dynamic && !nameservers.is_empty() {
		files.push(delegate(&from, interface, &nameservers, &hardware.paths));
	}
	Ok(files)
}

/// A wired interface whose candidates are none of them brought up: up, so its carrier can be seen,
/// and configured with nothing. networkd leaves a link no file matches alone, and a link left down
/// never reports carrier, so without this no candidate on it could ever become available (LINK).
pub(super) fn idle(interface: &str, paths: &Paths) -> File {
	let mut contents = header(&super::path(&[Segment::Name("attachments")]));
	let _ = write!(
		contents,
		"\n[Match]\nName={interface}\n\n[Link]\nRequiredForOnline=no\n\n\
		 [Network]\nDHCP=no\nIPv6AcceptRA=no\nLinkLocalAddressing=no\n"
	);
	File {
		path: path(paths, interface),
		contents,
		mode: PUBLIC,
	}
}

/// The DNS delegate carrying a dynamic link's own resolvers.
///
/// resolved routes a query to a link and then uses that link's servers in turn, falling through only
/// on an error, and a resolver answering that a name does not exist has not erred. So the link's own
/// resolvers and the ones its network supplies cannot share the link: the site's names would never
/// reach the site. The link keeps the supplied ones, with the site's search domains routed to it, and
/// the candidate's own answer everything else from a delegate bound to the link (LINK). Delegates
/// need systemd 258.
fn delegate(from: &str, interface: &str, nameservers: &[IpAddr], paths: &Paths) -> File {
	let mut contents = header(from);
	let _ = writeln!(
		contents,
		"
[Delegate]"
	);
	for server in nameservers {
		let _ = writeln!(contents, "DNS={server}%{interface}");
	}
	let _ = write!(contents, "Domains=~.\nDefaultRoute=yes\n");
	File {
		path: paths
			.resolved
			.join(format!("{PREFIX}{interface}.dns-delegate")),
		contents,
		mode: PUBLIC,
	}
}

fn path(paths: &Paths, interface: &str) -> std::path::PathBuf {
	paths.networkd.join(format!("{PREFIX}{interface}.network"))
}

fn wired(interface: &str, rank: usize, hardware: &Hardware) -> Result<(), Invalid> {
	if hardware.wired.iter().any(|known| known == interface) {
		Ok(())
	} else {
		Err(invalid_in(
			rank,
			&[Segment::Name("interface")],
			format!("this device has no wired interface {interface:?}"),
		))
	}
}

fn nameservers(attachment: &Attachment, rank: usize) -> Result<Vec<IpAddr>, Invalid> {
	attachment
		.nameservers
		.iter()
		.enumerate()
		.map(|(i, server)| {
			IpAddr::from_str(server).map_err(|_| {
				invalid_in(
					rank,
					&[Segment::Name("nameservers"), Segment::Index(i)],
					format!("{server:?} is not an address"),
				)
			})
		})
		.collect()
}

/// A static link's resolvers, which sit on the link itself: nothing supplies it any to share it with.
fn write_dns(out: &mut String, nameservers: &[IpAddr]) {
	for server in nameservers {
		let _ = writeln!(out, "DNS={server}");
	}
}

/// A link addressed by DHCPv4 and by router advertisement, with DHCPv6 where those ask for it.
fn dynamic(
	from: &str,
	interface: &str,
	rank: usize,
	nameservers: &[IpAddr],
	wireless: bool,
) -> String {
	let metric = metric(rank);
	let mut out = header(from);
	let _ = write!(
		out,
		"\n[Match]\nName={interface}\n\n[Network]\nDHCP=ipv4\nIPv6AcceptRA=yes\n"
	);
	if wireless {
		let _ = writeln!(out, "IgnoreCarrierLoss={ROAMING_GRACE}");
	}
	// The site's search domains route its own names to the resolvers it supplies. Routing them makes
	// resolved stop treating the link as a default route, so that is said outright: a link with
	// resolvers of its own in a delegate is not one, and one without is (see `delegate`).
	let default_route = if nameservers.is_empty() { "yes" } else { "no" };
	let _ = writeln!(out, "DNSDefaultRoute={default_route}");
	let _ = write!(
		out,
		"\n[DHCPv4]\nUseDNS=yes\nUseDomains=route\nRouteMetric={metric}\n\
		 \n[DHCPv6]\nUseDNS=yes\nUseDomains=route\n\
		 \n[IPv6AcceptRA]\nUseDNS=yes\nUseDomains=route\nRouteMetric={metric}\n"
	);
	out
}

/// A link with static addressing and a gateway.
fn fixed(
	from: &str,
	interface: &str,
	rank: usize,
	nameservers: &[IpAddr],
	addresses: &[IpNet],
	gateway: IpAddr,
) -> String {
	let mut out = header(from);
	let _ = write!(
		out,
		"\n[Match]\nName={interface}\n\n[Network]\nDHCP=no\nIPv6AcceptRA=no\n"
	);
	for address in addresses {
		let _ = writeln!(out, "Address={address}");
	}
	write_dns(&mut out, nameservers);
	let _ = write!(
		out,
		"\n[Route]\nGateway={gateway}\nMetric={}\n",
		metric(rank)
	);
	out
}

/// The addresses a hotspot hands out: the device holds the first host of the subnet and leases the
/// rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pool {
	/// The device's own address on the hotspot, with the subnet's prefix length.
	pub(super) server: Ipv4Net,
	/// Where the leased addresses start, counted from the subnet address.
	pub(super) offset: u32,
	/// How many addresses are leased.
	pub(super) size: u32,
}

/// Read `dhcp-range` as an IPv4 subnet (e.g. `10.41.0.0/24`).
///
/// Kept to this one function because the member's wire format is under review.
pub(super) fn dhcp_range(range: &str) -> Result<Pool, String> {
	let subnet = Ipv4Net::from_str(range)
		.map_err(|_| format!("{range:?} is not an IPv4 subnet such as {DEFAULT_DHCP_RANGE:?}"))?;
	if subnet.addr() != subnet.network() {
		return Err(format!(
			"{range:?} is not a subnet address; the subnet is {}",
			subnet.trunc()
		));
	}
	if subnet.prefix_len() > 30 {
		return Err(format!(
			"{range:?} leaves no address to hand out beside the device's own"
		));
	}
	let hosts = (1u32 << (32 - subnet.prefix_len())) - 2;
	let server = Ipv4Net::new(
		u32::from(subnet.network()).saturating_add(1).into(),
		subnet.prefix_len(),
	)
	.map_err(|_| format!("{range:?} is not an IPv4 subnet"))?;
	Ok(Pool {
		server,
		offset: 2,
		size: hosts - 1,
	})
}

/// The hotspot's interface: the device's address, a DHCP server over the rest of the range, and
/// masquerading onto the device's own network where the hotspot shares it (HOT).
pub(super) fn hotspot(hotspot: &Hotspot, interface: &str, paths: &Paths) -> Result<File, Invalid> {
	let pool = dhcp_range(hotspot.dhcp_range.as_deref().unwrap_or(DEFAULT_DHCP_RANGE)).map_err(
		|reason| {
			invalid(
				&[Segment::Name("hotspot"), Segment::Name("dhcp-range")],
				reason,
			)
		},
	)?;
	let share = hotspot.share_upstream.unwrap_or(true);
	let (masquerade, yes_no) = if share { ("ipv4", "yes") } else { ("no", "no") };

	let mut out = header("the hotspot");
	let _ = write!(
		out,
		"\n[Match]\nName={interface}\n\
		 \n[Network]\nAddress={server}\nDHCPServer=yes\nIPMasquerade={masquerade}\n\
		 IPv4Forwarding={yes_no}\nIPv6AcceptRA=no\n\
		 \n[DHCPServer]\nPoolOffset={offset}\nPoolSize={size}\nEmitRouter={yes_no}\nEmitDNS={yes_no}\n",
		server = pool.server,
		offset = pool.offset,
		size = pool.size,
	);
	Ok(File {
		path: path(paths, interface),
		contents: out,
		mode: PUBLIC,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	/// The device takes the first host and leases every other address short of broadcast.
	#[test]
	fn a_range_leases_all_but_the_device() {
		let pool = dhcp_range("10.41.0.0/24").unwrap();
		assert_eq!(pool.server.to_string(), "10.41.0.1/24");
		assert_eq!((pool.offset, pool.size), (2, 253));

		let pool = dhcp_range("192.0.2.0/30").unwrap();
		assert_eq!(pool.server.to_string(), "192.0.2.1/30");
		assert_eq!(pool.size, 1);
	}

	/// A range that is not an IPv4 subnet, names a host, or leaves nothing to lease is refused.
	#[test]
	fn a_bad_range_is_refused() {
		for range in [
			"10.41.0.0",
			"fd00::/64",
			"10.41.0.7/24",
			"10.41.0.0/31",
			"nope",
		] {
			assert!(dhcp_range(range).is_err(), "{range}");
		}
	}

	/// The default range is itself one the device accepts.
	#[test]
	fn the_default_range_parses() {
		assert!(dhcp_range(DEFAULT_DHCP_RANGE).is_ok());
	}

	/// Lower ranks carry lower metrics.
	#[test]
	fn metrics_follow_rank() {
		assert!(metric(0) < metric(1));
		assert_eq!(metric(usize::MAX), u32::MAX);
	}
}
