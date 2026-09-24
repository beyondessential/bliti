//! Establishing a device's own keys: which source wins, whether the cached root still holds, the
//! derivation when it does not, and the presence token and static key descending from the root.
//!
//! Behaviour is specified in BID and in KEY, "Deriving on the device". The memory-hard
//! derivation of the root is paid once and cached; establishing whether the cache still holds is a
//! comparison of cheap reads against the board, not a rederivation. The presence token and static key
//! are derived from the root on every start, which is cheap.

use std::{
	fs,
	path::{Path, PathBuf},
};

use bliti_core::{
	board_id::{
		BoardId, BoardIdSource, CacheDecision, CacheState, OneTimeProgrammableSource,
		PlatformSerial, RaspberryPiSerialSource, SourceKind, evaluate_cache, select,
		strongest_present,
	},
	key_schedule::{DeviceKeys, ROOT_LEN, Root, VERSION, check_memory, derive_root},
};
use serde::{Deserialize, Serialize};

/// Where the derived root and the board it was derived from are cached. It is a cache the device
/// can rebuild, not authoritative state: losing it costs one derivation.
pub const DEFAULT_CACHE_PATH: &str = "/var/lib/bliti/identity.json";

/// The device's own identity, once established.
pub struct Identity {
	/// The presence token and device static key this board derives.
	pub keys: DeviceKeys,
	/// Which kind of source it was derived from.
	pub kind: SourceKind,
	/// Whether the memory-hard derivation had to run, rather than the cache standing.
	pub derived: bool,
}

/// The cache file's contents.
///
/// A file that does not have this shape, as one written before the cache held a root, fails to parse
/// and is rederived.
#[derive(Debug, Serialize, Deserialize)]
struct CacheFile {
	/// The key-schedule version the root was derived under. A device that finds a different one has
	/// been upgraded across a version change and rederives.
	version: u8,
	/// Which kind of source won the precedence.
	kind: String,
	/// The board ID the root was derived from: the winning source's raw value, hex-encoded.
	board_id: String,
	/// The platform serial of the board, hex-encoded, or absent where the board offers none.
	platform_serial: Option<String>,
	/// The derived root, hex-encoded.
	root: String,
}

/// Assemble the board-ID backends this build can see.
///
/// Which backends are registered decides which source wins the precedence, and so which root the
/// board derives, which is why [`guard_unreadable_sources`] exists.
pub fn sources() -> Vec<Box<dyn BoardIdSource>> {
	#[cfg_attr(
		not(feature = "tpm"),
		expect(
			unused_mut,
			reason = "the TPM source is pushed only when that feature is on"
		)
	)]
	let mut sources: Vec<Box<dyn BoardIdSource>> = vec![
		Box::new(OneTimeProgrammableSource::new()),
		Box::new(RaspberryPiSerialSource::new()),
	];
	#[cfg(feature = "tpm")]
	sources.push(Box::new(
		bliti_core::board_id::TpmEndorsementKeySource::new(),
	));
	sources
}

/// Refuse to derive on a board carrying a source this build cannot read.
///
/// A build without the `tpm` feature cannot see a TPM, so on a board that has one it would derive
/// from the serial number instead and produce a root that does not match the QR code on the
/// enclosure. That is worse than not starting, because the device would advertise a handle nobody
/// can match while looking healthy, so it is refused here.
pub fn guard_unreadable_sources() -> Result<(), IdentityError> {
	#[cfg(not(feature = "tpm"))]
	for node in ["/dev/tpmrm0", "/dev/tpm0"] {
		if Path::new(node).exists() {
			return Err(IdentityError::UnreadableSource {
				kind: "TPM",
				detail: format!(
					"{node} is present, but this build was made without TPM support, so it would \
					 derive from a weaker source and not match this board's QR code"
				),
			});
		}
	}
	Ok(())
}

/// Read the board's platform serial, which identifies it across a change of winning source. Cheap,
/// and read on every start.
pub fn platform_serial(
	sources: &[Box<dyn BoardIdSource>],
) -> Result<PlatformSerial, IdentityError> {
	for source in sources {
		if source.kind().is_platform_serial()
			&& source.probe().map_err(IdentityError::BoardId)?
				== bliti_core::board_id::Presence::Present
		{
			return Ok(Some(source.read().map_err(IdentityError::BoardId)?));
		}
	}
	Ok(None)
}

