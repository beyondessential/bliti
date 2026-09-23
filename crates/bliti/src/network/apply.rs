//! Putting a rendered configuration in place and having the stack pick it up.
//!
//! [`apply`] writes every file a render holds, deletes every file bliti owns that it no longer holds,
//! and then has each backend pick up what changed through a [`System`]. It goes backend by backend
//! in the order the stack needs: the regulatory domain first, then the hotspot's interface and
//! hostapd, then the wireless client, then networkd. A backend none of whose files changed is not
//! touched at all.
//!
//! Whether a file changed is decided against a record of what bliti itself last put in place, not
//! against the file as it stands: iwd writes back into its known-network files, so their contents
//! drift from what was rendered without anything having changed. The record lives in a runtime
//! directory, [`STATE`] on a device, so it goes with a reboot along with the hotspot's interface,
//! the running hostapd and the runtime regulatory domain it describes. The first apply of every
//! boot therefore writes and picks up everything.
//!
//! Blocking: it does file I/O and waits on systemd jobs, so the daemon runs it off the async
//! runtime.

use std::{
	fmt, io,
	path::{Path, PathBuf},
};

use super::render::{File, Hardware, Paths, Rendered};

use record::Record;

mod files;
#[cfg(target_os = "linux")]
mod iw;
mod record;
#[cfg(target_os = "linux")]
mod system;
#[cfg(test)]
mod tests;

#[cfg(target_os = "linux")]
pub use system::Linux;

/// Where a device keeps the record of what bliti last put in place.
pub const STATE: &str = "/run/bliti/network";

/// A daemon whose files bliti renders, in the order [`apply`] has them pick changes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Backend {
	/// The kernel's regulatory domain, from the modprobe file.
	Regdom,
	/// hostapd, running the hotspot, and the interface it runs on.
	Hostapd,
	/// iwd, running the wireless client.
	Iwd,
	/// systemd-networkd, addressing every link.
	Networkd,
	/// systemd-resolved, reading the DNS delegates carrying a link's own resolvers. After networkd,
	/// since a delegate is bound to a link networkd brings up.
	Resolved,
}

impl Backend {
	/// Every backend, in the order they pick changes up.
	const ORDER: [Self; 5] = [
		Self::Regdom,
		Self::Hostapd,
		Self::Iwd,
		Self::Networkd,
		Self::Resolved,
	];

	/// The backend reading `path`, where it is a file bliti owns.
	fn of(paths: &Paths, path: &Path) -> Option<Self> {
		if !paths.owns(path) {
			None
		} else if path == paths.modprobe {
			Some(Self::Regdom)
		} else if path == paths.hostapd {
			Some(Self::Hostapd)
		} else if path == paths.iwd_config || path.parent() == Some(&paths.iwd_state) {
			Some(Self::Iwd)
		} else if path.parent() == Some(&paths.resolved) {
			Some(Self::Resolved)
		} else {
			Some(Self::Networkd)
		}
	}
}

impl fmt::Display for Backend {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(match self {
			Self::Regdom => "the regulatory domain",
			Self::Hostapd => "hostapd",
			Self::Iwd => "iwd",
			Self::Networkd => "systemd-networkd",
			Self::Resolved => "systemd-resolved",
		})
	}
}

/// One file put in place or taken away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
	/// The file was written with new contents or a new mode, or was missing.
	Written(PathBuf),
	/// The file was bliti's and the render no longer holds it.
	Removed(PathBuf),
}

/// Exactly the files an apply changed, by the backend reading them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
	/// The modprobe file.
	pub regdom: Vec<Change>,
	/// hostapd's configuration.
	pub hostapd: Vec<Change>,
	/// iwd's main configuration and its known networks.
	pub iwd: Vec<Change>,
	/// networkd's `.network` files.
	pub networkd: Vec<Change>,
	/// resolved's DNS delegates.
	pub resolved: Vec<Change>,
}

impl Changes {
	/// Whether nothing changed.
	#[cfg_attr(
		not(test),
		expect(dead_code, reason = "wired in by the backend that applies selections")
	)]
	pub fn is_empty(&self) -> bool {
		Backend::ORDER
			.into_iter()
			.all(|backend| self.get(backend).is_empty())
	}

	/// The files `backend` reads that changed.
	pub fn get(&self, backend: Backend) -> &[Change] {
		match backend {
			Backend::Regdom => &self.regdom,
			Backend::Hostapd => &self.hostapd,
			Backend::Iwd => &self.iwd,
			Backend::Networkd => &self.networkd,
			Backend::Resolved => &self.resolved,
		}
	}

	fn get_mut(&mut self, backend: Backend) -> &mut Vec<Change> {
		match backend {
			Backend::Regdom => &mut self.regdom,
			Backend::Hostapd => &mut self.hostapd,
			Backend::Iwd => &mut self.iwd,
			Backend::Networkd => &mut self.networkd,
			Backend::Resolved => &mut self.resolved,
		}
	}
}

