//! A configuration document rendered as the files iwd, hostapd and systemd-networkd read.
//!
//! Pure: nothing here touches the filesystem, starts a process or speaks D-Bus. [`render`] turns a
//! document, the hardware it runs on and the candidates selected on it into the complete set of files
//! bliti owns for that state. bliti owns these outright, so an applier writes every [`File`] rendered
//! and deletes every file [`Paths::owns`] claims that was not.

use std::{
	collections::{BTreeMap, BTreeSet},
	ffi::OsStr,
	path::{Path, PathBuf},
};

use bliti_core::channel::config::{
	Attachment, AttachmentKind, Document, Hotspot, Invalid, Segment, path,
};

use super::select::Alongside;

pub(crate) use iwd::SAE_DISABLED;

mod hostapd;
mod iwd;
mod networkd;
mod regdom;

/// The mode of a file anyone on the device may read.
pub const PUBLIC: u32 = 0o644;

/// The mode of a file carrying a secret.
pub const SECRET: u32 = 0o600;

/// What a document is rendered onto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hardware {
	/// The wired interfaces a candidate may name.
	pub wired: Vec<String>,
	/// The wireless radios, in the order the probe found them.
	pub radios: Vec<Radio>,
	/// Where each backend reads its files.
	pub paths: Paths,
}

/// One wireless radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Radio {
	/// The interface iwd runs the wireless client on, which is how a document names the radio.
	pub station: String,
	/// The access point interface bliti creates on it for hostapd, where it can run one.
	pub access_point: Option<AccessPoint>,
}

/// The access point interface bliti creates on a radio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPoint {
	/// Its name, as [`access_point`] gives it.
	pub interface: String,
	/// How the radio runs it beside its wireless client (HOT).
	pub alongside: Alongside,
}

/// The access point interface bliti creates on the radio at `index` among those probed: `ap0`,
/// `ap1` and so on, which `services/bliti-iwd-dropin.conf` keeps iwd off.
pub fn access_point(index: usize) -> String {
	format!("ap{index}")
}

/// Where each backend reads the files bliti renders for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
	/// systemd-networkd's configuration directory.
	pub networkd: PathBuf,
	/// iwd's state directory, where it reads its known networks.
	pub iwd_state: PathBuf,
	/// iwd's main configuration file.
	pub iwd_config: PathBuf,
	/// The directory of hostapd's configurations, one per access point interface, each named for
	/// its interface with [`Paths::hostapd_conf`].
	pub hostapd: PathBuf,
	/// The modprobe configuration carrying the kernel's regulatory domain.
	pub modprobe: PathBuf,
	/// The directory systemd-resolved reads DNS delegates from.
	pub resolved: PathBuf,
}

impl Hardware {
	/// The radio whose station interface is `station`.
	pub fn radio(&self, station: &str) -> Option<&Radio> {
		self.radios.iter().find(|radio| radio.station == station)
	}

	/// The station interface of the radio bliti creates the access point interface `interface`
	/// on, where it creates one of that name.
	pub fn access_point_radio(&self, interface: &str) -> Option<&str> {
		self.radios
			.iter()
			.find(|radio| {
				radio.station != interface
					&& radio
						.access_point
						.as_ref()
						.is_some_and(|ap| ap.interface == interface)
			})
			.map(|radio| radio.station.as_str())
	}
}

impl Paths {
	/// Where a device reads each file: all of them under `/run`, which does not survive a reboot.
	///
	/// A proposal is applied to the running system and recorded nowhere (CFG), and the files a render
	/// writes are that running system. In `/etc` a proposal would outlive a power cut, and the stack
	/// would bring it up at the next boot before bliti restored the recorded configuration. Under
	/// `/run` a reboot leaves nothing, and bliti renders the recorded configuration as it starts.
	/// iwd is pointed at its two directories by the unit drop-in in `services/`.
	pub fn system() -> Self {
		Self {
			networkd: "/run/systemd/network".into(),
			iwd_state: "/run/bliti/iwd".into(),
			iwd_config: "/run/bliti/iwd-config/main.conf".into(),
			hostapd: "/run/bliti/hostapd".into(),
			modprobe: "/run/modprobe.d/bliti-regdom.conf".into(),
			resolved: "/run/systemd/dns-delegate.d".into(),
		}
	}

