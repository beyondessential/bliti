//! The step `software`: install the Iti's software, as the medium carries it, into the system on the
//! disk, with the packages its network configuration needs, and enable its service (PDSK).
//!
//! Interim: replaced by installing from the apt repository (G5).

use std::{
	fmt, fs, io,
	os::unix::fs::PermissionsExt as _,
	path::{Path, PathBuf},
	sync::Arc,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use walkdir::WalkDir;

use self::network::{DEBS, Installer, Native, Status, carried, set_units, unset};
use super::{IMAGES, blocking};
use crate::provision::{
	context::Context,
	luks::Luks,
	step::{Done, Step},
	storage::{Storage, single_disk},
	system::{link, open_root, read, replace_with_mode},
};

mod network;

/// Where, under the images, the medium carries the software, at the paths it installs to.
const TREE: &str = "system";

/// The service, under the root filesystem.
const SERVICE: &str = "etc/systemd/system/bliti.service";

/// The link, under the root filesystem, that enables the service.
const ENABLED: &str = "etc/systemd/system/multi-user.target.wants/bliti.service";

/// A file of the software: where it goes under the root filesystem, what it holds, and its permission
/// bits.
struct File {
	path: PathBuf,
	content: Vec<u8>,
	mode: u32,
}

/// The step.
pub struct Software {
	storage: Arc<dyn Storage>,
	luks: Arc<dyn Luks>,
	installer: Arc<dyn Installer>,
	images: PathBuf,
}

impl Software {
	/// A step installing into the system on the disk `storage` finds, with that system's own package
	/// and service managers.
	pub fn new(storage: Arc<dyn Storage>, luks: Arc<dyn Luks>) -> Self {
		Self::with(storage, luks, Arc::new(Native), IMAGES)
	}

	fn with(
		storage: Arc<dyn Storage>,
		luks: Arc<dyn Luks>,
		installer: Arc<dyn Installer>,
		images: impl AsRef<Path>,
	) -> Self {
		Self {
			storage,
			luks,
			installer,
			images: images.as_ref().to_owned(),
		}
	}

	/// The software the medium carries.
	async fn software(&self) -> Result<Arc<[File]>, String> {
		let tree = self.images.join(TREE);
		blocking(move || software(&tree)).await
	}

	/// What the disk lacks of the software, in a phrase for each thing, or why that cannot be told.
	async fn lacks(&self, files: Arc<[File]>) -> Result<Vec<String>, String> {
		let (storage, luks, installer) = (
			self.storage.clone(),
			self.luks.clone(),
			self.installer.clone(),
		);
		blocking(move || {
			let disk = single_disk(&*storage)?;
			let root = open_root(&*storage, &luks, &disk, false)?;
			Ok(lacks(root.path(), &files, &*installer))
		})
		.await
	}
}

#[async_trait]
impl Step for Software {
	fn id(&self) -> &str {
		"software"
	}

	fn title(&self) -> &str {
		"Install the Iti's software"
	}

	fn undone_by(&self) -> &[&str] {
		&["disk"]
	}

	// spec: PDSK#done-state-of-the-step-software
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		let files = match self.software().await {
			Ok(files) => files,
			Err(why) => return unknown(why),
		};
		match self.lacks(files).await {
			Ok(lacks) if lacks.is_empty() => Done {
				state: DoneState::Done,
				why: Some("the Iti's software is installed".to_owned()),
			},
			Ok(lacks) => Done {
				state: DoneState::NotDone,
				why: Some(lacks.join("; ")),
			},
			Err(why) => unknown(why),
		}
	}

	// spec: PDSK#installing-the-itis-software
	async fn run(&self, _context: &mut Context) -> Result<(), String> {
		let files = self.software().await?;
		let debs = self.images.join(DEBS);
		let (storage, luks, installer) = (
			self.storage.clone(),
			self.luks.clone(),
			self.installer.clone(),
		);
		blocking(move || {
			let disk = single_disk(&*storage)?;
			let mounted = open_root(&*storage, &luks, &disk, true)?;
			let root = mounted.path();
			if !root.join("etc").is_dir() {
				return Err("the disk holds no root filesystem to install into".to_owned());
			}
			for file in files.iter() {
				if lack(root, file).is_none() {
					continue;
				}
				let place = root.join(&file.path);
				if let Some(dir) = place.parent() {
					fs::create_dir_all(dir)
						.map_err(|err| format!("cannot make {}: {err}", shown(dir, root)))?;
				}
				replace_with_mode(&place, &file.content, file.mode)
					.map_err(|err| format!("cannot write {}: {err}", shown(&place, root)))?;
			}
			if unenabled(root).is_some() {
				let place = root.join(ENABLED);
				if let Some(dir) = place.parent() {
					fs::create_dir_all(dir)
						.map_err(|err| format!("cannot make {}: {err}", shown(dir, root)))?;
				}
				link(&place, &Path::new("/").join(SERVICE))
					.map_err(|err| format!("cannot enable the Iti's service: {err}"))?;
			}
			install_packages(root, &debs, &*installer)?;
			set_units(root, &*installer)?;
			let lacks = lacks(root, &files, &*installer);
			if lacks.is_empty() {
				Ok(())
			} else {
				Err(format!(
					"the Iti's software did not read back: {}",
					lacks.join("; ")
				))
			}
		})
		.await
	}
}

