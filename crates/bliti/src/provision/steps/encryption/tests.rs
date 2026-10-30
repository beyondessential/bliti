use std::fs;

use bliti_core::provisioning::report::{StepReport, StepState};

use super::*;
use crate::{
	facts::i2c,
	provision::{
		fake_system::{Bench, Unlock, Volume},
		steps::testing::Rig,
	},
};

/// The board, which the step asks for its TPM, its suffix and its version.
struct Board {
	suffix: Suffix,
	tpm: bool,
	air: Result<u8, ()>,
}

impl Board {
	fn working() -> Self {
		Self {
			suffix: Suffix::Board("K7QA".to_owned()),
			tpm: true,
			air: Ok(0x61),
		}
	}
}

#[async_trait]
impl Identity for Board {
	fn tpm(&self) -> Result<(), String> {
		self.tpm
			.then_some(())
			.ok_or_else(|| "the TPM does not answer".to_owned())
	}

	async fn suffix(&self) -> Suffix {
		self.suffix.clone()
	}
}

impl Air for Board {
	fn chip_id(&self) -> Result<u8, i2c::Error> {
		self.air.map_err(|()| i2c::Error::NoAnswer)
	}
}

/// A bench with a system on its disk, as the image ships it.
fn system() -> Bench {
	let bench = Bench::new();
	bench.shipped();
	bench
}

fn step_for(bench: &Bench, board: Board) -> Encryption {
	let board = Arc::new(board);
	Encryption::with_images(
		bench.storage(),
		bench.luks(),
		bench.initramfs(),
		board.clone(),
		board,
		&bench.medium,
		&bench.images,
	)
}

fn step(bench: &Bench) -> Encryption {
	step_for(bench, Board::working())
}

async fn ran(bench: &Bench) -> StepReport {
	let rig = Rig::new(Arc::new(step(bench))).await;
	rig.start();
	rig.ended().await
}

/// The last line of the records file, as its fields.
fn last_record(bench: &Bench) -> Vec<String> {
	bench
		.records()
		.lines()
		.last()
		.expect("a record")
		.split('\t')
		.map(str::to_owned)
		.collect()
}

fn crypttab(bench: &Bench) -> String {
	fs::read_to_string(bench.root.join("etc/crypttab")).unwrap()
}

/// What the step leaves, which every way of carrying it out gives (PENC, "What the step leaves").
fn assert_set_up(bench: &Bench) {
	let volume = bench.volume();
	let image = Volume::shipped();
	assert_ne!(volume.digest, image.digest, "the volume key is the image's");
	assert_ne!(volume.uuid, image.uuid, "the UUID is the image's");
	assert!(!volume.reencrypting);
	assert_eq!(volume.slots.len(), 2, "{:?}", volume.slots);
	assert!(
		volume
			.slots
			.iter()
			.any(|(_, unlock)| matches!(unlock, Unlock::Passphrase(_)))
	);
	assert!(
		volume
			.slots
			.iter()
			.any(|(_, unlock)| matches!(unlock, Unlock::Tpm { pcrs, .. } if pcrs.is_empty()))
	);
	let record = last_record(bench);
	assert!(
		volume.unlocked_by_key(&Key::Passphrase(&record[3])),
		"the recorded passphrase does not unlock"
	);
	assert!(volume.unlocked_by_key(&Key::Tpm));
	assert!(!volume.unlocked_by_key(&Key::Empty));
	assert_eq!(record[2], volume.uuid);
	assert!(crypttab(bench).contains("tpm2-device=auto"));
}

// spec: PENC#what-the-step-leaves
#[tokio::test]
async fn a_disk_as_the_image_ships_it_is_set_up() {
	let bench = system();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed, "{:?}", report.reason);
	assert_set_up(&bench);
	let initrd = fs::read_to_string(bench.firmware.join("current/initrd.img")).unwrap();
	assert!(initrd.ends_with("unlocks from the TPM: true"), "{initrd}");
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
}

// spec: PENC#the-record
#[tokio::test]
async fn the_record_is_saved_and_reported() {
	let bench = system();
	let report = ran(&bench).await;
	let line = last_record(&bench);
	assert_eq!(line.len(), 5);
	assert_eq!(line[0], "K7QA");
	assert_eq!(line[1], "4.0");
	assert_eq!(
		line[4]
			.parse::<jiff::Timestamp>()
			.unwrap()
			.to_string()
			.len(),
		line[4].len()
	);
	assert!(line[4].ends_with('Z'));
	assert_eq!(bench.records().lines().count(), 1);
	assert!(bench.records().ends_with('\n'));

	let result = report.result.expect("a result");
	let record = result["record"].as_object().unwrap();
	assert_eq!(record["suffix"], line[0].as_str());
	assert_eq!(record["version"], line[1].as_str());
	assert_eq!(record["disk"], line[2].as_str());
	assert_eq!(record["passphrase"], line[3].as_str());
	assert_eq!(record["at"], line[4].as_str());
}

