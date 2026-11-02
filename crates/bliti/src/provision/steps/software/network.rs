//! What the Iti's network configuration needs of the system on the disk: the packages the stack
//! backend drives, installed from those the medium carries, and the states of their services (PDSK).

use std::{
	collections::HashMap,
	fs, io,
	path::{Path, PathBuf},
	process::Command,
};

use crate::provision::{
	exec::{failure, finish, run},
	system::Mounts,
};

/// The packages the network configuration needs.
pub const PACKAGES: [&str; 2] = ["iwd", "hostapd"];

/// Where, under the images, the medium carries the packages, each in a file named as the archive
/// names it, `<package>_<version>_<architecture>.deb`.
pub const DEBS: &str = "debs";

/// The package database, under the root filesystem.
const STATUS: &str = "var/lib/dpkg/status";

/// Where the package manager journals what it has not yet folded into its database, under the root
/// filesystem.
const JOURNAL: &str = "var/lib/dpkg/updates";

/// The status of a package that is wholly installed.
const INSTALLED: &str = "install ok installed";

/// What a unit's state is to be in the system on the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
	/// Installed, and started only when something asks for it.
	Disabled,
	/// Not started at boot, or not installed at all.
	Off,
	/// Never started.
	Masked,
}

impl Want {
	fn word(self) -> &'static str {
		match self {
			Self::Disabled | Self::Off => "disabled",
			Self::Masked => "masked",
		}
	}

	fn holds(self, state: &str) -> bool {
		match self {
			Self::Disabled => state == "disabled",
			Self::Off => matches!(state, "disabled" | "masked" | NOT_FOUND),
			Self::Masked => state == "masked",
		}
	}
}

/// The state `systemctl is-enabled` gives a unit that is not installed.
pub const NOT_FOUND: &str = "not-found";

/// The units, and what each is to be: bliti starts iwd itself and drives hostapd through its own
/// unit, and wpa_supplicant would contend with iwd for the radio.
pub const UNITS: [(&str, Want); 3] = [
	("iwd.service", Want::Disabled),
	("wpa_supplicant.service", Want::Off),
	("hostapd.service", Want::Masked),
];

/// What changes a unit's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
	Disable,
	Mask,
	Unmask,
}

impl Action {
	fn verb(self) -> &'static str {
		match self {
			Self::Disable => "disable",
			Self::Mask => "mask",
			Self::Unmask => "unmask",
		}
	}
}

/// The system on the disk's own package and service managers, run on its root filesystem.
pub trait Installer: Send + Sync {
	/// Finish whatever the package manager left unfinished in the system at `root`, then install the
	/// packages in the files `names` of the directory `debs`.
	fn install(&self, root: &Path, debs: &Path, names: &[String]) -> Result<(), String>;

	/// The state of `unit` in the system at `root`, as `systemctl is-enabled` names it.
	fn state(&self, root: &Path, unit: &str) -> Result<String, String>;

	/// Change the state of `unit` in the system at `root`.
	fn set(&self, root: &Path, unit: &str, action: Action) -> Result<(), String>;
}

/// The package database of the system at a root filesystem.
pub struct Status {
	/// Each package's status, an installed one where several architectures of it are recorded.
	held: HashMap<String, String>,
	/// Whether the package manager stopped with changes journalled and not yet in the database.
	interrupted: bool,
}

impl Status {
	/// The package database of the system at `root`.
	pub fn read(root: &Path) -> Result<Self, String> {
		let text = fs::read_to_string(root.join(STATUS))
			.map_err(|err| format!("cannot read the system's package database: {err}"))?;
		let mut held = HashMap::<String, String>::new();
		for stanza in text.split("\n\n") {
			let field = |name: &str| {
				stanza
					.lines()
					.find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
					.map(str::trim)
			};
			let (Some(package), Some(status)) = (field("Package"), field("Status")) else {
				continue;
			};
			if held.get(package).is_none_or(|had| had != INSTALLED) {
				held.insert(package.to_owned(), status.to_owned());
			}
		}
		let interrupted = match fs::read_dir(root.join(JOURNAL)) {
			Ok(entries) => entries.flatten().any(|entry| {
				let name = entry.file_name();
				let name = name.to_string_lossy();
				!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_digit())
			}),
			Err(err) if err.kind() == io::ErrorKind::NotFound => false,
			Err(err) => {
				return Err(format!(
					"cannot read the system's package manager journal: {err}"
				));
			}
		};
		Ok(Self { held, interrupted })
	}

	/// Whether `package` is wholly installed.
	pub fn installed(&self, package: &str) -> bool {
		self.held.get(package).is_some_and(|status| status == INSTALLED)
	}

	/// A phrase for each of the packages the network configuration needs that is not wholly
	/// installed, and for an interrupted package manager.
	pub fn lacks(&self) -> Vec<String> {
		let interrupted = self
			.interrupted
			.then(|| "the system's package manager was interrupted".to_owned());
		interrupted
			.into_iter()
			.chain(PACKAGES.iter().filter_map(|package| {
				match self.held.get(*package) {
					None => Some(format!("{package} is not installed")),
					Some(status) if status == INSTALLED => None,
					Some(status) => Some(format!("{package} is not wholly installed ({status})")),
				}
			}))
			.collect()
	}
}