/// Establish the device's keys, deriving the root only where the cache does not hold.
pub fn establish(cache_path: &Path) -> Result<Identity, IdentityError> {
	guard_unreadable_sources()?;

	let sources = sources();
	let refs: Vec<&dyn BoardIdSource> = sources.iter().map(AsRef::as_ref).collect();

	// Both of these are cheap: a file read and a set of presence probes, with no source value read
	// and no key generation inside a TPM.
	let serial = platform_serial(&sources)?;
	let strongest = strongest_present(&refs).map_err(IdentityError::BoardId)?;
	let cached = read_cache(cache_path)?;

	match evaluate_cache(cached.as_ref().map(|(state, _)| state), &serial, strongest) {
		CacheDecision::Fresh => {
			let (state, root) = cached.expect("a fresh cache was read");
			Ok(Identity {
				keys: root.device_keys(),
				kind: state.board_id_kind,
				derived: false,
			})
		}
		CacheDecision::QrDead => Err(IdentityError::QrDead {
			cached: cached.map(|(state, _)| state.board_id_kind),
			found: strongest,
		}),
		CacheDecision::Rederive => {
			let board_id = select(&refs).map_err(IdentityError::BoardId)?;

			// The derivation needs its full memory parameter at once and is killed by the operating
			// system rather than told the allocation failed, so establish there is room first.
			check_memory(available_memory_bytes()).map_err(IdentityError::Key)?;

			let root = derive_root(&board_id).map_err(IdentityError::Key)?;
			write_cache(cache_path, &board_id, &serial, &root)?;
			Ok(Identity {
				keys: root.device_keys(),
				kind: board_id.kind(),
				derived: true,
			})
		}
	}
}

/// Bytes of memory available, from the kernel's own estimate of what can be allocated without
/// swapping. `MemAvailable` is the right figure rather than `MemFree`, which ignores reclaimable
/// cache.
fn available_memory_bytes() -> u64 {
	fs::read_to_string("/proc/meminfo")
		.ok()
		.and_then(|meminfo| {
			meminfo.lines().find_map(|line| {
				let rest = line.strip_prefix("MemAvailable:")?;
				let kib: u64 = rest.split_whitespace().next()?.parse().ok()?;
				Some(kib * 1024)
			})
		})
		.unwrap_or(u64::MAX)
}

fn kind_name(kind: SourceKind) -> &'static str {
	match kind {
		SourceKind::TpmEndorsementKey => "tpm-endorsement-key",
		SourceKind::OneTimeProgrammable => "one-time-programmable",
		SourceKind::RaspberryPiSerial => "raspberry-pi-serial",
	}
}

fn kind_from_name(name: &str) -> Option<SourceKind> {
	SourceKind::ALL.into_iter().find(|k| kind_name(*k) == name)
}

/// Read the cache, or `None` where it is absent or does not apply. A cache that cannot be understood
/// is treated as absent: it costs one derivation to rebuild, which is better than refusing to start.
fn read_cache(path: &Path) -> Result<Option<(CacheState, Root)>, IdentityError> {
	let raw = match fs::read_to_string(path) {
		Ok(raw) => raw,
		Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		Err(err) => return Err(IdentityError::Cache(err.to_string())),
	};
	let Ok(file) = serde_json::from_str::<CacheFile>(&raw) else {
		tracing::warn!("identity cache is unreadable; rederiving");
		return Ok(None);
	};
	if file.version != VERSION {
		tracing::warn!(
			cached = file.version,
			current = VERSION,
			"identity cache is from another key-schedule version; rederiving"
		);
		return Ok(None);
	}
	let (Some(kind), Ok(root)) = (kind_from_name(&file.kind), hex::decode(&file.root)) else {
		tracing::warn!("identity cache is unreadable; rederiving");
		return Ok(None);
	};
	let Ok(root): Result<[u8; ROOT_LEN], _> = root.try_into() else {
		tracing::warn!("identity cache holds a root of the wrong length; rederiving");
		return Ok(None);
	};
	if hex::decode(&file.board_id).is_err() {
		tracing::warn!("identity cache is unreadable; rederiving");
		return Ok(None);
	}
	let platform_serial = match file.platform_serial.as_deref().map(hex::decode) {
		Some(Ok(serial)) => Some(serial),
		Some(Err(_)) => return Ok(None),
		None => None,
	};
	Ok(Some((
		CacheState {
			board_id_kind: kind,
			platform_serial,
		},
		Root::from_bytes(root),
	)))
}

fn write_cache(
	path: &Path,
	board_id: &BoardId,
	platform_serial: &PlatformSerial,
	root: &Root,
) -> Result<(), IdentityError> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|err| IdentityError::Cache(err.to_string()))?;
	}
	let file = CacheFile {
		version: VERSION,
		kind: kind_name(board_id.kind()).to_owned(),
		board_id: hex::encode(board_id.raw()),
		platform_serial: platform_serial.as_ref().map(hex::encode),
		root: hex::encode(root.as_bytes()),
	};
	let body =
		serde_json::to_string_pretty(&file).map_err(|err| IdentityError::Cache(err.to_string()))?;

	// Write through a temporary file in the same directory, so a start interrupted part-way leaves
	// either the old cache or the new one rather than a truncated file.
	let temporary = path.with_extension("json.new");
	fs::write(&temporary, body).map_err(|err| IdentityError::Cache(err.to_string()))?;
	restrict(&temporary)?;
	fs::rename(&temporary, path).map_err(|err| IdentityError::Cache(err.to_string()))
}