/// What to do with hostapd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hostapd {
	/// Bring the hotspot up, where it was not.
	Start,
	/// Bring the hotspot up again on a changed configuration.
	Restart,
	/// Take the hotspot down.
	Stop,
}

/// What has the stack pick changes up, so tests can stand in for the device.
pub trait System {
	/// Put the radio under `domain` at runtime, `00` being the world domain.
	fn set_regulatory_domain(&mut self, domain: &str) -> anyhow::Result<()>;

	/// Create the access point interface `interface` on the radio whose station interface is
	/// `radio`, doing nothing where it exists.
	fn create_access_point(&mut self, radio: &str, interface: &str) -> anyhow::Result<()>;

	/// Delete the access point interface `interface`, doing nothing where it does not exist.
	fn delete_access_point(&mut self, interface: &str) -> anyhow::Result<()>;

	/// Start, restart or stop hostapd, returning once it has done so.
	fn hostapd(&mut self, action: Hostapd) -> anyhow::Result<()>;

	/// Restart iwd, so it reads its main configuration again, returning once it is up.
	fn restart_iwd(&mut self) -> anyhow::Result<()>;

	/// Have networkd read its configuration again and reconfigure the links it changed for.
	fn reload_networkd(&mut self) -> anyhow::Result<()>;

	/// Have resolved read its configuration again, DNS delegates included.
	fn reload_resolved(&mut self) -> anyhow::Result<()>;
}

/// Why an apply stopped.
///
/// A backend that fails leaves the backends after it untouched, and is left out of the record, so
/// the next apply writes its files and picks them up again.
#[derive(Debug, thiserror::Error)]
pub enum Error {
	/// A rendered file is not one bliti owns, which is the renderer's fault.
	#[error("{0:?} is not a file bliti owns")]
	Foreign(PathBuf),
	/// A file could not be put in place or taken away.
	#[error("{backend}: {path:?}: {source}")]
	File {
		/// The backend reading it.
		backend: Backend,
		/// The file.
		path: PathBuf,
		/// What went wrong.
		source: io::Error,
	},
	/// A backend could not be had to pick its changes up.
	#[error("{backend}: {source:#}")]
	System {
		/// The backend.
		backend: Backend,
		/// What went wrong.
		source: anyhow::Error,
	},
	/// The record of what was put in place could not be written.
	#[error("the record at {path:?}: {source}")]
	Record {
		/// The record's file.
		path: PathBuf,
		/// What went wrong.
		source: io::Error,
	},
}

/// Put `rendered` in place on `hardware` and have the stack pick it up, keeping the record of what
/// was put in place under `state`.
///
/// Returns exactly the files that changed. Where nothing did, nothing is written and `system` is not
/// called.
pub fn apply(
	rendered: &Rendered,
	hardware: &Hardware,
	state: &Path,
	system: &mut dyn System,
) -> Result<Changes, Error> {
	let paths = &hardware.paths;
	let mut wanted: Vec<(Backend, &File)> = Vec::with_capacity(rendered.files.len());
	for file in &rendered.files {
		let backend =
			Backend::of(paths, &file.path).ok_or_else(|| Error::Foreign(file.path.clone()))?;
		wanted.push((backend, file));
	}

	let mut record = Record::load(state);
	record.retain(|path| paths.owns(path));

	let mut stale: Vec<PathBuf> = files::owned(paths);
	stale.extend(record.paths().map(Path::to_path_buf));
	stale.retain(|path| !rendered.files.iter().any(|file| file.path == *path));
	stale.sort();
	stale.dedup();

	let mut changes = Changes::default();
	for backend in Backend::ORDER {
		let written: Vec<&File> = wanted
			.iter()
			.filter(|(of, file)| {
				*of == backend && !(record.holds(file) && files::exists(&file.path))
			})
			.map(|(_, file)| *file)
			.collect();
		let removed: Vec<&Path> = stale
			.iter()
			.map(PathBuf::as_path)
			.filter(|path| Backend::of(paths, path) == Some(backend))
			.collect();
		if written.is_empty() && removed.is_empty() {
			continue;
		}

		let step = Step {
			backend,
			hardware,
			record: &record,
			written: &written,
			removed: &removed,
		};
		step.run(system)?;

		for file in written {
			record.insert(file);
			changes
				.get_mut(backend)
				.push(Change::Written(file.path.clone()));
		}
		for path in removed {
			record.remove(path);
			changes
				.get_mut(backend)
				.push(Change::Removed(path.to_path_buf()));
		}
		record.save(state)?;
	}
	Ok(changes)
}

