//! Files put in place atomically, and the files bliti owns found on disk.

use std::{
	fs::{self, DirBuilder, OpenOptions, Permissions},
	io::{self, Write as _},
	os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _, PermissionsExt as _},
	path::{Path, PathBuf},
};

use super::super::render::{File, Paths};

/// The mode of a directory bliti creates to put a file in.
const DIRECTORY: u32 = 0o755;

/// Whether anything is at `path`.
pub(super) fn exists(path: &Path) -> bool {
	path.symlink_metadata().is_ok()
}

/// Every file bliti owns that is on disk under `paths`.
///
/// A directory that cannot be read holds nothing to delete: it is either absent, or the write that
/// follows fails on it and says so.
pub(super) fn owned(paths: &Paths) -> Vec<PathBuf> {
	let mut found: Vec<PathBuf> = [&paths.iwd_config, &paths.modprobe]
		.into_iter()
		.filter(|path| exists(path))
		.cloned()
		.collect();
	for dir in [&paths.networkd, &paths.iwd_state, &paths.hostapd] {
		let Ok(entries) = fs::read_dir(dir) else {
			continue;
		};
		found.extend(
			entries
				.filter_map(Result::ok)
				.map(|entry| entry.path())
				.filter(|path| paths.owns(path)),
		);
	}
	found
}

/// Put `file` in place, so that a reader sees either the old file or the whole new one.
///
/// The contents go to a temporary file in the same directory, which has its mode before any content
/// lands in it so a secret is never readable by anyone else, and is synced and renamed over the
/// file. The directory is synced after, so the rename survives a power cut.
pub(super) fn write(file: &File) -> io::Result<()> {
	write_atomically(&file.path, file.contents.as_bytes(), file.mode)
}

pub(super) fn write_atomically(path: &Path, contents: &[u8], mode: u32) -> io::Result<()> {
	let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
		return Err(io::Error::new(
			io::ErrorKind::InvalidInput,
			"not a path to a file",
		));
	};
	DirBuilder::new()
		.recursive(true)
		.mode(DIRECTORY)
		.create(dir)?;

	// A leading dot and a suffix no backend reads, so neither bliti nor iwd, which watches its
	// directory, takes the temporary file for a real one.
	let temporary = dir.join(format!(
		".{}.{:016x}.bliti-tmp",
		name.to_string_lossy(),
		rand::random::<u64>()
	));
	let written = (|| {
		let mut out = OpenOptions::new()
			.write(true)
			.create_new(true)
			.mode(mode)
			.open(&temporary)?;
		// The mode given at creation is narrowed by the umask; this sets it exactly, still before
		// any content lands.
		out.set_permissions(Permissions::from_mode(mode))?;
		out.write_all(contents)?;
		out.sync_all()?;
		fs::rename(&temporary, path)
	})();
	if written.is_err() {
		let _ = fs::remove_file(&temporary);
	}
	written?;
	sync_directory(dir)
}

/// Take away the file at `path`, where there is one.
pub(super) fn remove(path: &Path) -> io::Result<()> {
	match fs::remove_file(path) {
		Ok(()) => {}
		Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
		Err(e) => return Err(e),
	}
	match path.parent() {
		Some(dir) => sync_directory(dir),
		None => Ok(()),
	}
}

fn sync_directory(dir: &Path) -> io::Result<()> {
	fs::File::open(dir)?.sync_all()
}
