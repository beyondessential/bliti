//! What the tests of the steps that write and encrypt the disk share: a disk, a medium and a volume
//! held in files, directories and memory.

use std::{
	fs,
	io::{self, Cursor, Read},
	path::{Path, PathBuf},
	sync::{Arc, Mutex},
};

use sha2::{Digest as _, Sha256};

use super::{
	luks::{Header, Key, Luks, Slot},
	manifest::Manifest,
	storage::{FIRMWARE, Mounted, Payload, ROOT, Storage, partition},
	system::Initramfs,
};

/// The payload's size: three blocks, which the disk is made twice the size of.
pub const PAYLOAD_SIZE: usize = 3 * 4096;

/// What opening the root volume maps it as, and so where its device node is.
const MAPPER: &str = "/dev/mapper/bliti-root";

/// What a keyslot of the fake volume is unlocked by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unlock {
	Empty,
	Passphrase(String),
	/// The TPM, which is this board's where `ours`.
	Tpm {
		ours: bool,
		pcrs: Vec<u32>,
	},
}

/// A LUKS volume held in memory.
#[derive(Debug, Clone)]
pub struct Volume {
	pub uuid: String,
	pub digest: String,
	pub slots: Vec<(u32, Unlock)>,
	pub reencrypting: bool,
}

impl Volume {
	/// The volume of the image as it ships.
	pub fn shipped() -> Self {
		Self {
			uuid: "image-uuid".to_owned(),
			digest: "image-key".to_owned(),
			slots: vec![(0, Unlock::Empty)],
			reencrypting: false,
		}
	}

	fn header(&self) -> Header {
		Header {
			uuid: self.uuid.clone(),
			digest: self.digest.clone(),
			slots: self
				.slots
				.iter()
				.map(|(id, unlock)| Slot {
					id: *id,
					tpm: match unlock {
						Unlock::Tpm { pcrs, .. } => Some(pcrs.clone()),
						_ => None,
					},
				})
				.collect(),
			reencrypting: self.reencrypting,
		}
	}

	fn unlocked_by(&self, key: &Key<'_>, slot: Option<u32>) -> bool {
		self.slots.iter().any(|(id, unlock)| {
			slot.is_none_or(|slot| slot == *id)
				&& match (key, unlock) {
					(Key::Empty, Unlock::Empty) => true,
					(Key::Passphrase(given), Unlock::Passphrase(held)) => given == held,
					(Key::Tpm, Unlock::Tpm { ours, .. }) => *ours,
					_ => false,
				}
		})
	}

	/// Whether the volume opens by any means a person or the board has.
	pub fn opens(&self, passphrase: Option<&str>) -> bool {
		self.unlocked_by(&Key::Empty, None)
			|| self.unlocked_by(&Key::Tpm, None)
			|| passphrase
				.is_some_and(|passphrase| self.unlocked_by(&Key::Passphrase(passphrase), None))
	}

	pub fn unlocked_by_key(&self, key: &Key<'_>) -> bool {
		self.unlocked_by(key, None)
	}
}

/// Everything the fakes share, which a test reads and changes.
pub struct State {
	pub disks: Vec<PathBuf>,
	/// The labels the disk reports, where it reports any other than the image's own.
	pub labels: Option<Vec<String>>,
	pub firmware_mountable: bool,
	/// The payload fails to read after this many bytes.
	pub payload_fails_at: Option<usize>,
	/// The volume on the disk's root partition, if it holds one.
	pub volume: Option<Volume>,
	/// The root volume of the payload.
	pub payload_volume: Volume,
	/// Whether the board's TPM answers.
	pub tpm_answers: bool,
	/// Whether the system can make an initramfs that unlocks from the TPM.
	pub can_unlock_from_tpm: bool,
	/// How many operations that change the volume or the system may still run before one is
	/// interrupted.
	pub budget: Option<usize>,
	/// How many mappings and mounts are open.
	pub mappings: usize,
	pub mounts: usize,
	/// What the initramfs was made for, each time.
	pub initramfs_made: Vec<String>,
	/// The PCRs a TPM keyslot is bound to when it is enrolled.
	pub enrol_pcrs: Vec<u32>,
}

impl State {
	/// Whether an operation may go on, spending from `budget`.
	fn spend(&mut self, what: &str) -> Result<(), String> {
		match &mut self.budget {
			Some(0) => Err(format!("interrupted at {what}")),
			Some(left) => {
				*left -= 1;
				Ok(())
			}
			None => Ok(()),
		}
	}
}