/// The cache holds the root, from which both credentials descend, and the board ID, from which the
/// root does, so it is readable only by the user the daemon runs as.
fn restrict(path: &Path) -> Result<(), IdentityError> {
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(path, fs::Permissions::from_mode(0o600))
			.map_err(|err| IdentityError::Cache(err.to_string()))?;
	}
	Ok(())
}

/// The default cache path, overridable for testing and for running unprivileged.
pub fn default_cache_path() -> PathBuf {
	PathBuf::from(DEFAULT_CACHE_PATH)
}

/// A failure establishing the device's identity. These leave the device unreachable over the channel,
/// so they are reported where the device is rather than to a client (BLI, "Reporting").
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
	/// No usable board ID source, or a backend failed.
	#[error(transparent)]
	BoardId(bliti_core::board_id::BoardIdError),

	/// The derivation could not run.
	#[error(transparent)]
	Key(bliti_core::key_schedule::KeyError),

	/// The board has gained hardware carrying a stronger source, so the QR code on its enclosure is
	/// dead and no client can reach it. Recovering means printing a new QR code for this board.
	#[error(
		"this board's QR code is dead: it was derived from {cached:?} but the board now offers \
		 {found:?}, so the printed QR code no longer matches it. Print a new QR code for this board."
	)]
	QrDead {
		/// The kind of source the cached root was derived from.
		cached: Option<SourceKind>,
		/// The strongest kind of source the board offers now.
		found: Option<SourceKind>,
	},

	/// This build cannot read a source the board carries, so deriving would give the wrong root.
	#[cfg_attr(
		feature = "tpm",
		expect(dead_code, reason = "only raised by a build without TPM support")
	)]
	#[error("refusing to derive: {detail}")]
	UnreadableSource {
		/// The kind of source that cannot be read.
		kind: &'static str,
		/// What was found and why it is refused.
		detail: String,
	},

	/// The cache could not be read or written.
	#[error("identity cache: {0}")]
	Cache(String),
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A scratch cache path, its directory cleaned up on drop.
	struct Scratch(PathBuf);

	impl Scratch {
		fn new() -> Self {
			use std::sync::atomic::{AtomicU32, Ordering};
			static COUNTER: AtomicU32 = AtomicU32::new(0);
			let n = COUNTER.fetch_add(1, Ordering::Relaxed);
			let dir =
				std::env::temp_dir().join(format!("bliti-identity-{}-{n}", std::process::id()));
			fs::create_dir_all(&dir).unwrap();
			Self(dir)
		}

		fn path(&self) -> PathBuf {
			self.0.join("identity.json")
		}
	}

	impl Drop for Scratch {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn the_cache_holds_the_root_and_the_board_it_came_from() {
		let scratch = Scratch::new();
		let board_id =
			BoardId::new(SourceKind::RaspberryPiSerial, vec![0xf3, 0x75, 0x65, 0x10]).unwrap();
		let serial = Some(vec![0xf3, 0x75, 0x65, 0x10]);
		let root = Root::from_bytes([0x42; ROOT_LEN]);
		write_cache(&scratch.path(), &board_id, &serial, &root).unwrap();

		let file: serde_json::Value =
			serde_json::from_str(&fs::read_to_string(scratch.path()).unwrap()).unwrap();
		assert_eq!(file["root"], hex::encode([0x42; ROOT_LEN]));
		assert_eq!(file["board_id"], "f3756510");
		assert_eq!(file["platform_serial"], "f3756510");
		assert_eq!(file["kind"], "raspberry-pi-serial");

		let (state, read) = read_cache(&scratch.path()).unwrap().unwrap();
		assert_eq!(read, root);
		assert_eq!(state.board_id_kind, SourceKind::RaspberryPiSerial);
		assert_eq!(state.platform_serial, serial);
	}

	#[test]
	fn a_cache_holding_a_token_rather_than_a_root_is_rederived() {
		let scratch = Scratch::new();
		fs::write(
			scratch.path(),
			serde_json::json!({
				"version": VERSION,
				"kind": "raspberry-pi-serial",
				"platform_serial": "f3756510",
				"secret": hex::encode([0x42; 32]),
			})
			.to_string(),
		)
		.unwrap();
		assert!(read_cache(&scratch.path()).unwrap().is_none());
	}
}