/// The packages the medium carries in the directory `debs`: each one's name and its file's.
pub fn carried(debs: &Path) -> Result<Vec<(String, String)>, String> {
	let entries = match fs::read_dir(debs) {
		Ok(entries) => entries,
		Err(err) if err.kind() == io::ErrorKind::NotFound => {
			return Err("the medium carries no packages to install".to_owned());
		}
		Err(err) => return Err(format!("cannot read the packages to install: {err}")),
	};
	let mut carried = Vec::new();
	for entry in entries {
		let entry = entry.map_err(|err| format!("cannot read the packages to install: {err}"))?;
		let file = entry.file_name().to_string_lossy().into_owned();
		let Some(package) = file
			.strip_suffix(".deb")
			.and_then(|stem| stem.split_once('_'))
			.map(|(package, _)| package.to_owned())
		else {
			continue;
		};
		carried.push((package, file));
	}
	carried.sort();
	Ok(carried)
}

/// A phrase for each unit not in the state it is to be in, in the system at `root`.
pub fn unset(root: &Path, installer: &dyn Installer) -> Vec<String> {
	UNITS
		.iter()
		.filter_map(|(unit, want)| match installer.state(root, unit) {
			Ok(state) if want.holds(&state) => None,
			Ok(state) if state == NOT_FOUND => Some(format!("{unit} is not installed")),
			Ok(state) => Some(format!("{unit} is {state}, not {}", want.word())),
			Err(err) => Some(format!("cannot read whether {unit} is enabled: {err}")),
		})
		.collect()
}

/// Put each unit in the state it is to be in, in the system at `root`, changing none already there.
pub fn set_units(root: &Path, installer: &dyn Installer) -> Result<(), String> {
	for (unit, want) in UNITS {
		let state = installer.state(root, unit)?;
		if want.holds(&state) {
			continue;
		}
		let actions: &[Action] = match want {
			Want::Disabled | Want::Off if state == "masked" => &[Action::Unmask, Action::Disable],
			Want::Disabled | Want::Off => &[Action::Disable],
			Want::Masked => &[Action::Mask],
		};
		for action in actions {
			installer.set(root, unit, *action)?;
		}
	}
	Ok(())
}

/// Where, inside the system on the disk, the packages to install are put while it installs them.
const INSIDE: &str = "run/bliti-debs";

/// The system's own dpkg, run inside it with `chroot`, and systemctl, run on its root filesystem.
pub struct Native;

impl Installer for Native {
	fn install(&self, root: &Path, debs: &Path, names: &[String]) -> Result<(), String> {
		let mut mounts = Mounts::chroot(root)?;
		// An empty /run tells maintainer scripts the system is not running, so they start nothing.
		mounts.tmpfs(&root.join("run"))?;
		let inside = root.join(INSIDE);
		fs::create_dir_all(&inside)
			.map_err(|err| format!("cannot make /{INSIDE} in the system: {err}"))?;
		mounts.bind(debs, &inside)?;
		let dpkg = || {
			let mut command = Command::new("chroot");
			command
				.arg(root)
				.args(["dpkg", "--force-confdef", "--force-confold"])
				.env("DEBIAN_FRONTEND", "noninteractive");
			command
		};
		run(dpkg().args(["--configure", "-a"]))?;
		if !names.is_empty() {
			run(dpkg()
				.arg("--install")
				.args(names.iter().map(|name| Path::new("/").join(INSIDE).join(name))))?;
		}
		Ok(())
	}

	fn state(&self, root: &Path, unit: &str) -> Result<String, String> {
		let mut command = systemctl(root);
		command.args(["is-enabled", unit]);
		let output = finish(&mut command)?;
		let stdout = String::from_utf8_lossy(&output.stdout);
		if let Some(state) = stdout.lines().map(str::trim).find(|line| !line.is_empty()) {
			return Ok(state.to_owned());
		}
		if String::from_utf8_lossy(&output.stderr).contains("No such file or directory") {
			return Ok(NOT_FOUND.to_owned());
		}
		Err(failure(&command, &output))
	}

	fn set(&self, root: &Path, unit: &str, action: Action) -> Result<(), String> {
		run(systemctl(root).args([action.verb(), unit])).map(drop)
	}
}

fn systemctl(root: &Path) -> Command {
	let mut command = Command::new("systemctl");
	command.arg(PathBuf::from("--root=").join(root).as_os_str());
	command
}
