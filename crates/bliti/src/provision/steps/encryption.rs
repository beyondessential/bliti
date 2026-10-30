//! The step `encryption`: give the root volume of the system on the disk a key of its own, unlock it
//! from the board's TPM, and replace its empty passphrase with a recovery passphrase (PENC).

use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use async_trait::async_trait;
use bliti_core::provisioning::{Suffix, report::DoneState};

use self::records::Record;
use super::{
	IMAGES, blocking,
	hardware::place::{Air, place},
};
use crate::provision::{
	context::Context,
	luks::{Header, Key, Luks},
	manifest::Manifest,
	step::{Done, Step},
	storage::{FIRMWARE, ROOT, Storage, partition, single_disk},
	system::{Initramfs, check, open_root, rewire},
};

mod passphrase;
mod records;

/// What the step needs to know of the board it runs on, beyond its disk.
#[async_trait]
pub trait Identity: Send + Sync {
	/// Whether the TPM answers.
	fn tpm(&self) -> Result<(), String>;

	/// The suffix of the board's name, once it is known.
	async fn suffix(&self) -> Suffix;
}

/// What the step does to the disk, which the blocking work holds.
#[derive(Clone)]
struct Tools {
	storage: Arc<dyn Storage>,
	luks: Arc<dyn Luks>,
	initramfs: Arc<dyn Initramfs>,
	images: PathBuf,
}

/// The step.
pub struct Encryption {
	tools: Tools,
	air: Arc<dyn Air>,
	identity: Arc<dyn Identity>,
	medium: PathBuf,
}

impl Encryption {
	/// A step encrypting the system on the disk `storage` finds, and keeping the board's record under
	/// `medium`.
	pub fn new(
		storage: Arc<dyn Storage>,
		luks: Arc<dyn Luks>,
		initramfs: Arc<dyn Initramfs>,
		air: Arc<dyn Air>,
		identity: Arc<dyn Identity>,
		medium: impl AsRef<Path>,
	) -> Self {
		Self::with_images(storage, luks, initramfs, air, identity, medium, IMAGES)
	}

	fn with_images(
		storage: Arc<dyn Storage>,
		luks: Arc<dyn Luks>,
		initramfs: Arc<dyn Initramfs>,
		air: Arc<dyn Air>,
		identity: Arc<dyn Identity>,
		medium: impl AsRef<Path>,
		images: impl AsRef<Path>,
	) -> Self {
		Self {
			tools: Tools {
				storage,
				luks,
				initramfs,
				images: images.as_ref().to_owned(),
			},
			air,
			identity,
			medium: medium.as_ref().to_owned(),
		}
	}

	async fn tpm(&self) -> Result<(), String> {
		let identity = self.identity.clone();
		blocking(move || identity.tpm()).await
	}
}

#[async_trait]
impl Step for Encryption {
	fn id(&self) -> &str {
		"encryption"
	}

	fn title(&self) -> &str {
		"Set up disk encryption"
	}

	// spec: PENC#done-state
	async fn done(&self) -> Done {
		let unknown = |why: String| Done {
			state: DoneState::Unknown,
			why: Some(why),
		};
		if let Err(why) = self.tpm().await {
			return unknown(why);
		}
		let tools = self.tools.clone();
		match blocking(move || tools.lacks()).await {
			Ok(lacks) if lacks.is_empty() => Done {
				state: DoneState::Done,
				why: Some("the volume has a key of its own and unlocks from the TPM".to_owned()),
			},
			Ok(lacks) => Done {
				state: DoneState::NotDone,
				why: Some(lacks.join("; ")),
			},
			Err(why) => unknown(format!("cannot read the disk: {why}")),
		}
	}

	// spec: PENC#carrying-out-the-step
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		self.tpm().await?;
		let suffix = match self.identity.suffix().await {
			Suffix::Board(suffix) => suffix,
			Suffix::Fault => {
				return Err(
					"the board's ID could not be established, so it has no suffix".to_owned(),
				);
			}
		};
		let version = place(&self.air).await.map(|version| version.to_string());
		let (tools, medium) = (self.tools.clone(), self.medium.clone());
		let record = blocking(move || tools.carry_out(&suffix, version, &medium)).await?;
		if let Some(record) = record {
			context.result(record.result());
		}
		Ok(())
	}
}

impl Tools {
	/// The disk and the partition of its root volume.
	fn root_partition(&self) -> Result<(PathBuf, PathBuf), String> {
		let disk = single_disk(&*self.storage)?;
		let part = partition(&disk, ROOT);
		Ok((disk, part))
	}

	/// The header of the root volume of the payload.
	fn payload_volume(&self) -> Result<Header, String> {
		let manifest = Manifest::read(&self.images.join("manifest"))?;
		let payload = self.storage.payload(&manifest)?;
		let part = payload.root()?;
		self.luks
			.header(&part)?
			.ok_or_else(|| "the payload holds no encrypted root volume".to_owned())
	}

	/// What the disk's root volume lacks of what the step leaves, a phrase for each.
	fn lacks(&self) -> Result<Vec<String>, String> {
		let (_, part) = self.root_partition()?;
		let Some(header) = self.luks.header(&part)? else {
			return Ok(vec!["the disk holds no encrypted root volume".to_owned()]);
		};
		if self.luks.unlocks(&part, &Key::Empty, None)? {
			return Ok(vec![
				"the empty passphrase still unlocks the volume".to_owned(),
			]);
		}
		if !self.luks.unlocks(&part, &Key::Tpm, None).unwrap_or(false) {
			return Ok(vec![
				"the board's TPM does not unlock the volume".to_owned(),
			]);
		}
		if header.reencrypting || header.digest == self.payload_volume()?.digest {
			return Ok(vec![
				"the volume still holds the key of the image".to_owned(),
			]);
		}
		Ok(Vec::new())
	}