	/// Whether `path` is a file bliti owns, which an applier deletes when a render no longer holds it.
	///
	/// In networkd's directory that is the files named with bliti's prefix. iwd names its network files
	/// after the SSID, leaving no room for a prefix, so bliti owns every network file in iwd's state
	/// directory.
	pub fn owns(&self, path: &Path) -> bool {
		if path == self.iwd_config || path == self.modprobe {
			return true;
		}
		let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(OsStr::to_str))
		else {
			return false;
		};
		(dir == self.networkd && networkd::owns(name))
			|| (dir == self.iwd_state && iwd::owns(name))
			|| (dir == self.resolved && networkd::owns_delegate(name))
			|| self.hostapd_interface(path).is_some()
	}

	/// The configuration hostapd runs the access point interface `interface` on, which
	/// `services/bliti-hostapd@.service` reads for its instance of that name.
	pub fn hostapd_conf(&self, interface: &str) -> PathBuf {
		self.hostapd.join(format!("{interface}.conf"))
	}

	/// The access point interface a hostapd configuration at `path` runs, where it is one.
	pub fn hostapd_interface<'a>(&self, path: &'a Path) -> Option<&'a str> {
		if path.parent() != Some(&self.hostapd) {
			return None;
		}
		let name = path.file_name()?.to_str()?;
		name.strip_suffix(".conf")
			.filter(|interface| !interface.is_empty() && !interface.starts_with('.'))
	}
}

/// Which candidates are up, on which interfaces, and what each radio is doing, in the state being
/// rendered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
	/// The candidate each interface brings up, by index into `attachments`: a wired candidate on its
	/// own interface, a wireless one on its radio's station interface. At most one per interface
	/// (LINK), so of several statics on one interface this names the one being tried.
	pub links: BTreeMap<String, usize>,
	/// The radio running the hotspot, by its station interface, or `None` where none runs it. A
	/// hotspot waiting for the wireless client sharing its radio's channel to associate runs on
	/// none: started first, it would hold the radio on a channel of its own and the client could
	/// join only there.
	pub hotspot: Option<String>,
	/// The channel each radio's wireless client is associated on, by its station interface, which a
	/// shared-channel radio's hotspot follows (HOT).
	pub channels: BTreeMap<String, Channel>,
}

/// A 20 MHz channel on a band.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Channel {
	/// The band.
	pub band: Band,
	/// The channel number within it.
	pub number: u32,
}

/// A band the hotspot operates on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
	/// 2.4 GHz.
	TwoPointFour,
	/// 5 GHz.
	Five,
}

/// The complete set of files bliti owns for one state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
	/// Every file, sorted by path.
	pub files: Vec<File>,
}

/// One file to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
	/// Where it goes.
	pub path: PathBuf,
	/// What it holds.
	pub contents: String,
	/// Its permission bits: [`SECRET`] where it carries a secret, else [`PUBLIC`].
	pub mode: u32,
}

/// Why a state could not be rendered.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
	/// The document cannot be carried out on this hardware, and the fault is the document's.
	#[error("{reason} (at {at})", reason = .0.reason, at = .0.at)]
	Invalid(Invalid),
	/// The selection does not fit the document, which is the caller's fault rather than the operator's.
	#[error("the selection does not fit the document: {0}")]
	Selection(String),
}

impl From<Invalid> for Error {
	fn from(invalid: Invalid) -> Self {
		Self::Invalid(invalid)
	}
}