// spec: PENC#the-record
#[tokio::test]
async fn a_board_that_cannot_be_placed_has_a_record_with_no_version() {
	let bench = system();
	let board = Board {
		air: Err(()),
		..Board::working()
	};
	let rig = Rig::new(Arc::new(step_for(&bench, board))).await;
	rig.start();
	let report = rig.ended().await;
	assert_eq!(report.state, StepState::Passed);
	assert_eq!(last_record(&bench)[1], "");
	assert!(
		!report.result.unwrap()["record"]
			.as_object()
			.unwrap()
			.contains_key("version")
	);
}

// spec: PENC#the-record
#[tokio::test]
async fn records_are_appended_to_the_ones_already_there() {
	let bench = system();
	fs::write(
		bench.medium.join("records.tsv"),
		"ZZZZ\t4.0\tuuid\tpass\tthen\n",
	)
	.unwrap();
	ran(&bench).await;
	let records = bench.records();
	assert_eq!(records.lines().count(), 2);
	assert!(records.starts_with("ZZZZ\t4.0\tuuid\tpass\tthen\n"));
}

// spec: PENC#what-the-step-leaves
#[tokio::test]
async fn a_volume_whose_key_and_uuid_are_not_the_images_keeps_them() {
	let bench = system();
	{
		let mut state = bench.state.lock().unwrap();
		let volume = state.volume.as_mut().unwrap();
		volume.digest = "its-own-key".to_owned();
		volume.uuid = "its-own-uuid".to_owned();
	}
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	let volume = bench.volume();
	assert_eq!(volume.digest, "its-own-key");
	assert_eq!(volume.uuid, "its-own-uuid");
	assert_eq!(last_record(&bench)[2], "its-own-uuid");
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn keyslots_an_earlier_try_left_are_removed() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots = vec![
		(0, Unlock::Empty),
		(
			1,
			Unlock::Passphrase("an-old-recovery-passphrase".to_owned()),
		),
		(
			2,
			Unlock::Tpm {
				ours: true,
				pcrs: vec![1],
			},
		),
	];
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert_set_up(&bench);
	assert!(
		!bench
			.volume()
			.unlocked_by_key(&Key::Passphrase("an-old-recovery-passphrase"))
	);
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_re_encryption_that_was_interrupted_is_resumed() {
	let bench = system();
	{
		let mut state = bench.state.lock().unwrap();
		state.volume.as_mut().unwrap().reencrypting = true;
	}
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert_set_up(&bench);
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn every_interruption_leaves_a_disk_that_opens_and_the_next_try_finishes() {
	for budget in 0..10 {
		let bench = system();
		bench.state.lock().unwrap().budget = Some(budget);
		let first = ran(&bench).await;

		let volume = bench.volume();
		let recorded = bench
			.records()
			.lines()
			.last()
			.map(|line| line.split('\t').nth(3).unwrap().to_owned());
		assert!(
			volume.opens(recorded.as_deref()),
			"budget {budget}: nothing opens the volume: {:?} (record {recorded:?})",
			volume.slots
		);
		if !volume.unlocked_by_key(&Key::Empty) && first.state == StepState::Failed {
			assert!(
				recorded.is_some(),
				"budget {budget}: the empty passphrase went before the record was saved"
			);
		}

		bench.state.lock().unwrap().budget = None;
		let second = ran(&bench).await;
		assert_eq!(
			second.state,
			StepState::Passed,
			"budget {budget}: {:?}",
			second.reason
		);
		if first.state == StepState::Failed {
			assert_set_up(&bench);
		}
	}
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn an_interruption_after_the_record_is_saved_leaves_a_second_line_and_the_last_is_current() {
	let bench = system();
	// Past the re-encryption, the UUID, the two keyslots and the initramfs, at the last removal.
	bench.state.lock().unwrap().budget = Some(5);
	assert_eq!(ran(&bench).await.state, StepState::Failed);
	assert_eq!(bench.records().lines().count(), 1);
	let first = last_record(&bench)[3].clone();
	assert!(bench.volume().unlocked_by_key(&Key::Empty));

	bench.state.lock().unwrap().budget = None;
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert_eq!(bench.records().lines().count(), 2);
	assert_ne!(last_record(&bench)[3], first);
	assert!(!bench.volume().unlocked_by_key(&Key::Passphrase(&first)));
	assert_set_up(&bench);
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_record_that_cannot_be_saved_fails_the_step_and_leaves_the_empty_passphrase() {
	let bench = system();
	fs::remove_dir_all(&bench.medium).unwrap();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("record"));
	assert!(bench.volume().unlocked_by_key(&Key::Empty));
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_system_that_cannot_unlock_from_the_tpm_fails_the_step_before_anything_changes() {
	let bench = system();
	bench.state.lock().unwrap().can_unlock_from_tpm = false;
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("unlocks from the TPM"));
	let volume = bench.volume();
	assert_eq!(volume.digest, Volume::shipped().digest);
	assert_eq!(volume.uuid, Volume::shipped().uuid);
	assert_eq!(volume.slots.len(), 1);
	assert_eq!(bench.records(), "");
	assert!(crypttab(&bench).contains("empty-keyfile"));
	let state = bench.state.lock().unwrap();
	assert_eq!((state.mounts, state.mappings), (0, 0));
}

// spec: PENC#what-the-step-leaves
#[tokio::test]
async fn a_tpm_keyslot_bound_to_pcrs_fails_the_step_and_leaves_the_empty_passphrase() {
	let bench = system();
	bench.state.lock().unwrap().enrol_pcrs = vec![7];
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("PCR"));
	assert!(bench.volume().unlocked_by_key(&Key::Empty));
	assert_eq!(bench.records(), "");
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn the_tpm_not_answering_or_no_suffix_fails_the_step() {
	let bench = system();
	let board = Board {
		tpm: false,
		..Board::working()
	};
	let rig = Rig::new(Arc::new(step_for(&bench, board))).await;
	rig.start();
	assert!(rig.ended().await.reason.unwrap().contains("TPM"));

	let board = Board {
		suffix: Suffix::Fault,
		..Board::working()
	};
	let rig = Rig::new(Arc::new(step_for(&bench, board))).await;
	rig.start();
	assert!(rig.ended().await.reason.unwrap().contains("suffix"));
	assert_eq!(bench.volume().slots.len(), 1);
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_disk_with_no_encrypted_root_volume_fails_the_step() {
	let bench = Bench::new();
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("no encrypted root volume"));
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_volume_neither_the_empty_passphrase_nor_the_tpm_opens_fails_the_step() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots =
		vec![(1, Unlock::Passphrase("someone-elses".to_owned()))];
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Failed);
	assert!(report.reason.unwrap().contains("could not be opened"));
}

// spec: PENC#carrying-out-the-step
#[tokio::test]
async fn a_volume_already_set_up_passes_without_changing_anything() {
	let bench = system();
	assert_eq!(ran(&bench).await.state, StepState::Passed);
	let (records, volume, initrd) = (
		bench.records(),
		format!("{:?}", bench.volume()),
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
	);
	let report = ran(&bench).await;
	assert_eq!(report.state, StepState::Passed);
	assert!(report.result.is_none());
	assert_eq!(bench.records(), records);
	assert_eq!(format!("{:?}", bench.volume()), volume);
	assert_eq!(
		fs::read(bench.firmware.join("current/initrd.img")).unwrap(),
		initrd
	);
}

// spec: PENC#done-state
#[tokio::test]
async fn the_done_state_follows_the_disk() {
	let bench = system();
	let step = step(&bench);
	let before = step.done().await;
	assert_eq!(before.state, DoneState::NotDone);
	assert!(before.why.unwrap().contains("empty passphrase"));

	assert_eq!(ran(&bench).await.state, StepState::Passed);
	assert_eq!(step.done().await.state, DoneState::Done);
}

// spec: PENC#done-state
#[tokio::test]
async fn a_disk_with_no_encrypted_root_volume_is_not_done() {
	let bench = Bench::new();
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("no encrypted root volume"));
}

// spec: PENC#done-state
#[tokio::test]
async fn a_volume_another_boards_tpm_unlocks_is_not_done() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots = vec![(
		1,
		Unlock::Tpm {
			ours: false,
			pcrs: Vec::new(),
		},
	)];
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("TPM"));
}

