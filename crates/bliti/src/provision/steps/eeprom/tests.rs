use std::sync::Mutex;

use bliti_core::provisioning::report::{PromptKind, StepState};

use super::{
	chip::tests::Chip,
	hat::tests::{image, iti_4_0},
	*,
};
use crate::provision::steps::testing::{Rig, open};

/// An EEPROM that refuses writes until it is "bridged", as the real one does while write-protected.
struct Fake {
	chip: Chip,
	/// How many writes are refused before the pads are bridged, and so the chip is let write.
	refusals: Mutex<u32>,
	/// Whether a write that is let through reports success without changing anything.
	forgetful: Mutex<u32>,
	writes: Mutex<u32>,
}

impl Fake {
	fn holding(bytes: &[u8]) -> Arc<Self> {
		Arc::new(Self {
			chip: Chip::holding(bytes),
			refusals: Mutex::new(0),
			forgetful: Mutex::new(0),
			writes: Mutex::new(0),
		})
	}

	fn refusing(self: Arc<Self>, writes: u32) -> Arc<Self> {
		*self.refusals.lock().unwrap() = writes;
		self
	}

	fn forgetting(self: Arc<Self>, writes: u32) -> Arc<Self> {
		*self.forgetful.lock().unwrap() = writes;
		self
	}

	fn contents(&self, len: usize) -> Vec<u8> {
		self.chip.state().memory[..len].to_vec()
	}

	fn writes(&self) -> u32 {
		*self.writes.lock().unwrap()
	}
}

impl Memory for Fake {
	fn read(&self, len: usize) -> Result<Vec<u8>, String> {
		chip::read(&mut self.chip.clone(), len).map_err(|err| err.to_string())
	}

	fn write(&self, image: &[u8]) -> Result<(), String> {
		*self.writes.lock().unwrap() += 1;
		{
			let mut refusals = self.refusals.lock().unwrap();
			if *refusals > 0 {
				*refusals -= 1;
				return Err("Remote I/O error".to_owned());
			}
		}
		{
			let mut forgetful = self.forgetful.lock().unwrap();
			if *forgetful > 0 {
				*forgetful -= 1;
				return Ok(());
			}
		}
		chip::write(&mut self.chip.clone(), image).map_err(|err| err.to_string())
	}
}

/// A bus on which the EEPROM cannot be reached at all.
struct Unreachable;

impl Memory for Unreachable {
	fn read(&self, _: usize) -> Result<Vec<u8>, String> {
		Err("nothing answered at the address".to_owned())
	}

	fn write(&self, _: &[u8]) -> Result<(), String> {
		Err("nothing answered at the address".to_owned())
	}
}

/// What the air sensor's address answers with.
struct Chip0(Result<u8, ()>);

impl Air for Chip0 {
	fn chip_id(&self) -> Result<u8, i2c::Error> {
		self.0.map_err(|()| i2c::Error::NoAnswer)
	}
}

const V4_0: Version = Version { major: 4, minor: 0 };

struct Rigged {
	step: Arc<Eeprom>,
	/// Keeps the images in place.
	_medium: tempfile::TempDir,
}

fn medium(images: &[(&str, &[u8])]) -> tempfile::TempDir {
	let dir = tempfile::tempdir().unwrap();
	for (name, bytes) in images {
		std::fs::write(dir.path().join(name), bytes).unwrap();
	}
	dir
}

fn step(air: Result<u8, ()>, memory: Arc<dyn Memory>, images: &[(&str, &[u8])]) -> Rigged {
	let medium = medium(images);
	Rigged {
		step: Arc::new(Eeprom::with_images(
			Arc::new(Chip0(air)),
			memory,
			medium.path(),
		)),
		_medium: medium,
	}
}

