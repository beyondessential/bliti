//! A LUKS2 volume reached with `cryptsetup` and `systemd-cryptenroll`.

use std::{
	path::{Path, PathBuf},
	process::Command,
};

use serde_json::Value as Json;

use super::{Header, Key, Luks, Slot};
use crate::provision::exec::{failure, finish, run, secret};

/// The exit status `cryptsetup` gives where no key it was given unlocks the volume.
const NO_KEY: i32 = 2;

/// The exit status `cryptsetup` gives where a device holds no LUKS volume.
const NOT_LUKS: i32 = 1;

/// The token type `systemd-cryptenroll` makes for a TPM keyslot.
const TPM_TOKEN: &str = "systemd-tpm2";

/// The real thing.
pub struct Cryptsetup;

/// A key as the arguments and the file `cryptsetup` is given it by. The file is kept for as long as
/// the command runs.
struct Arguments {
	args: Vec<String>,
	_file: Option<tempfile::NamedTempFile>,
}

impl Arguments {
	fn of(key: &Key<'_>) -> Result<Self, String> {
		let passphrase = match key {
			Key::Empty => "",
			Key::Passphrase(passphrase) => passphrase,
			Key::Tpm => {
				return Ok(Self {
					args: vec!["--token-only".to_owned()],
					_file: None,
				});
			}
		};
		let file = secret(passphrase)?;
		Ok(Self {
			args: vec![
				"--key-file".to_owned(),
				file.path().to_string_lossy().into_owned(),
			],
			_file: Some(file),
		})
	}
}

impl Luks for Cryptsetup {
	fn header(&self, partition: &Path) -> Result<Option<Header>, String> {
		let mut is_luks = Command::new("cryptsetup");
		is_luks.arg("isLuks").arg(partition);
		let output = finish(&mut is_luks)?;
		match output.status.code() {
			Some(0) => {}
			Some(NOT_LUKS) => return Ok(None),
			_ => return Err(failure(&is_luks, &output)),
		}

		let mut dump = Command::new("cryptsetup");
		dump.arg("luksDump")
			.arg("--dump-json-metadata")
			.arg(partition);
		let json = String::from_utf8(run(&mut dump)?.stdout).map_err(|err| err.to_string())?;
		let mut uuid = Command::new("cryptsetup");
		uuid.arg("luksUUID").arg(partition);
		let uuid = String::from_utf8(run(&mut uuid)?.stdout)
			.map_err(|err| err.to_string())?
			.trim()
			.to_ascii_lowercase();
		parse(&uuid, &json).map(Some)
	}

	fn unlocks(&self, partition: &Path, key: &Key<'_>, slot: Option<u32>) -> Result<bool, String> {
		let arguments = Arguments::of(key)?;
		let mut command = Command::new("cryptsetup");
		command
			.arg("open")
			.arg("--test-passphrase")
			.args(&arguments.args);
		if let (Some(slot), Key::Empty | Key::Passphrase(_)) = (slot, key) {
			command.arg("--key-slot").arg(slot.to_string());
		}
		command.arg(partition);
		let output = finish(&mut command)?;
		match output.status.code() {
			Some(0) => Ok(true),
			Some(NO_KEY) => Ok(false),
			_ => Err(failure(&command, &output)),
		}
	}

	fn reencrypt(&self, partition: &Path) -> Result<(), String> {
		let resume = self
			.header(partition)?
			.is_some_and(|header| header.reencrypting);
		let key = Arguments::of(&Key::Empty)?;
		let mut command = Command::new("cryptsetup");
		command.arg("reencrypt").arg("--batch-mode");
		if resume {
			command.arg("--resume-only");
		}
		command.args(&key.args).arg(partition);
		run(&mut command).map(drop)
	}

	fn set_uuid(&self, partition: &Path, uuid: &str) -> Result<(), String> {
		run(Command::new("cryptsetup")
			.args(["luksUUID", "--batch-mode", "--uuid", uuid])
			.arg(partition))
		.map(drop)
	}

	fn add_passphrase(&self, partition: &Path, passphrase: &str) -> Result<(), String> {
		let existing = Arguments::of(&Key::Empty)?;
		let new = secret(passphrase)?;
		run(Command::new("cryptsetup")
			.args(["luksAddKey", "--batch-mode"])
			.args(&existing.args)
			.arg(partition)
			.arg(new.path()))
		.map(drop)
	}

	fn enrol_tpm(&self, partition: &Path, passphrase: &str) -> Result<(), String> {
		let unlock = secret(passphrase)?;
		let mut command = Command::new("systemd-cryptenroll");
		command
			.arg(format!("--unlock-key-file={}", unlock.path().display()))
			.arg("--tpm2-device=auto")
			.arg("--tpm2-pcrs=")
			.arg(partition);
		run(&mut command).map(drop)
	}