/// The files of the tree at `tree`, each at its path under the tree.
fn software(tree: &Path) -> Result<Arc<[File]>, String> {
	if !tree.is_dir() {
		return Err("there is no software to install".to_owned());
	}
	let cannot = |err: &dyn fmt::Display| format!("cannot read the software to install: {err}");
	let mut files = Vec::new();
	for entry in WalkDir::new(tree).sort_by_file_name() {
		let entry = entry.map_err(|err| cannot(&err))?;
		let kind = entry.file_type();
		if kind.is_dir() {
			continue;
		}
		let path = entry
			.path()
			.strip_prefix(tree)
			.expect("the walk stays under the tree")
			.to_owned();
		if !kind.is_file() {
			return Err(format!(
				"the software to install holds {}, which is not a file",
				Path::new("/").join(path).display()
			));
		}
		let mode = entry
			.metadata()
			.map_err(|err| cannot(&err))?
			.permissions()
			.mode() & 0o7777;
		let content = fs::read(entry.path()).map_err(|err| cannot(&err))?;
		files.push(File {
			path,
			content,
			mode,
		});
	}
	if files.is_empty() {
		return Err("there is no software to install".to_owned());
	}
	Ok(files.into())
}

/// Where the system at `root` lacks any of the packages the network configuration needs, finish what
/// its package manager left unfinished and install those the medium carries in `debs` that the
/// system does not wholly hold.
// spec: PDSK#installing-the-itis-software
fn install_packages(root: &Path, debs: &Path, installer: &dyn Installer) -> Result<(), String> {
	let status = Status::read(root).ok();
	if status
		.as_ref()
		.is_some_and(|status| status.lacks().is_empty())
	{
		return Ok(());
	}
	let carried = carried(debs)?;
	let names: Vec<String> = carried
		.into_iter()
		.filter(|(package, _)| {
			!status
				.as_ref()
				.is_some_and(|status| status.installed(package))
		})
		.map(|(_, file)| file)
		.collect();
	installer.install(root, debs, &names)
}

/// A phrase for each file the root filesystem at `root` does not hold as `files` has it, for the
/// service where it is not enabled, for each package the network configuration needs that is not
/// installed, and for each unit not in its state.
fn lacks(root: &Path, files: &[File], installer: &dyn Installer) -> Vec<String> {
	let packages = match Status::read(root) {
		Ok(status) => status.lacks(),
		Err(why) => vec![why],
	};
	files
		.iter()
		.filter_map(|file| lack(root, file))
		.chain(unenabled(root))
		.chain(packages)
		.chain(unset(root, installer))
		.collect()
}

/// What the root filesystem at `root` lacks of `file`, if anything.
fn lack(root: &Path, file: &File) -> Option<String> {
	let place = root.join(&file.path);
	let shown = shown(&place, root);
	let meta = match fs::symlink_metadata(&place) {
		Ok(meta) => meta,
		Err(err) if err.kind() == io::ErrorKind::NotFound => {
			return Some(format!("{shown} is missing"));
		}
		Err(err) => return Some(format!("cannot read {shown}: {err}")),
	};
	if !meta.is_file() {
		return Some(format!("{shown} is not a file"));
	}
	match read(&place) {
		Ok(Some(held)) if held != file.content => Some(format!("{shown} differs")),
		Ok(Some(_)) => {
			let mode = meta.permissions().mode() & 0o7777;
			(mode != file.mode).then(|| format!("{shown} has mode {mode:o}, not {:o}", file.mode))
		}
		Ok(None) => Some(format!("{shown} is missing")),
		Err(err) => Some(format!("cannot read {shown}: {err}")),
	}
}

/// What keeps the service from being enabled in the root filesystem at `root`, if anything.
fn unenabled(root: &Path) -> Option<String> {
	match fs::read_link(root.join(ENABLED)) {
		Ok(target) if target == Path::new("/").join(SERVICE) => None,
		Ok(_) => Some("the link enabling the Iti's service points elsewhere".to_owned()),
		Err(err)
			if matches!(
				err.kind(),
				io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
			) =>
		{
			Some("the Iti's service is not enabled".to_owned())
		}
		Err(err) => Some(format!(
			"cannot read whether the Iti's service is enabled: {err}"
		)),
	}
}

/// `place`, under the root filesystem at `root`, as the system on the disk names it.
fn shown(place: &Path, root: &Path) -> String {
	Path::new("/")
		.join(place.strip_prefix(root).unwrap_or(place))
		.display()
		.to_string()
}

#[cfg(test)]
mod tests;