/// A disk, a medium and a volume.
pub struct Bench {
	_dir: tempfile::TempDir,
	pub disk: PathBuf,
	pub firmware: PathBuf,
	pub root: PathBuf,
	pub images: PathBuf,
	pub medium: PathBuf,
	pub payload: Vec<u8>,
	pub state: Arc<Mutex<State>>,
}

impl Bench {
	pub fn new() -> Self {
		let dir = tempfile::tempdir().expect("a directory");
		let at = |name: &str| {
			let path = dir.path().join(name);
			fs::create_dir_all(&path).expect("a directory");
			path
		};
		let (firmware, root, images, medium) =
			(at("firmware"), at("root"), at("images"), at("medium"));

		let payload: Vec<u8> = (0..PAYLOAD_SIZE).map(|i| (i % 251) as u8 + 1).collect();
		let disk = dir.path().join("nvme0n1");
		fs::write(&disk, vec![0u8; PAYLOAD_SIZE * 2]).expect("a disk");

		fs::create_dir_all(firmware.join("current/overlays")).unwrap();
		fs::write(firmware.join("current/overlays/README"), "overlays").unwrap();
		fs::write(firmware.join("current/initrd.img"), "old initramfs").unwrap();
		fs::create_dir_all(root.join("etc/dracut.conf.d")).unwrap();
		fs::write(
			root.join("etc/crypttab"),
			"root /dev/disk/by-partlabel/root /etc/luks/empty-keyfile luks\n",
		)
		.unwrap();
		fs::write(
			root.join("etc/dracut.conf.d/02-luks-keyfile.conf"),
			"install_items+=\" /etc/luks/empty-keyfile \"\n",
		)
		.unwrap();
		fs::create_dir_all(root.join("usr/lib/modules/6.17.0-test")).unwrap();

		let sha256 = hex::encode(Sha256::digest(&payload));
		fs::write(
			images.join("manifest"),
			format!(
				"BLITI_COMMIT=abc\nBUILT=2026-10-29T01:49:00Z\nPI_IMAGE_VERSION=v1.2.3\nPI_IMAGE_SIZE={PAYLOAD_SIZE}\nPI_IMAGE_SHA256={sha256}\nPAYLOAD_HASH_OFFSET={PAYLOAD_SIZE}\nPAYLOAD_ROOT_HASH=00ff\nBOOTLOADER_VERSION=2026-10-29\n"
			),
		)
		.unwrap();
		fs::write(images.join("tamanu-iti-v4.dtbo"), b"the HAT overlay").unwrap();

		let state = State {
			disks: vec![disk.clone()],
			labels: None,
			firmware_mountable: true,
			payload_fails_at: None,
			volume: None,
			payload_volume: Volume::shipped(),
			tpm_answers: true,
			can_unlock_from_tpm: true,
			budget: None,
			mappings: 0,
			mounts: 0,
			initramfs_made: Vec::new(),
			enrol_pcrs: Vec::new(),
		};
		Self {
			_dir: dir,
			disk,
			firmware,
			root,
			images,
			medium,
			payload,
			state: Arc::new(Mutex::new(state)),
		}
	}

	pub fn storage(&self) -> Arc<dyn Storage> {
		Arc::new(FakeStorage {
			disk: self.disk.clone(),
			firmware: self.firmware.clone(),
			root: self.root.clone(),
			payload: self.payload.clone(),
			state: self.state.clone(),
		})
	}

	pub fn luks(&self) -> Arc<dyn Luks> {
		Arc::new(FakeLuks {
			state: self.state.clone(),
			disk: self.disk.clone(),
		})
	}

	pub fn initramfs(&self) -> Arc<dyn Initramfs> {
		Arc::new(FakeInitramfs {
			state: self.state.clone(),
		})
	}

	pub fn manifest(&self) -> Manifest {
		Manifest::read(&self.images.join("manifest")).expect("a manifest")
	}

	/// Write the first `len` bytes of the payload to the disk, as the step does.
	pub fn write_payload(&self, len: usize) {
		let mut bytes = fs::read(&self.disk).unwrap();
		bytes[..len].copy_from_slice(&self.payload[..len]);
		fs::write(&self.disk, bytes).unwrap();
	}