/// Render `document` on `hardware` with `selection` up, as the complete set of files bliti owns.
///
/// Every candidate and the hotspot are checked whether or not they are selected, so a document that
/// could not be carried out in some state is refused in every state.
pub fn render(
	document: &Document,
	hardware: &Hardware,
	selection: &Selection,
) -> Result<Rendered, Error> {
	let domain = regdom::domain(document)?;
	radios(document, hardware)?;

	let mut files = iwd::networks(document, hardware)?;
	let networks = document
		.attachments
		.iter()
		.enumerate()
		.map(|(rank, attachment)| {
			networkd::candidate(attachment, rank, checked_on(hardware, attachment), hardware)
		})
		.collect::<Result<Vec<_>, _>>()?;

	if let Some(hotspot) = &document.hotspot {
		files.extend(self::hotspot(hotspot, hardware, selection, domain)?);
	}

	let mut placed = BTreeSet::new();
	for (interface, &index) in &selection.links {
		let Some(attachment) = document.attachments.get(index) else {
			return Err(Error::Selection(format!(
				"candidate {index} is not in a document of {}",
				document.attachments.len()
			)));
		};
		if !placed.insert(index) {
			return Err(Error::Selection(format!(
				"candidate {index} is selected on more than one interface"
			)));
		}
		if !attachment.enabled {
			return Err(Error::Selection(format!(
				"candidate {index} is turned off and is selected on {interface:?}"
			)));
		}
		match &attachment.kind {
			AttachmentKind::Wireless(wireless) => {
				if hardware.radio(interface).is_none() {
					return Err(Error::Selection(format!(
						"candidate {index} is selected on {interface:?}, which is not a radio"
					)));
				}
				if let Some(pin) = wireless.interface.as_deref()
					&& pin != interface
				{
					return Err(Error::Selection(format!(
						"candidate {index} names {pin:?} and is selected on {interface:?}"
					)));
				}
				files.extend(networkd::candidate(
					attachment,
					index,
					Some(interface),
					hardware,
				)?);
			}
			AttachmentKind::WiredDynamic { interface: own }
			| AttachmentKind::WiredStatic { interface: own, .. } => {
				if own != interface {
					return Err(Error::Selection(format!(
						"candidate {index} is on {own:?} and is selected on {interface:?}"
					)));
				}
				files.extend(networks[index].iter().cloned());
			}
		}
	}

	let mut idle: Vec<&str> = document
		.attachments
		.iter()
		.filter_map(|attachment| match &attachment.kind {
			AttachmentKind::WiredDynamic { interface }
			| AttachmentKind::WiredStatic { interface, .. } => Some(interface.as_str()),
			AttachmentKind::Wireless(_) => None,
		})
		.filter(|interface| !selection.links.contains_key(*interface))
		.collect();
	idle.sort_unstable();
	idle.dedup();
	files.extend(
		idle.into_iter()
			.map(|interface| networkd::idle(interface, &hardware.paths)),
	);

	if !hardware.radios.is_empty() {
		files.push(iwd::main_conf(&hardware.paths, domain));
		files.push(regdom::modprobe(&hardware.paths, domain));
	}

	files.sort_by(|a, b| a.path.cmp(&b.path));
	Ok(Rendered { files })
}

/// The station a wireless candidate's link is checked on whatever it is selected on: the radio it
/// names, else the first.
fn checked_on<'a>(hardware: &'a Hardware, attachment: &'a Attachment) -> Option<&'a str> {
	match &attachment.kind {
		AttachmentKind::Wireless(wireless) => wireless
			.interface
			.as_deref()
			.or_else(|| hardware.radios.first().map(|radio| radio.station.as_str())),
		_ => None,
	}
}

/// The hotspot's files on the radio the selection runs it on, where it runs on one.
///
/// It is checked first on the radio it names, else the first able to run one, with no client's
/// channel to follow, so that what it chooses for itself is checked in every state.
fn hotspot(
	hotspot: &Hotspot,
	hardware: &Hardware,
	selection: &Selection,
	domain: Option<&str>,
) -> Result<Vec<File>, Error> {
	let paths = &hardware.paths;
	let checked = hardware
		.radios
		.iter()
		.filter(|radio| {
			hotspot
				.interface
				.as_deref()
				.is_none_or(|pin| pin == radio.station)
		})
		.find_map(|radio| radio.access_point.as_ref())
		.ok_or_else(|| {
			invalid(
				&[Segment::Name("hotspot")],
				"this device cannot run a hotspot",
			)
		})?;
	networkd::hotspot(hotspot, &checked.interface, paths)?;
	hostapd::conf(hotspot, checked, None, paths, domain)?;

	let Some(station) = &selection.hotspot else {
		return Ok(Vec::new());
	};
	let Some(access_point) = hardware
		.radio(station)
		.and_then(|radio| radio.access_point.as_ref())
	else {
		return Err(Error::Selection(format!(
			"the hotspot is selected on {station:?}, which cannot run one"
		)));
	};
	if let Some(pin) = hotspot.interface.as_deref()
		&& pin != station
	{
		return Err(Error::Selection(format!(
			"the hotspot names {pin:?} and is selected on {station:?}"
		)));
	}
	Ok(vec![
		networkd::hotspot(hotspot, &access_point.interface, paths)?,
		hostapd::conf(
			hotspot,
			access_point,
			selection.channels.get(station).copied(),
			paths,
			domain,
		)?,
	])
}