	/// Do what [Carrying out the step](PENC) has the step do, and give the record it saved.
	fn carry_out(
		&self,
		suffix: &str,
		version: Option<String>,
		medium: &Path,
	) -> Result<Option<Record>, String> {
		let (disk, part) = self.root_partition()?;
		let header = self
			.luks
			.header(&part)?
			.ok_or("the disk holds no encrypted root volume: write the system to it first")?;

		if !self.luks.unlocks(&part, &Key::Empty, None)? {
			// Set up already, or a volume nothing here opens.
			return if self.lacks()?.is_empty() {
				Ok(None)
			} else {
				Err("the root volume could not be opened".to_owned())
			};
		}

		self.preflight(&disk)?;
		let image = self.payload_volume()?;
		let header = self.rotate(&part, header, &image)?;

		let passphrase = passphrase::generate();
		for slot in &header.slots {
			if !self.luks.unlocks(&part, &Key::Empty, Some(slot.id))? {
				self.luks.remove(&part, slot.id, &Key::Empty)?;
			}
		}
		self.luks.add_passphrase(&part, &passphrase)?;
		self.luks.enrol_tpm(&part, &passphrase)?;

		self.make_system_unlock_from_tpm(&disk)?;
		self.check_keys(&part, &passphrase)?;

		let uuid = self.header(&part)?.uuid;
		let record = Record {
			suffix: suffix.to_owned(),
			version,
			disk: uuid,
			passphrase: passphrase.clone(),
			at: jiff::Timestamp::now()
				.strftime("%Y-%m-%dT%H:%M:%SZ")
				.to_string(),
		};
		record
			.save(medium)
			.map_err(|err| format!("cannot save the record to the provisioning medium: {err}"))?;

		for slot in self.header(&part)?.slots {
			if self.luks.unlocks(&part, &Key::Empty, Some(slot.id))? {
				self.luks
					.remove(&part, slot.id, &Key::Passphrase(&passphrase))?;
			}
		}
		if self.luks.unlocks(&part, &Key::Empty, None)?
			|| !self.luks.unlocks(&part, &Key::Tpm, None)?
		{
			return Err("the empty passphrase could not be replaced by the TPM".to_owned());
		}
		Ok(Some(record))
	}

	fn header(&self, part: &Path) -> Result<Header, String> {
		self.luks
			.header(part)?
			.ok_or_else(|| "the root volume has gone".to_owned())
	}

	/// Check, before anything is changed, that the system on the disk can be made to unlock from the
	/// TPM.
	fn preflight(&self, disk: &Path) -> Result<(), String> {
		let firmware = self
			.storage
			.mount(&partition(disk, FIRMWARE), "vfat", "ro")
			.map_err(|err| format!("the disk's firmware partition could not be mounted: {err}"))?;
		let root = open_root(&*self.storage, &self.luks, disk, false)?;
		check(root.path(), firmware.path(), &*self.initramfs).map(drop)
	}

	/// Give the volume a key of its own and a UUID of its own, where it still has the image's.
	fn rotate(&self, part: &Path, header: Header, image: &Header) -> Result<Header, String> {
		let mut header = header;
		if header.reencrypting || header.digest == image.digest {
			self.luks.reencrypt(part)?;
			header = self.header(part)?;
			if header.reencrypting || header.digest == image.digest {
				return Err("the volume was not re-encrypted under a new key".to_owned());
			}
		}
		if header.uuid == image.uuid {
			self.luks.set_uuid(part, &random_uuid())?;
			header = self.header(part)?;
		}
		Ok(header)
	}

	fn make_system_unlock_from_tpm(&self, disk: &Path) -> Result<(), String> {
		let firmware = self
			.storage
			.mount(&partition(disk, FIRMWARE), "vfat", "rw")
			.map_err(|err| format!("the disk's firmware partition could not be mounted: {err}"))?;
		let root = open_root(&*self.storage, &self.luks, disk, true)?;
		rewire(root.path(), firmware.path(), &*self.initramfs)
	}

	/// Check that the recovery passphrase and the TPM each unlock the volume, and that the TPM's
	/// keyslot is bound to no PCR.
	fn check_keys(&self, part: &Path, passphrase: &str) -> Result<(), String> {
		if !self
			.luks
			.unlocks(part, &Key::Passphrase(passphrase), None)?
		{
			return Err("the recovery passphrase does not unlock the volume".to_owned());
		}
		if !self.luks.unlocks(part, &Key::Tpm, None)? {
			return Err("the board's TPM does not unlock the volume".to_owned());
		}
		let header = self.header(part)?;
		if header
			.slots
			.iter()
			.any(|slot| slot.tpm.as_ref().is_some_and(|pcrs| !pcrs.is_empty()))
		{
			return Err("the TPM keyslot is bound to PCRs".to_owned());
		}
		Ok(())
	}
}

/// A version 4 UUID, in lowercase.
fn random_uuid() -> String {
	let mut bytes: [u8; 16] = rand::random();
	bytes[6] = (bytes[6] & 0x0f) | 0x40;
	bytes[8] = (bytes[8] & 0x3f) | 0x80;
	let hex = hex::encode(bytes);
	format!(
		"{}-{}-{}-{}-{}",
		&hex[..8],
		&hex[8..12],
		&hex[12..16],
		&hex[16..20],
		&hex[20..]
	)
}

#[cfg(test)]
mod tests;
