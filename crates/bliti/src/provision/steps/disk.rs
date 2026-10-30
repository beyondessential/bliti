//! The step `disk`: write the system on the provisioning medium to the board's NVMe disk, and read
//! it back (PDSK).

use std::{
	fmt, fs,
	fs::{File, OpenOptions},
	io::{self, Read, Write as _},
	path::{Path, PathBuf},
	sync::Arc,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::DoneState;
use sha2::{Digest as _, Sha256};
use tokio::sync::mpsc;

use super::{IMAGES, blocking};
use crate::provision::{
	context::Context,
	manifest::Manifest,
	step::{Done, Step},
	storage::{FIRMWARE, Storage, partition, single_disk},
	system::{read, replace},
};

/// The file at the root of the disk's firmware partition that marks a system the step wrote and
/// verified (PDSK, "What the disk holds").
pub const MARKER: &str = "iti-image.txt";

/// The labels the `pi` image gives its partitions.
const LABELS: [&str; 3] = ["firmware", "xboot", "root"];

/// How much is written or read at a time.
const CHUNK: usize = 4 << 20;

/// What the disk holds (PDSK, "What the disk holds").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
	/// The medium's system, of this version.
	Medium(String),
	/// An earlier system, of this version where its marker names one.
	Earlier(Option<String>),
	/// Nothing.
	Blank,
	/// A system other than the Iti's.
	Other,
}

impl fmt::Display for Held {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Medium(version) => write!(f, "the medium's system, {version}"),
			Self::Earlier(Some(version)) => write!(f, "an earlier system, {version}"),
			Self::Earlier(None) => write!(f, "an earlier system, or one written in part"),
			Self::Blank => write!(f, "nothing, and is blank"),
			Self::Other => write!(f, "a system other than the Iti's"),
		}
	}
}

/// The step.
pub struct Disk {
	storage: Arc<dyn Storage>,
	images: PathBuf,
}

impl Disk {
	/// A step writing to the disk `storage` finds.
	pub fn new(storage: Arc<dyn Storage>) -> Self {
		Self::with_images(storage, IMAGES)
	}

	fn with_images(storage: Arc<dyn Storage>, images: impl AsRef<Path>) -> Self {
		Self {
			storage,
			images: images.as_ref().to_owned(),
		}
	}

	async fn manifest(&self) -> Result<Manifest, String> {
		let path = self.images.join("manifest");
		blocking(move || Manifest::read(&path)).await
	}

	/// What the disk holds, and whether it carries the marker.
	async fn held(&self, manifest: &Manifest) -> Result<(Held, bool), String> {
		let (storage, manifest) = (self.storage.clone(), manifest.clone());
		blocking(move || inspect(&*storage, &manifest)).await
	}
}

#[async_trait]
impl Step for Disk {
	fn id(&self) -> &str {
		"disk"
	}

	fn title(&self) -> &str {
		"Write the system to the disk"
	}

	// spec: PDSK#done-state-of-the-step-disk
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		let manifest = match self.manifest().await {
			Ok(manifest) => manifest,
			Err(why) => return unknown(why),
		};
		match self.held(&manifest).await {
			Ok((held @ Held::Medium(_), _)) => Done {
				state: DoneState::Done,
				why: Some(format!("holds {held}")),
			},
			Ok((held, _)) => Done {
				state: DoneState::NotDone,
				why: Some(format!("holds {held}")),
			},
			Err(why) => unknown(format!("cannot read the disk: {why}")),
		}
	}

	// spec: PDSK#writing-the-system
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let manifest = self.manifest().await?;
		let (storage, size) = (self.storage.clone(), manifest.size);
		let disk = blocking(move || {
			let disk = single_disk(&*storage)?;
			if storage.size(&disk)? < size {
				return Err("the NVMe drive is smaller than the system to write".to_owned());
			}
			Ok(disk)
		})
		.await?;

		let (held, marked) = self
			.held(&manifest)
			.await
			.map_err(|err| format!("cannot read the disk: {err}"))?;
		if held == Held::Other {
			let text = format!(
				"The disk holds {held}. Writing the Iti's system erases it. Write it anyway?"
			);
			if !context.confirm("overwrite", &text).await {
				return Err("the disk was left as it was".to_owned());
			}
		}
		if marked {
			let (storage, disk) = (self.storage.clone(), disk.clone());
			blocking(move || remove_marker(&*storage, &disk))
				.await
				.map_err(|err| format!("cannot remove the marker from the disk: {err}"))?;
		}

		let (progress, mut reports) = mpsc::unbounded_channel();
		let work = {
			let (storage, manifest, disk) = (self.storage.clone(), manifest.clone(), disk.clone());
			blocking(move || {
				write_and_verify(&*storage, &manifest, &disk, &|fraction| {
					let _ = progress.send(fraction);
				})
			})
		};
		tokio::pin!(work);
		let written = loop {
			tokio::select! {
				written = &mut work => break written,
				Some(fraction) = reports.recv() => context.progress(fraction),
			}
		};
		while let Ok(fraction) = reports.try_recv() {
			context.progress(fraction);
		}
		written?;

		let (storage, manifest) = (self.storage.clone(), manifest);
		blocking(move || write_marker(&*storage, &manifest, &disk))
			.await
			.map_err(|err| format!("cannot mark the disk as written: {err}"))
	}
}

