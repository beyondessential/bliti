//! The system on the board's disk, as the steps that install into it and encrypt it reach it
//! (PDSK, PENC).

use std::{
	fs::{self, File},
	io::{self, Write as _},
	path::Path,
	sync::Arc,
};

use crate::provision::{
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

/// Put `content` at `path`, so that `path` holds the old content or the new at every moment,
/// including through a power cut.
pub fn replace(path: &Path, content: &[u8]) -> io::Result<()> {
	let dir = path.parent().unwrap_or(Path::new("."));
	let mut file = tempfile::Builder::new()
		.prefix(".bliti-")
		.tempfile_in(dir)?;
	file.write_all(content)?;
	file.as_file().sync_all()?;
	file.persist(path).map_err(|err| err.error)?;
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
	fn replace_makes_the_file_where_there_is_none() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("file");
		replace(&path, b"new").unwrap();
		assert_eq!(read(&path).unwrap().as_deref(), Some(&b"new"[..]));
		assert_eq!(read(&dir.path().join("none")).unwrap(), None);
	}
}