	pub fn marker(&self) -> Option<String> {
		fs::read_to_string(self.firmware.join("iti-image.txt")).ok()
	}

	/// Put a volume on the disk, as the image ships it.
	pub fn shipped(&self) {
		self.state.lock().unwrap().volume = Some(Volume::shipped());
	}

	pub fn volume(&self) -> Volume {
		self.state.lock().unwrap().volume.clone().expect("a volume")
	}

	pub fn records(&self) -> String {
		fs::read_to_string(self.medium.join("records.tsv")).unwrap_or_default()
	}
}

struct FakeStorage {
	disk: PathBuf,
	firmware: PathBuf,
	root: PathBuf,
	payload: Vec<u8>,
	state: Arc<Mutex<State>>,
}

impl Storage for FakeStorage {
	fn nvme(&self) -> Result<Vec<PathBuf>, String> {
		Ok(self.state.lock().unwrap().disks.clone())
	}

	fn size(&self, disk: &Path) -> Result<u64, String> {
		fs::metadata(disk)
			.map(|meta| meta.len())
			.map_err(|err| err.to_string())
	}

	fn blank(&self, disk: &Path) -> Result<bool, String> {
		let bytes = fs::read(disk).map_err(|err| err.to_string())?;
		Ok(bytes.iter().take(4096).all(|byte| *byte == 0))
	}

	fn labels(&self, disk: &Path) -> Result<Vec<String>, String> {
		if let Some(labels) = &self.state.lock().unwrap().labels {
			return Ok(labels.clone());
		}
		let bytes = fs::read(disk).map_err(|err| err.to_string())?;
		Ok(if bytes[..4096] == self.payload[..4096] {
			["firmware", "xboot", "root"].map(str::to_owned).to_vec()
		} else {
			Vec::new()
		})
	}

	fn rescan(&self, _disk: &Path) -> Result<(), String> {
		Ok(())
	}

	fn mount(&self, device: &Path, _fstype: &str, _options: &str) -> Result<Mounted, String> {
		let mut state = self.state.lock().unwrap();
		let path = if device == partition(&self.disk, FIRMWARE) {
			if !state.firmware_mountable {
				return Err("no filesystem".to_owned());
			}
			self.firmware.clone()
		} else if device == Path::new(MAPPER) {
			if state.mappings == 0 {
				return Err("no such device".to_owned());
			}
			self.root.clone()
		} else {
			return Err(format!("cannot mount {}", device.display()));
		};
		state.mounts += 1;
		let shared = self.state.clone();
		Ok(Mounted::new(path, move || {
			shared.lock().unwrap().mounts -= 1;
		}))
	}

	fn payload(&self, _manifest: &Manifest) -> Result<Box<dyn Payload>, String> {
		let fails_at = self.state.lock().unwrap().payload_fails_at;
		Ok(Box::new(FakePayload {
			bytes: Cursor::new(self.payload.clone()),
			fails_at,
		}))
	}
}

struct FakePayload {
	bytes: Cursor<Vec<u8>>,
	fails_at: Option<usize>,
}

impl Read for FakePayload {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		let at = self.bytes.position() as usize;
		match self.fails_at {
			Some(limit) if at >= limit => Err(io::Error::other("a block failed its check")),
			Some(limit) => {
				let room = (limit - at).min(buf.len());
				self.bytes.read(&mut buf[..room])
			}
			None => self.bytes.read(buf),
		}
	}
}

impl Payload for FakePayload {
	fn root(&self) -> Result<PathBuf, String> {
		Ok(PathBuf::from("/fake/payload-root"))
	}
}

struct FakeLuks {
	state: Arc<Mutex<State>>,
	disk: PathBuf,
}

impl FakeLuks {
	fn is_root(&self, node: &Path) -> bool {
		node == partition(&self.disk, ROOT)
	}
}

impl Luks for FakeLuks {
	fn header(&self, node: &Path) -> Result<Option<Header>, String> {
		let state = self.state.lock().unwrap();
		if self.is_root(node) {
			Ok(state.volume.as_ref().map(Volume::header))
		} else if node == Path::new("/fake/payload-root") {
			Ok(Some(state.payload_volume.header()))
		} else {
			Err(format!("no such partition {}", node.display()))
		}
	}

