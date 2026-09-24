//! A configuration document rendered as the files iwd, hostapd and systemd-networkd read.
//!
//! Pure: nothing here touches the filesystem, starts a process or speaks D-Bus. [`render`] turns a
//! document, the hardware it runs on and the candidates selected on it into the complete set of files
//! bliti owns for that state. bliti owns these outright, so an applier writes every [`File`] rendered
//! and deletes every file [`Paths::owns`] claims that was not.

use std::{
	ffi::OsStr,
	path::{Path, PathBuf},
};

use bliti_core::channel::config::{AttachmentKind, Document, Invalid, Segment, path};

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
	/// The interface iwd runs the wireless client on, where the device has a radio.
	pub station: Option<String>,
	/// The interface hostapd runs the hotspot on, where the radio can run one.
	pub access_point: Option<String>,
	/// Whether the radio runs its access point and its client on one channel between them (HOT).
	pub shared_channel: bool,
	/// Where each backend reads its files.
	pub paths: Paths,
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
	/// The configuration file hostapd is started with.
	pub hostapd: PathBuf,
	/// The modprobe configuration carrying the kernel's regulatory domain.
	pub modprobe: PathBuf,
	/// The directory systemd-resolved reads DNS delegates from.
	pub resolved: PathBuf,
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
			hostapd: "/run/bliti/hostapd.conf".into(),
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
		if path == self.iwd_config || path == self.hostapd || path == self.modprobe {
			return true;
		}
		let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(OsStr::to_str))
		else {
			return false;
		};
		(dir == self.networkd && networkd::owns(name))
			|| (dir == self.iwd_state && iwd::owns(name))
			|| (dir == self.resolved && networkd::owns_delegate(name))
	}
}

/// Which candidates are up, and what the radio is doing, in the state being rendered.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
	/// The candidates brought up, by index into `attachments`. At most one per interface (LINK), so of
	/// several statics on one interface this names the one being tried.
	pub active: Vec<usize>,
	/// The channel the wireless client is associated on, which a shared-channel radio's hotspot has
	/// to follow (HOT).
	pub station_channel: Option<Channel>,
	/// Whether the hotspot waits for the wireless client sharing its radio's channel to associate.
	/// Started first, it would hold the radio on a channel of its own and the client could join only
	/// there.
	pub hotspot_waits: bool,
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
/// Every candidate is checked whether or not it is selected, so a document that could not be carried
/// out in some state is refused in every state.
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
		.map(|(rank, attachment)| networkd::candidate(attachment, rank, hardware))
		.collect::<Result<Vec<_>, _>>()?;

	if let Some(hotspot) = &document.hotspot {
		let Some(interface) = hardware.access_point.as_deref() else {
			return Err(invalid(
				&[Segment::Name("hotspot")],
				"this device cannot run a hotspot",
			)
			.into());
		};
		let network = networkd::hotspot(hotspot, interface, &hardware.paths)?;
		let conf = hostapd::conf(
			hotspot,
			interface,
			hardware,
			selection.station_channel,
			domain,
		)?;
		if !selection.hotspot_waits {
			files.push(network);
			files.push(conf);
		}
	}

	let mut interfaces = Vec::new();
	for &index in &selection.active {
		let Some(attachment) = document.attachments.get(index) else {
			return Err(Error::Selection(format!(
				"candidate {index} is not in a document of {}",
				document.attachments.len()
			)));
		};
		let interface = match &attachment.kind {
			AttachmentKind::Wireless(_) => hardware.station.as_deref().unwrap_or_default(),
			AttachmentKind::WiredDynamic { interface }
			| AttachmentKind::WiredStatic { interface, .. } => interface,
		};
		if interfaces.contains(&interface) {
			return Err(Error::Selection(format!(
				"more than one candidate is selected on {interface:?}"
			)));
		}
		interfaces.push(interface);
		files.extend(networks[index].iter().cloned());
	}

	let mut idle: Vec<&str> = document
		.attachments
		.iter()
		.filter_map(|attachment| match &attachment.kind {
			AttachmentKind::WiredDynamic { interface }
			| AttachmentKind::WiredStatic { interface, .. } => Some(interface.as_str()),
			AttachmentKind::Wireless(_) => None,
		})
		.filter(|interface| !interfaces.contains(interface))
		.collect();
	idle.sort_unstable();
	idle.dedup();
	files.extend(
		idle.into_iter()
			.map(|interface| networkd::idle(interface, &hardware.paths)),
	);

	if hardware.station.is_some() {
		files.push(iwd::main_conf(&hardware.paths, domain));
	}
	if hardware.station.is_some() || hardware.access_point.is_some() {
		files.push(regdom::modprobe(&hardware.paths, domain));
	}

	files.sort_by(|a, b| a.path.cmp(&b.path));
	Ok(Rendered { files })
}

/// Refuse a wireless candidate or hotspot naming a radio other than the one this hardware has.
///
/// One radio is all [`Hardware`] describes, so a name that is not its station interface is an adapter
/// nothing here could drive (LINK, HOT). Carrying it anyway on the one radio there is would run
/// something other than what was written.
fn radios(document: &Document, hardware: &Hardware) -> Result<(), Invalid> {
	let named = |interface: &str| hardware.station.as_deref() == Some(interface);
	for (rank, attachment) in document.attachments.iter().enumerate() {
		if let AttachmentKind::Wireless(wireless) = &attachment.kind
			&& let Some(interface) = wireless.interface.as_deref()
			&& !named(interface)
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
		&& !named(interface)
	{
		return Err(invalid(
			&[Segment::Name("hotspot"), Segment::Name("interface")],
			format!("{interface:?} is not a wireless interface on this device"),
		));
	}
	Ok(())
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
