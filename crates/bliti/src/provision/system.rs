//! The system on the board's disk, as the steps that install into it and encrypt it reach it
//! (PDSK, PENC).

use std::{
	fs::{self, File, Permissions},
	io::{self, Write as _},
	os::unix::fs::{PermissionsExt as _, symlink},
	path::{Path, PathBuf},
	process::Command,
	sync::Arc,
};

use crate::provision::{
	exec::run,
	luks::{Key, Luks},
	storage::{Mounted, ROOT, Storage, partition},
};

pub use self::initramfs::{Dracut, Initramfs, check, rewire};

mod initramfs;

/// The name the disk's root volume is opened as.
const MAPPING: &str = "bliti-root";

/// The root filesystem of the system on the disk, open and mounted for as long as this is held.
pub struct Root {
	mount: Option<Mounted>,
	luks: Arc<dyn Luks>,
}

impl Root {
	/// Where the root filesystem is.
	pub fn path(&self) -> &Path {
		self.mount
			.as_ref()
			.expect("the mount is held until the root is dropped")
			.path()
	}
}

impl Drop for Root {
	fn drop(&mut self) {
		drop(self.mount.take());
		self.luks.close(MAPPING);
	}
}

/// Open the root volume of the system on `disk` with the empty passphrase or, where that does not
/// unlock it, the board's TPM, and mount the root filesystem inside it (PDSK, "Opening the root
/// volume").
pub fn open_root(
	storage: &dyn Storage,
	luks: &Arc<dyn Luks>,
	disk: &Path,
	writable: bool,
) -> Result<Root, String> {
	let part = partition(disk, ROOT);
	let key = if luks.unlocks(&part, &Key::Empty, None)? {
		Key::Empty
	} else if luks.unlocks(&part, &Key::Tpm, None).unwrap_or(false) {
		Key::Tpm
	} else {
		return Err("the root volume could not be opened".to_owned());
	};
	let mapper = luks.open(&part, MAPPING, &key)?;
	let mode = if writable { "rw" } else { "ro" };
	match storage.mount(&mapper, "btrfs", &format!("subvol=@,{mode}")) {
		Ok(mount) => Ok(Root {
			mount: Some(mount),
			luks: luks.clone(),
		}),
		Err(err) => {
			luks.close(MAPPING);
			Err(format!("the root filesystem could not be mounted: {err}"))
		}
	}
}

/// Mounts made under a system's root filesystem, unmounted last first when dropped.
pub struct Mounts(Vec<PathBuf>);

impl Mounts {
	/// Bind this system's `/dev`, `/proc` and `/sys` into the system at `root`, as what runs inside it
	/// by `chroot` needs them.
	pub fn chroot(root: &Path) -> Result<Self, String> {
		let mut mounts = Self(Vec::new());
		for dir in ["dev", "proc", "sys"] {
			mounts.bind(&Path::new("/").join(dir), &root.join(dir))?;
		}
		Ok(mounts)
	}

	/// Bind `source` at `target`.
	pub fn bind(&mut self, source: &Path, target: &Path) -> Result<(), String> {
		self.add(
			Command::new("mount")
				.args(["--bind", "--make-private"])
				.arg(source),
			target,
		)
	}

	/// Mount an empty filesystem in memory at `target`.
	pub fn tmpfs(&mut self, target: &Path) -> Result<(), String> {
		self.add(
			Command::new("mount").args(["--make-private", "-t", "tmpfs", "tmpfs"]),
			target,
		)
	}

	fn add(&mut self, command: &mut Command, target: &Path) -> Result<(), String> {
		run(command.arg(target))?;
		self.0.push(target.to_owned());
		Ok(())
	}
}

impl Drop for Mounts {
	fn drop(&mut self) {
		for target in self.0.iter().rev() {
			if let Err(err) = run(Command::new("umount").arg("--lazy").arg(target)) {
				tracing::warn!(%err, path = %target.display(), "cannot unmount");
			}
		}
	}
}

/// Put `content` at `path`, so that `path` holds the old content or the new at every moment,
/// including through a power cut.
pub fn replace(path: &Path, content: &[u8]) -> io::Result<()> {
	put(path, content, None)
}

/// As [`replace`], with the file's permission bits `mode` from the moment it has the name.
pub fn replace_with_mode(path: &Path, content: &[u8], mode: u32) -> io::Result<()> {
	put(path, content, Some(mode))
}

fn put(path: &Path, content: &[u8], mode: Option<u32>) -> io::Result<()> {
	let dir = path.parent().unwrap_or(Path::new("."));
	let mut file = tempfile::Builder::new()
		.prefix(".bliti-")
		.tempfile_in(dir)?;
	file.write_all(content)?;
	if let Some(mode) = mode {
		file.as_file()
			.set_permissions(Permissions::from_mode(mode))?;
	}
	file.as_file().sync_all()?;
	file.persist(path).map_err(|err| err.error)?;
	File::open(dir)?.sync_all()
}

/// Put a symbolic link to `target` at `path`, in place of whatever is there. Where that is not a
/// directory, `path` holds it or the link at every moment.
pub fn link(path: &Path, target: &Path) -> io::Result<()> {
	let dir = path.parent().unwrap_or(Path::new("."));
	if fs::symlink_metadata(path).is_ok_and(|meta| meta.is_dir()) {
		fs::remove_dir_all(path)?;
	}
	tempfile::Builder::new()
		.prefix(".bliti-")
		.make_in(dir, |temp| symlink(target, temp))?
		.persist(path)
		.map_err(|err| err.error)?;
	File::open(dir)?.sync_all()
}

/// The content of the file at `path`, nothing where there is no file.
pub fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
	match fs::read(path) {
		Ok(content) => Ok(Some(content)),
		Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
		Err(err) => Err(err),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn replace_leaves_only_the_new_content_and_no_litter() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("file");
		fs::write(&path, b"old").unwrap();
		replace(&path, b"new").unwrap();
		assert_eq!(fs::read(&path).unwrap(), b"new");
		assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
	}

	#[test]
	fn replace_with_mode_sets_the_mode_whatever_the_file_had() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("file");
		fs::write(&path, b"old").unwrap();
		fs::set_permissions(&path, Permissions::from_mode(0o600)).unwrap();
		replace_with_mode(&path, b"new", 0o755).unwrap();
		assert_eq!(fs::read(&path).unwrap(), b"new");
		assert_eq!(
			fs::metadata(&path).unwrap().permissions().mode() & 0o7777,
			0o755
		);
		assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
	}

	#[test]
	fn link_replaces_a_file_a_link_or_a_directory_and_leaves_no_litter() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("link");
		let target = Path::new("/etc/target");
		for before in ["file", "link", "directory"] {
			match before {
				"file" => fs::write(&path, b"old").unwrap(),
				"link" => symlink("/elsewhere", &path).unwrap(),
				_ => fs::create_dir_all(path.join("inside")).unwrap(),
			}
			link(&path, target).unwrap();
			assert_eq!(fs::read_link(&path).unwrap(), target, "over a {before}");
			assert_eq!(
				fs::read_dir(dir.path()).unwrap().count(),
				1,
				"over a {before}"
			);
			fs::remove_file(&path).unwrap();
		}
	}

	#[test]
	fn replace_makes_the_file_where_there_is_none() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("file");
		replace(&path, b"new").unwrap();
		assert_eq!(read(&path).unwrap().as_deref(), Some(&b"new"[..]));
		assert_eq!(read(&dir.path().join("none")).unwrap(), None);
	}
}