	fn unlocks(&self, node: &Path, key: &Key<'_>, slot: Option<u32>) -> Result<bool, String> {
		let state = self.state.lock().unwrap();
		if matches!(key, Key::Tpm) && !state.tpm_answers {
			return Err("the TPM does not answer".to_owned());
		}
		match (&state.volume, self.is_root(node)) {
			(Some(volume), true) => Ok(volume.unlocked_by(key, slot)),
			_ => Err("no volume".to_owned()),
		}
	}

	fn reencrypt(&self, _node: &Path) -> Result<(), String> {
		let mut state = self.state.lock().unwrap();
		let interrupted = state.spend("re-encryption");
		let volume = state.volume.as_mut().expect("a volume");
		if interrupted.is_err() {
			volume.reencrypting = true;
			return interrupted;
		}
		volume.reencrypting = false;
		volume.digest = format!("fresh-key-{}", volume.uuid);
		Ok(())
	}

	fn set_uuid(&self, _node: &Path, uuid: &str) -> Result<(), String> {
		let mut state = self.state.lock().unwrap();
		state.spend("setting the UUID")?;
		state.volume.as_mut().expect("a volume").uuid = uuid.to_owned();
		Ok(())
	}

	fn add_passphrase(&self, _node: &Path, passphrase: &str) -> Result<(), String> {
		let mut state = self.state.lock().unwrap();
		state.spend("adding the passphrase")?;
		let volume = state.volume.as_mut().expect("a volume");
		if !volume.unlocked_by(&Key::Empty, None) {
			return Err("no key available with this passphrase".to_owned());
		}
		let id = volume.slots.iter().map(|(id, _)| id + 1).max().unwrap_or(0);
		volume
			.slots
			.push((id, Unlock::Passphrase(passphrase.to_owned())));
		Ok(())
	}

	fn enrol_tpm(&self, _node: &Path, passphrase: &str) -> Result<(), String> {
		let mut state = self.state.lock().unwrap();
		state.spend("enrolling the TPM")?;
		if !state.tpm_answers {
			return Err("the TPM does not answer".to_owned());
		}
		let pcrs = state.enrol_pcrs.clone();
		let volume = state.volume.as_mut().expect("a volume");
		if !volume.unlocked_by(&Key::Passphrase(passphrase), None) {
			return Err("no key available with this passphrase".to_owned());
		}
		let id = volume.slots.iter().map(|(id, _)| id + 1).max().unwrap_or(0);
		volume.slots.push((id, Unlock::Tpm { ours: true, pcrs }));
		Ok(())
	}

	fn remove(&self, _node: &Path, slot: u32, key: &Key<'_>) -> Result<(), String> {
		let mut state = self.state.lock().unwrap();
		state.spend("removing a keyslot")?;
		let volume = state.volume.as_mut().expect("a volume");
		if !volume.unlocked_by(key, None) {
			return Err("no key available with this passphrase".to_owned());
		}
		volume.slots.retain(|(id, _)| *id != slot);
		Ok(())
	}

	fn open(&self, node: &Path, _name: &str, key: &Key<'_>) -> Result<PathBuf, String> {
		let mut state = self.state.lock().unwrap();
		match (&state.volume, self.is_root(node)) {
			(Some(volume), true) if volume.unlocked_by(key, None) => {}
			_ => return Err("no key available with this passphrase".to_owned()),
		}
		state.mappings += 1;
		Ok(PathBuf::from(MAPPER))
	}

	fn close(&self, _name: &str) {
		let mut state = self.state.lock().unwrap();
		state.mappings = state.mappings.saturating_sub(1);
	}
}

struct FakeInitramfs {
	state: Arc<Mutex<State>>,
}

impl Initramfs for FakeInitramfs {
	fn can_unlock_from_tpm(&self, _root: &Path) -> bool {
		self.state.lock().unwrap().can_unlock_from_tpm
	}

	fn make(&self, root: &Path, kernel: &str) -> Result<PathBuf, String> {
		let mut state = self.state.lock().unwrap();
		state.spend("making the initramfs")?;
		let crypttab =
			fs::read_to_string(root.join("etc/crypttab")).map_err(|err| err.to_string())?;
		let tpm = crypttab.contains("tpm2-device=auto");
		state.initramfs_made.push(kernel.to_owned());
		let out = root.join("initrd.new");
		fs::write(
			&out,
			format!("initramfs for {kernel}; unlocks from the TPM: {tpm}"),
		)
		.map_err(|err| err.to_string())?;
		Ok(out)
	}
}