	fn remove(&self, partition: &Path, slot: u32, key: &Key<'_>) -> Result<(), String> {
		let key = Arguments::of(key)?;
		run(Command::new("cryptsetup")
			.args(["luksKillSlot", "--batch-mode"])
			.args(&key.args)
			.arg(partition)
			.arg(slot.to_string()))
		.map(drop)
	}

	fn open(&self, partition: &Path, name: &str, key: &Key<'_>) -> Result<PathBuf, String> {
		let key = Arguments::of(key)?;
		run(Command::new("cryptsetup")
			.arg("open")
			.args(&key.args)
			.arg(partition)
			.arg(name))?;
		Ok(Path::new("/dev/mapper").join(name))
	}

	fn close(&self, name: &str) {
		if let Err(err) = run(Command::new("cryptsetup").arg("close").arg(name)) {
			tracing::warn!(%err, name, "cannot close a volume");
		}
	}
}

/// The header of a volume with the UUID `uuid`, from `cryptsetup luksDump --dump-json-metadata`.
fn parse(uuid: &str, json: &str) -> Result<Header, String> {
	let bad = |what: &str| format!("the volume's header has no {what}");
	let json: Json = serde_json::from_str(json)
		.map_err(|err| format!("cannot read the volume's header: {err}"))?;

	let digest = json["digests"]
		.get("0")
		.and_then(|digest| digest["digest"].as_str())
		.ok_or_else(|| bad("volume key digest"))?
		.to_owned();

	let tpm_slots: Vec<(u32, Vec<u32>)> = json["tokens"]
		.as_object()
		.into_iter()
		.flatten()
		.filter(|(_, token)| token["type"] == TPM_TOKEN)
		.flat_map(|(_, token)| {
			let pcrs: Vec<u32> = token["tpm2-pcrs"]
				.as_array()
				.into_iter()
				.flatten()
				.filter_map(|pcr| pcr.as_u64().and_then(|pcr| u32::try_from(pcr).ok()))
				.collect();
			token["keyslots"]
				.as_array()
				.into_iter()
				.flatten()
				.filter_map(Json::as_str)
				.filter_map(|id| id.parse().ok())
				.map(move |id| (id, pcrs.clone()))
				.collect::<Vec<_>>()
		})
		.collect();

	let mut slots: Vec<Slot> = json["keyslots"]
		.as_object()
		.ok_or_else(|| bad("keyslots"))?
		.keys()
		.filter_map(|id| id.parse().ok())
		.map(|id| Slot {
			id,
			tpm: tpm_slots
				.iter()
				.find(|(slot, _)| *slot == id)
				.map(|(_, pcrs)| pcrs.clone()),
		})
		.collect();
	slots.sort_by_key(|slot| slot.id);

	let reencrypting = json["config"]["requirements"]
		.as_array()
		.into_iter()
		.flatten()
		.any(|requirement| requirement == "online-reencrypt");

	Ok(Header {
		uuid: uuid.to_owned(),
		digest,
		slots,
		reencrypting,
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	const JSON: &str = r#"{
		"keyslots": {
			"0": {"type": "luks2", "key_size": 64},
			"2": {"type": "luks2", "key_size": 64},
			"1": {"type": "luks2", "key_size": 64}
		},
		"tokens": {
			"0": {"type": "systemd-tpm2", "keyslots": ["2"], "tpm2-pcrs": []},
			"1": {"type": "something-else", "keyslots": ["1"]}
		},
		"digests": {"0": {"type": "pbkdf2", "keyslots": ["0", "1", "2"], "digest": "AbC="}},
		"config": {"requirements": []}
	}"#;

	#[test]
	fn reads_the_digest_the_slots_and_which_is_the_tpms() {
		let header = parse("uuid", JSON).unwrap();
		assert_eq!(header.uuid, "uuid");
		assert_eq!(header.digest, "AbC=");
		assert!(!header.reencrypting);
		assert_eq!(
			header.slots,
			[
				Slot { id: 0, tpm: None },
				Slot { id: 1, tpm: None },
				Slot {
					id: 2,
					tpm: Some(vec![])
				},
			]
		);
	}

	#[test]
	fn a_pcr_policy_is_read() {
		let json = JSON.replace(r#""tpm2-pcrs": []"#, r#""tpm2-pcrs": [1, 7]"#);
		let header = parse("uuid", &json).unwrap();
		assert_eq!(header.slots[2].tpm, Some(vec![1, 7]));
	}

	#[test]
	fn a_re_encryption_under_way_is_read() {
		let json = JSON.replace(
			r#""requirements": []"#,
			r#""requirements": ["online-reencrypt"]"#,
		);
		assert!(parse("uuid", &json).unwrap().reencrypting);
	}

	#[test]
	fn a_header_without_a_digest_is_an_error() {
		let json = r#"{"keyslots": {}, "tokens": {}, "digests": {}, "config": {}}"#;
		assert!(parse("uuid", json).is_err());
		assert!(parse("uuid", "not json").is_err());
	}
}
