//! Asking a link's gateway whether it is there: an ARP request for an IPv4 gateway, an ICMPv6 echo
//! for an IPv6 one, each sent on the link alone.
//!
//! ARP rather than an ICMP echo for IPv4 because every IPv4 router on a link answers ARP, and plenty
//! drop echo requests. A few tries a second apart, then the gateway is taken not to answer.

use std::{
	fs,
	io::{self, Read as _},
	net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddrV6},
	time::Duration,
};

use futures::future::BoxFuture;
use socket2::{Domain, Protocol, SockAddr, SockAddrStorage, Socket, Type};
use tokio::io::unix::AsyncFd;

/// How many times the gateway is asked.
const TRIES: u32 = 3;

/// How long each ask waits for the answer.
const WAIT: Duration = Duration::from_secs(1);

/// The gateway probe, over raw sockets. The daemon runs as root, which is what opening them takes.
pub(super) struct Probe;

impl super::Gateway for Probe {
	fn probe(
		&self,
		interface: &str,
		source: IpAddr,
		gateway: IpAddr,
	) -> BoxFuture<'static, Result<(), String>> {
		let interface = interface.to_owned();
		Box::pin(async move {
			let asked = match (source, gateway) {
				(IpAddr::V4(source), IpAddr::V4(gateway)) => arp(&interface, source, gateway).await,
				(_, IpAddr::V6(gateway)) => echo(&interface, gateway).await,
				(IpAddr::V6(_), IpAddr::V4(_)) => Err(io::Error::other(
					"an IPv4 gateway is asked from an IPv4 address",
				)),
			};
			match asked {
				Ok(true) => Ok(()),
				Ok(false) => Err(format!(
					"the gateway {gateway} did not answer on {interface}"
				)),
				Err(error) => Err(format!(
					"the gateway {gateway} could not be asked on {interface}: {error}"
				)),
			}
		})
	}
}

fn index(interface: &str) -> io::Result<u32> {
	fs::read_to_string(format!("/sys/class/net/{interface}/ifindex"))?
		.trim()
		.parse()
		.map_err(io::Error::other)
}

fn hardware_address(interface: &str) -> io::Result<[u8; 6]> {
	let text = fs::read_to_string(format!("/sys/class/net/{interface}/address"))?;
	let bytes: Vec<u8> = text
		.trim()
		.split(':')
		.map(|byte| u8::from_str_radix(byte, 16))
		.collect::<Result<_, _>>()
		.map_err(io::Error::other)?;
	bytes
		.try_into()
		.map_err(|_| io::Error::other(format!("{interface} has no Ethernet address")))
}

/// A link-layer address on `interface`, for the ARP socket to bind to or send to.
fn link_address(index: u32, to: Option<[u8; 6]>) -> SockAddr {
	let mut storage = SockAddrStorage::zeroed();
	// SAFETY: `sockaddr_storage` is large and aligned enough for any address, `sockaddr_ll` among
	// them, and every field of the zeroed storage is valid as an integer.
	let ll = unsafe { storage.view_as::<libc::sockaddr_ll>() };
	ll.sll_family = libc::AF_PACKET as u16;
	ll.sll_protocol = (libc::ETH_P_ARP as u16).to_be();
	ll.sll_ifindex = index as i32;
	if let Some(to) = to {
		ll.sll_halen = 6;
		ll.sll_addr[..6].copy_from_slice(&to);
	}
	// SAFETY: the storage holds an initialised `sockaddr_ll`, and the length says so.
	unsafe { SockAddr::new(storage, size_of::<libc::sockaddr_ll>() as libc::socklen_t) }
}

/// Ask for `gateway`'s hardware address by ARP, from `source`, until it answers or the tries run out.
async fn arp(interface: &str, source: Ipv4Addr, gateway: Ipv4Addr) -> io::Result<bool> {
	let index = index(interface)?;
	let own = hardware_address(interface)?;
	let socket = Socket::new(
		Domain::PACKET,
		Type::DGRAM.nonblocking(),
		Some(Protocol::from(i32::from((libc::ETH_P_ARP as u16).to_be()))),
	)?;
	socket.bind(&link_address(index, None))?;
	let socket = AsyncFd::new(socket)?;

	let mut request = Vec::with_capacity(28);
	request.extend_from_slice(&[0, 1, 8, 0, 6, 4, 0, 1]);
	request.extend_from_slice(&own);
	request.extend_from_slice(&source.octets());
	request.extend_from_slice(&[0; 6]);
	request.extend_from_slice(&gateway.octets());
	let broadcast = link_address(index, Some([0xff; 6]));

	ask(
		&socket,
		|socket| socket.send_to(&request, &broadcast),
		|reply| reply.len() >= 28 && reply[6..8] == [0, 2] && reply[14..18] == gateway.octets(),
	)
	.await
}

/// Ask `gateway` for an echo over ICMPv6, until it answers or the tries run out.
async fn echo(interface: &str, gateway: Ipv6Addr) -> io::Result<bool> {
	let index = index(interface)?;
	let socket = Socket::new(
		Domain::IPV6,
		Type::RAW.nonblocking(),
		Some(Protocol::ICMPV6),
	)?;
	socket.bind_device(Some(interface.as_bytes()))?;
	let socket = AsyncFd::new(socket)?;
	let link_local = gateway.segments()[0] & 0xffc0 == 0xfe80;
	let to = SockAddr::from(SocketAddrV6::new(
		gateway,
		0,
		0,
		if link_local { index } else { 0 },
	));
	let id = (std::process::id() as u16).to_be_bytes();
	// The kernel fills in an ICMPv6 checksum itself.
	let request = [128, 0, 0, 0, id[0], id[1], 0, 1];

	ask(
		&socket,
		|socket| socket.send_to(&request, &to),
		|reply| reply.len() >= 8 && reply[0] == 129 && reply[4..6] == id,
	)
	.await
}

/// Send with `send` up to [`TRIES`] times, waiting [`WAIT`] after each for a datagram `answers`.
async fn ask(
	socket: &AsyncFd<Socket>,
	send: impl Fn(&Socket) -> io::Result<usize>,
	answers: impl Fn(&[u8]) -> bool,
) -> io::Result<bool> {
	for _ in 0..TRIES {
		send(socket.get_ref())?;
		let heard = tokio::time::timeout(WAIT, async {
			let mut buffer = [0u8; 1500];
			loop {
				let mut ready = socket.readable().await?;
				match ready.try_io(|socket| socket.get_ref().read(&mut buffer)) {
					Ok(Ok(len)) if answers(&buffer[..len]) => return io::Result::Ok(()),
					Ok(Ok(_)) | Err(_) => {}
					Ok(Err(error)) => return Err(error),
				}
			}
		})
		.await;
		match heard {
			Ok(Ok(())) => return Ok(true),
			Ok(Err(error)) => return Err(error),
			Err(_) => {}
		}
	}
	Ok(false)
}