// spec: PENC#done-state
#[tokio::test]
async fn a_volume_that_still_has_the_images_key_is_not_done() {
	let bench = system();
	bench.state.lock().unwrap().volume.as_mut().unwrap().slots = vec![(
		1,
		Unlock::Tpm {
			ours: true,
			pcrs: Vec::new(),
		},
	)];
	let done = step(&bench).done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("key of the image"));
}

// spec: PENC#done-state
#[tokio::test]
async fn the_tpm_not_answering_or_no_disk_makes_the_done_state_unknown() {
	let bench = system();
	let board = Board {
		tpm: false,
		..Board::working()
	};
	assert_eq!(
		step_for(&bench, board).done().await.state,
		DoneState::Unknown
	);

	bench.state.lock().unwrap().disks.clear();
	assert_eq!(step(&bench).done().await.state, DoneState::Unknown);
}

// spec: PENC#done-state
#[tokio::test]
async fn working_out_the_done_state_changes_nothing() {
	let bench = system();
	let before = format!("{:?}", bench.volume());
	step(&bench).done().await;
	assert_eq!(format!("{:?}", bench.volume()), before);
	assert_eq!(bench.records(), "");
	assert!(crypttab(&bench).contains("empty-keyfile"));
}

#[test]
fn a_random_uuid_is_a_lowercase_version_4_uuid() {
	let uuid = random_uuid();
	assert_eq!(uuid.len(), 36);
	assert_eq!(uuid.matches('-').count(), 4);
	assert_eq!(uuid.as_bytes()[14], b'4');
	assert!(matches!(uuid.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
	assert_eq!(uuid, uuid.to_ascii_lowercase());
	assert_ne!(random_uuid(), random_uuid());
}