/// What the disk holds, and whether it carries the marker.
fn inspect(storage: &dyn Storage, manifest: &Manifest) -> Result<(Held, bool), String> {
	let disk = single_disk(storage)?;
	if let Some((version, sha256)) = read_marker(storage, &disk) {
		let held = if sha256 == manifest.sha256 {
			Held::Medium(version)
		} else {
			Held::Earlier(Some(version))
		};
		return Ok((held, true));
	}
	if storage.blank(&disk)? {
		return Ok((Held::Blank, false));
	}
	let labels = storage.labels(&disk)?;
	if LABELS
		.iter()
		.all(|label| labels.iter().any(|held| held == label))
	{
		Ok((Held::Earlier(None), false))
	} else {
		Ok((Held::Other, false))
	}
}

/// The version and digest the disk's marker names, where it carries one.
fn read_marker(storage: &dyn Storage, disk: &Path) -> Option<(String, String)> {
	let mounted = storage
		.mount(&partition(disk, FIRMWARE), "vfat", "ro")
		.ok()?;
	let text = read(&mounted.path().join(MARKER)).ok()??;
	let text = String::from_utf8(text).ok()?;
	let get = |name: &str| {
		text.lines()
			.find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
			.map(|value| value.trim().to_owned())
	};
	Some((
		get("PI_IMAGE_VERSION")?,
		get("PI_IMAGE_SHA256")?.to_ascii_lowercase(),
	))
}

fn remove_marker(storage: &dyn Storage, disk: &Path) -> Result<(), String> {
	let mounted = storage.mount(&partition(disk, FIRMWARE), "vfat", "rw")?;
	match fs::remove_file(mounted.path().join(MARKER)) {
		Ok(()) => Ok(()),
		Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
		Err(err) => Err(err.to_string()),
	}
}

fn write_marker(storage: &dyn Storage, manifest: &Manifest, disk: &Path) -> Result<(), String> {
	storage.rescan(disk)?;
	let mounted = storage.mount(&partition(disk, FIRMWARE), "vfat", "rw")?;
	let text = format!(
		"PI_IMAGE_VERSION={}\nPI_IMAGE_SHA256={}\n",
		manifest.version, manifest.sha256
	);
	replace(&mounted.path().join(MARKER), text.as_bytes()).map_err(|err| err.to_string())
}

/// Write the payload to the disk, flush it, and read the disk back to check it against the manifest.
/// `progress` is told how far through both, from 0 to 1.
fn write_and_verify(
	storage: &dyn Storage,
	manifest: &Manifest,
	disk: &Path,
	progress: &dyn Fn(f64),
) -> Result<(), String> {
	let total = manifest.size;
	let fraction = |done: u64, base: f64| base + done as f64 / (2 * total.max(1)) as f64;

	let mut payload = storage.payload(manifest)?;
	let mut out = OpenOptions::new()
		.write(true)
		.open(disk)
		.map_err(|err| format!("cannot write to {}: {err}", disk.display()))?;
	let mut buf = vec![0u8; CHUNK];
	let mut written = 0u64;
	while written < total {
		let want = CHUNK.min((total - written) as usize);
		let got = payload
			.read(&mut buf[..want])
			.map_err(|err| format!("cannot read the system on the provisioning medium: {err}"))?;
		if got == 0 {
			return Err(
				"the system on the provisioning medium is shorter than its manifest says"
					.to_owned(),
			);
		}
		out.write_all(&buf[..got])
			.map_err(|err| format!("cannot write to the disk: {err}"))?;
		written += got as u64;
		progress(fraction(written, 0.0));
	}
	out.sync_all()
		.map_err(|err| format!("cannot flush the disk: {err}"))?;
	drop(payload);

	// A fresh open, with what the kernel holds of the disk dropped, so the bytes come from the disk.
	let mut back =
		File::open(disk).map_err(|err| format!("cannot read {}: {err}", disk.display()))?;
	drop_cache(&back);
	let mut hash = Sha256::new();
	let mut read = 0u64;
	while read < total {
		let want = CHUNK.min((total - read) as usize);
		let got = back
			.read(&mut buf[..want])
			.map_err(|err| format!("cannot read the disk back: {err}"))?;
		if got == 0 {
			return Err("the disk is shorter than what was written".to_owned());
		}
		hash.update(&buf[..got]);
		read += got as u64;
		progress(fraction(read, 0.5));
	}
	if hex::encode(hash.finalize()) == manifest.sha256 {
		Ok(())
	} else {
		Err(
			"what was read back from the disk differs from the system on the provisioning medium"
				.to_owned(),
		)
	}
}

/// Drop what the kernel holds of `file`, so that reading it goes to the device.
fn drop_cache(file: &File) {
	if let Err(err) = rustix::fs::fadvise(file, 0, None, rustix::fs::Advice::DontNeed) {
		tracing::warn!(%err, "cannot drop the cache of the disk before reading it back");
	}
}

#[cfg(test)]
mod tests;