/// One backend's share of an apply: its files put in place, then picked up.
struct Step<'a> {
	backend: Backend,
	hardware: &'a Hardware,
	/// What was in place before this apply.
	record: &'a Record,
	written: &'a [&'a File],
	removed: &'a [&'a Path],
}

impl Step<'_> {
	fn run(&self, system: &mut dyn System) -> Result<(), Error> {
		match self.backend {
			Backend::Regdom => {
				self.put()?;
				if let Some(file) = self.written.first() {
					let domain = regulatory_domain(&file.contents).ok_or_else(|| {
						self.failed(anyhow::anyhow!(
							"{:?} names no regulatory domain",
							file.path
						))
					})?;
					system
						.set_regulatory_domain(domain)
						.map_err(|e| self.failed(e))?;
				}
			}
			Backend::Hostapd => self.hostapd(system)?,
			Backend::Iwd => {
				self.put()?;
				// iwd watches its state directory and rereads a known network as it lands or goes,
				// so only its main configuration wants a restart.
				let config = &self.hardware.paths.iwd_config;
				if self.touches(config) {
					system.restart_iwd().map_err(|e| self.failed(e))?;
				}
			}
			Backend::Networkd => {
				self.put()?;
				system.reload_networkd().map_err(|e| self.failed(e))?;
			}
			Backend::Resolved => {
				self.put()?;
				system.reload_resolved().map_err(|e| self.failed(e))?;
			}
		}
		Ok(())
	}

	/// hostapd's configuration only ever changes as one file, so it is written, restarted on, or
	/// taken away with the interface it ran on.
	fn hostapd(&self, system: &mut dyn System) -> Result<(), Error> {
		let interface = self.virtual_interface();
		if !self.removed.is_empty() {
			system.hostapd(Hostapd::Stop).map_err(|e| self.failed(e))?;
			if let Some((_, interface)) = interface {
				system
					.delete_access_point(interface)
					.map_err(|e| self.failed(e))?;
			}
			return self.put();
		}

		self.put()?;
		let conf = &self.hardware.paths.hostapd;
		if self.record.contains(conf) {
			system
				.hostapd(Hostapd::Restart)
				.map_err(|e| self.failed(e))?;
		} else {
			if let Some((radio, interface)) = interface {
				system
					.create_access_point(radio, interface)
					.map_err(|e| self.failed(e))?;
			}
			system.hostapd(Hostapd::Start).map_err(|e| self.failed(e))?;
		}
		Ok(())
	}

	/// The radio and the access point interface bliti creates on it, where the hotspot runs on an
	/// interface of its own beside the station.
	fn virtual_interface(&self) -> Option<(&str, &str)> {
		let radio = self.hardware.station.as_deref()?;
		let interface = self.hardware.access_point.as_deref()?;
		(radio != interface).then_some((radio, interface))
	}

	fn touches(&self, path: &Path) -> bool {
		self.written.iter().any(|file| file.path == path) || self.removed.contains(&path)
	}

	/// Write and remove this backend's files.
	fn put(&self) -> Result<(), Error> {
		for file in self.written {
			files::write(file).map_err(|source| self.file(&file.path, source))?;
		}
		for path in self.removed {
			files::remove(path).map_err(|source| self.file(path, source))?;
		}
		Ok(())
	}

	fn file(&self, path: &Path, source: io::Error) -> Error {
		Error::File {
			backend: self.backend,
			path: path.to_path_buf(),
			source,
		}
	}

	fn failed(&self, source: anyhow::Error) -> Error {
		Error::System {
			backend: self.backend,
			source,
		}
	}
}

/// The domain a rendered modprobe file puts cfg80211 under.
fn regulatory_domain(modprobe: &str) -> Option<&str> {
	modprobe
		.lines()
		.find_map(|line| line.strip_prefix("options cfg80211 ieee80211_regdom="))
		.map(str::trim)
		.filter(|domain| !domain.is_empty())
}