fn v4_0() -> (Vec<u8>, [(&'static str, Vec<u8>); 1]) {
	let image = iti_4_0();
	(image.clone(), [("tamanu-iti-v4.0.eep", image)])
}

fn standard(memory: Arc<dyn Memory>) -> (Rigged, Vec<u8>) {
	let (image, files) = v4_0();
	let files: Vec<_> = files.iter().map(|(n, b)| (*n, b.as_slice())).collect();
	(step(Ok(0x61), memory, &files), image)
}

fn ids(snapshot: &crate::provision::Snapshot) -> Vec<&str> {
	open(snapshot)
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn a_blank_eeprom_is_written_without_asking_once_the_pads_are_bridged() {
	let memory = Fake::holding(&[]).refusing(4);
	let (rigged, image) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let snapshot = rig.prompted("id-wp").await;
	assert_eq!(ids(&snapshot), ["id-wp"], "nothing is asked first");
	let prompt = &snapshot.prompts[0];
	assert_eq!(prompt.kind, PromptKind::Notice);
	for words in [
		"ID WP",
		"bare metal",
		"scissors",
		"Coated tools do not conduct",
		"until this step passes",
	] {
		assert!(
			prompt.text.contains(words),
			"{words} not in {}",
			prompt.text
		);
	}

	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Passed);
	assert_eq!(memory.contents(image.len()), image);
	assert_eq!(memory.writes(), 5, "refused four times, then written");
	let snapshot = rig.provisioner.watch().borrow().clone();
	assert!(
		snapshot.prompts.is_empty(),
		"the prompt closes as the step passes"
	);
}

/// A write that reports success and changes nothing does not count.
// spec: PEEP
#[tokio::test(start_paused = true)]
async fn a_write_that_reads_back_wrong_is_tried_again() {
	let memory = Fake::holding(&[]).forgetting(2);
	let (rigged, image) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Passed);
	assert_eq!(memory.writes(), 3);
	assert_eq!(memory.contents(image.len()), image);
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn unbridged_pads_fail_the_step_after_five_minutes_and_close_the_prompt() {
	let memory = Fake::holding(&[]).refusing(u32::MAX);
	let (rigged, _) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	rig.prompted("id-wp").await;
	let started = tokio::time::Instant::now();
	let step = rig.ended().await;

	assert_eq!(started.elapsed(), WAIT);
	assert_eq!(step.state, StepState::Failed);
	let reason = step.reason.unwrap();
	assert!(
		reason.contains("bare metal bridges both ID WP pads"),
		"{reason}"
	);
	let snapshot = rig.provisioner.watch().borrow().clone();
	assert!(snapshot.prompts.is_empty());
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn an_eeprom_already_holding_the_image_passes_without_writing() {
	let (image, _) = v4_0();
	let memory = Fake::holding(&image);
	let (rigged, _) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Passed);
	assert_eq!(memory.writes(), 0);
}

/// An earlier image of the same version is brought up to date without a question.
// spec: PEEP
#[tokio::test(start_paused = true)]
async fn the_same_version_with_other_contents_is_written_without_asking() {
	let earlier = image(
		0x02,
		crate::facts::product::ITI_UUID,
		4,
		0,
		b"BES",
		b"Old name",
		"tamanu-iti-v4",
	);
	let memory = Fake::holding(&earlier).refusing(2);
	let (rigged, image) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let snapshot = rig.prompted("id-wp").await;
	assert_eq!(ids(&snapshot), ["id-wp"]);
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(memory.contents(image.len()), image);
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn another_hat_is_written_over_only_once_the_operator_confirms() {
	let theirs = image(
		0x02,
		"00000000-0000-0000-0000-000000000001",
		1,
		0,
		b"Ac\x1bme",
		b"Widget",
		"x",
	);
	let memory = Fake::holding(&theirs).refusing(2);
	let (rigged, image) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let snapshot = rig.prompted("overwrite").await;
	let prompt = &snapshot.prompts[0];
	assert_eq!(prompt.kind, PromptKind::Confirm);
	assert!(prompt.text.contains("another HAT"), "{}", prompt.text);
	assert!(prompt.text.contains("Ac?me"), "{}", prompt.text);
	assert!(prompt.text.contains("Widget"), "{}", prompt.text);
	assert!(prompt.text.contains("Tamanu Iti v4.0"), "{}", prompt.text);
	assert_eq!(memory.writes(), 0, "nothing is written before the answer");

	rig.answer("overwrite", true);
	rig.prompted("id-wp").await;
	assert_eq!(rig.ended().await.state, StepState::Passed);
	assert_eq!(memory.contents(image.len()), image);
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn another_version_of_the_iti_asks_too() {
	let v3 = image(
		0x02,
		crate::facts::product::ITI_UUID,
		3,
		0,
		b"BES",
		b"Tamanu Iti v3.0",
		"x",
	);
	let memory = Fake::holding(&v3);
	let (rigged, _) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let snapshot = rig.prompted("overwrite").await;
	assert!(snapshot.prompts[0].text.contains("a Tamanu Iti v3.0"));
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn unrecognised_contents_ask_before_they_are_written_over() {
	let memory = Fake::holding(&[0x12; 300]);
	let (rigged, _) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let snapshot = rig.prompted("overwrite").await;
	assert!(snapshot.prompts[0].text.contains("not a HAT image"));
}

/// A declined overwrite leaves the chip as it was.
// spec: PEEP
#[tokio::test(start_paused = true)]
async fn a_declined_overwrite_fails_and_leaves_the_eeprom_untouched() {
	let theirs = image(
		0x02,
		"00000000-0000-0000-0000-000000000001",
		1,
		0,
		b"Acme",
		b"Widget",
		"x",
	);
	let memory = Fake::holding(&theirs);
	let (rigged, _) = standard(memory.clone());
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	rig.prompted("overwrite").await;
	rig.answer("overwrite", false);
	let step = rig.ended().await;

	assert_eq!(step.state, StepState::Failed);
	assert_eq!(step.reason.unwrap(), "the EEPROM was left as it was");
	assert_eq!(memory.writes(), 0);
	assert_eq!(memory.contents(theirs.len()), theirs);
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn a_board_that_cannot_be_placed_fails_naming_that() {
	let (rigged, _) = {
		let (image, _) = v4_0();
		(
			step(
				Err(()),
				Fake::holding(&[]),
				&[("tamanu-iti-v4.0.eep", &image)],
			),
			(),
		)
	};
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Failed);
	assert!(step.reason.unwrap().contains("could not be placed"));
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn a_medium_without_the_image_fails_naming_the_version() {
	let rigged = step(Ok(0x61), Fake::holding(&[]), &[]);
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Failed);
	assert!(step.reason.unwrap().contains("no image for v4.0"));
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn an_eeprom_that_cannot_be_read_fails() {
	let (rigged, _) = standard(Arc::new(Unreachable));
	let rig = Rig::new(rigged.step.clone()).await;
	rig.start();
	let step = rig.ended().await;
	assert_eq!(step.state, StepState::Failed);
	assert!(step.reason.unwrap().contains("cannot read the EEPROM"));
}

/// The version is placed afresh, not taken from the hardware check.
// spec: PEEP
#[tokio::test(start_paused = true)]
async fn the_version_is_placed_from_the_devices_each_time() {
	let (rigged, _) = standard(Fake::holding(&[]));
	assert_eq!(place(&rigged.step.air).await, Some(V4_0));
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn done_is_read_off_the_chip() {
	let (image, _) = v4_0();

	let (holding, _) = standard(Fake::holding(&image));
	let done = holding.step.done().await;
	assert_eq!(done.state, DoneState::Done);
	assert!(done.why.unwrap().contains("v4.0"));

	let (blank, _) = standard(Fake::holding(&[]));
	let done = blank.step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("blank"));

	let theirs = image_of_another();
	let (other, _) = standard(Fake::holding(&theirs));
	let done = other.step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("another HAT"));

	let (odd, _) = standard(Fake::holding(&[3; 50]));
	assert_eq!(odd.step.done().await.state, DoneState::NotDone);

	let earlier = image_of_version(4, 0);
	let (same, _) = standard(Fake::holding(&earlier));
	let done = same.step.done().await;
	assert_eq!(done.state, DoneState::NotDone);
	assert!(done.why.unwrap().contains("a Tamanu Iti v4.0"));
}

// spec: PEEP
#[tokio::test(start_paused = true)]
async fn done_is_unknown_where_it_cannot_be_told() {
	let (image, _) = v4_0();

	let unplaced = step(
		Err(()),
		Fake::holding(&image),
		&[("tamanu-iti-v4.0.eep", &image)],
	);
	let done = unplaced.step.done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("could not be placed"));

	let missing = step(Ok(0x61), Fake::holding(&image), &[]);
	let done = missing.step.done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("no image for v4.0"));

	let (unreadable, _) = standard(Arc::new(Unreachable));
	let done = unreadable.step.done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert!(done.why.unwrap().contains("cannot read the EEPROM"));
}

/// What the firmware publishes lags the chip until the next boot, so the step looks at the chip.
// spec: PEEP
#[tokio::test(start_paused = true)]
async fn done_follows_the_chip_as_it_changes() {
	let memory = Fake::holding(&[]);
	let (rigged, image) = standard(memory.clone());
	assert_eq!(rigged.step.done().await.state, DoneState::NotDone);
	chip::write(&mut memory.chip.clone(), &image).unwrap();
	assert_eq!(rigged.step.done().await.state, DoneState::Done);
}

#[tokio::test(start_paused = true)]
async fn the_step_is_named_for_the_operator() {
	let (rigged, _) = standard(Fake::holding(&[]));
	assert_eq!(rigged.step.id(), "eeprom");
	assert_eq!(rigged.step.title(), "Program the EEPROM");
}

fn image_of_another() -> Vec<u8> {
	image(
		0x02,
		"00000000-0000-0000-0000-000000000001",
		1,
		0,
		b"Acme",
		b"Widget",
		"x",
	)
}

fn image_of_version(major: u16, minor: u16) -> Vec<u8> {
	image(
		0x02,
		crate::facts::product::ITI_UUID,
		major,
		minor,
		b"BES",
		b"Old",
		"x",
	)
}
