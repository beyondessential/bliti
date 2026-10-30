//! The root volume of a disk, a LUKS2 volume, as the step that encrypts it reaches it (PENC).
//!
//! The step works through [`Luks`], so that it is tested on a volume held in memory. [`Cryptsetup`]
//! is the real one.

use std::path::{Path, PathBuf};

pub use self::cryptsetup::Cryptsetup;

mod cryptsetup;

/// A way of unlocking a volume.
#[derive(Debug, Clone, Copy)]
pub enum Key<'a> {
	/// The empty passphrase.
	Empty,
	/// A passphrase.
	Passphrase(&'a str),
	/// The board's TPM, through a TPM keyslot.
	Tpm,
}

/// What a volume's header holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
	/// The volume's UUID, in lowercase.
	pub uuid: String,
	/// The digest that stands for the volume key: two volumes hold one volume key only if they hold
	/// one digest.
	pub digest: String,
	/// The keyslots.
	pub slots: Vec<Slot>,
	/// Whether a re-encryption of the volume has begun and not finished.
	pub reencrypting: bool,
}

/// A keyslot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
	/// Its number.
	pub id: u32,
	/// Where it is a TPM keyslot, the PCRs it is bound to.
	pub tpm: Option<Vec<u32>>,
}

/// A LUKS2 volume's header, keys and unlocking, each call blocking and saying why it failed.
pub trait Luks: Send + Sync {
	/// The header of the volume on `partition`, or nothing where it holds no LUKS2 volume.
	fn header(&self, partition: &Path) -> Result<Option<Header>, String>;

	/// Whether `key` unlocks the volume on `partition`, through the keyslot `slot` where one is given.
	fn unlocks(&self, partition: &Path, key: &Key<'_>, slot: Option<u32>) -> Result<bool, String>;

	/// Re-encrypt the volume under a new volume key, unlocked by the empty passphrase, or resume a
	/// re-encryption that was interrupted.
	fn reencrypt(&self, partition: &Path) -> Result<(), String>;

	/// Give the volume the UUID `uuid`.
	fn set_uuid(&self, partition: &Path, uuid: &str) -> Result<(), String>;

	/// Add a keyslot that `passphrase` unlocks, authorised by the empty passphrase.
	fn add_passphrase(&self, partition: &Path, passphrase: &str) -> Result<(), String>;

	/// Add a TPM keyslot bound to no PCR, authorised by `passphrase`.
	fn enrol_tpm(&self, partition: &Path, passphrase: &str) -> Result<(), String>;

	/// Remove the keyslot `slot`, authorised by `key`.
	fn remove(&self, partition: &Path, slot: u32, key: &Key<'_>) -> Result<(), String>;

	/// Unlock the volume with `key` as the mapping `name`, and give the node of the mapping.
	fn open(&self, partition: &Path, name: &str, key: &Key<'_>) -> Result<PathBuf, String>;

	/// Close the mapping `name`.
	fn close(&self, name: &str);
}