/// Refuse a wireless candidate or hotspot naming a radio this hardware does not have, or a hotspot
/// naming one that cannot run it: nothing here could drive it (LINK, HOT), and carrying it on
/// another radio would run something other than what was written.
fn radios(document: &Document, hardware: &Hardware) -> Result<(), Invalid> {
	for (rank, attachment) in document.attachments.iter().enumerate() {
		if let AttachmentKind::Wireless(wireless) = &attachment.kind
			&& let Some(interface) = wireless.interface.as_deref()
			&& hardware.radio(interface).is_none()
		{
			return Err(invalid_in(
				rank,
				&[Segment::Name("interface")],
				format!("{interface:?} is not a wireless interface on this device"),
			));
		}
	}
	if let Some(interface) = document
		.hotspot
		.as_ref()
		.and_then(|hotspot| hotspot.interface.as_deref())
	{
		let at = [Segment::Name("hotspot"), Segment::Name("interface")];
		match hardware.radio(interface) {
			None => {
				return Err(invalid(
					&at,
					format!("{interface:?} is not a wireless interface on this device"),
				));
			}
			Some(radio) if radio.access_point.is_none() => {
				return Err(invalid(&at, format!("{interface} cannot run a hotspot")));
			}
			Some(_) => {}
		}
	}
	Ok(())
}

/// The 20 MHz channels the hotspot occupies on `channel` of `band` at `width`, as the band is named
/// on the wire, where the renderer renders that pair.
pub fn hotspot_span(band: &str, channel: u32, width: u32) -> Option<(Band, Vec<u32>)> {
	let band = hostapd::band(band).ok()?;
	hostapd::exists(band, channel).then_some(())?;
	let span = hostapd::span(
		Channel {
			band,
			number: channel,
		},
		width,
	)?;
	Some((band, span))
}

/// Whether the hotspot renders on `channel` of `band`, as the band is named on the wire.
///
/// The one list of channels hostapd is rendered on, so capabilities never offer a channel the
/// renderer would then refuse.
pub(super) fn hotspot_channel(band: &str, channel: u32) -> bool {
	hostapd::band(band).is_ok_and(|band| hostapd::exists(band, channel))
}

/// Where iwd keeps the pre-shared-key network `ssid`, as a WPS join leaves it.
pub fn known_psk(paths: &Paths, ssid: &str) -> PathBuf {
	paths
		.iwd_state
		.join(format!("{}.psk", iwd::encode_ssid(ssid)))
}

/// The passphrase a pre-shared-key network file holds, where it holds one rather than only a raw
/// key.
pub fn known_passphrase(contents: &str) -> Option<String> {
	iwd::passphrase(contents)
}

/// A fault found in the document before anything was applied, at the node `at` names.
fn invalid(at: &[Segment<'_>], reason: impl Into<String>) -> Invalid {
	Invalid {
		at: path(at),
		reason: reason.into(),
		reached: None,
	}
}

/// The Normalized Path of the candidate at `index`, as a rendered file names where it came from.
fn candidate_path(index: usize) -> String {
	path(&[Segment::Name("attachments"), Segment::Index(index)])
}

/// A fault in a member of the candidate at `index`, named by the segments below it.
fn invalid_in(index: usize, below: &[Segment<'_>], reason: impl Into<String>) -> Invalid {
	let mut at = vec![Segment::Name("attachments"), Segment::Index(index)];
	at.extend_from_slice(below);
	invalid(&at, reason)
}

/// The first line of every file bliti renders, saying whose it is.
fn header(from: &str) -> String {
	format!(
		"# Written by bliti from {from}. bliti owns this file and overwrites any change to it.\n"
	)
}

#[cfg(test)]
mod tests;
