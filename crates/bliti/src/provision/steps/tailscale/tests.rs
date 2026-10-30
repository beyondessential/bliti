use std::{
	path::{Path, PathBuf},
	sync::Mutex,
};

use bliti_core::provisioning::report::{Answer, RunState, StepState};

use super::*;
use crate::provision::{
	Act, Provisioner, Snapshot,
	fake::{BUILT, Do, Fake, Gate, Log},
	fake_system::{Bench, Unlock, Volume},
	store::Store,
};

const KEY: &str = "tskey-auth-kS3cret";

/// A disk whose root volume opens through the board's TPM, and whose root filesystem is a
/// directory.
struct Disk {
	bench: Bench,
}

impl Disk {
	fn new() -> Arc<Self> {
		let bench = Bench::new();
		bench.state.lock().unwrap().volume = Some(Volume {
			uuid: "disk-uuid".to_owned(),
			digest: "disk-key".to_owned(),
			slots: vec![(
				0,
				Unlock::Tpm {
					ours: true,
					pcrs: Vec::new(),
				},
			)],
			reencrypting: false,
		});
		Arc::new(Self { bench })
	}

	fn files(&self) -> &Path {
		&self.bench.root
	}

	fn write(&self, path: &str, contents: &[u8]) {
		let path = self.files().join(path);
		fs::create_dir_all(path.parent().unwrap()).unwrap();
		fs::write(path, contents).unwrap();
	}

	/// The board's TPM no longer answers, so the root volume cannot be opened.
	fn lock(&self) {
		self.bench.state.lock().unwrap().tpm_answers = false;
	}

	/// The board has no drive.
	fn remove(&self) {
		self.bench.state.lock().unwrap().disks.clear();
	}

	/// Whether every mapping and mount that was made has been undone.
	fn released(&self) -> bool {
		let state = self.bench.state.lock().unwrap();
		state.mappings == 0 && state.mounts == 0
	}
}

/// What was asked of the Tailscale that was signed in.
#[derive(Debug, Clone, PartialEq)]
struct Joined {
	state: PathBuf,
	name: String,
	key: String,
	tailnet: Option<String>,
}

struct Tailnet {
	joined: Mutex<Vec<Joined>>,
	outcome: Mutex<Result<bool, String>>,
}

impl Tailnet {
	fn new() -> Arc<Self> {
		Arc::new(Self {
			joined: Mutex::new(Vec::new()),
			outcome: Mutex::new(Ok(false)),
		})
	}

	fn will(&self, outcome: Result<bool, String>) {
		*self.outcome.lock().unwrap() = outcome;
	}

	fn joined(&self) -> Vec<Joined> {
		self.joined.lock().unwrap().clone()
	}
}

#[async_trait]
impl Node for Tailnet {
	async fn sign_in(
		&self,
		state: &std::path::Path,
		name: &str,
		key: Secret,
		tailnet: Option<String>,
	) -> Result<bool, String> {
		self.joined.lock().unwrap().push(Joined {
			state: state.to_owned(),
			name: name.to_owned(),
			key: key.expose().to_owned(),
			tailnet,
		});
		self.outcome.lock().unwrap().clone()
	}
}

fn named(suffix: Suffix) -> Name {
	Name { marker: 1, suffix }
}

fn known(suffix: Option<Suffix>) -> Known {
	let (sender, receiver) = watch::channel(suffix.map(named));
	// A sender that is gone leaves the last value, which is all these tests need.
	drop(sender);
	receiver
}

fn board() -> Known {
	known(Some(Suffix::Board("K7QA".to_owned())))
}

fn step(disk: &Arc<Disk>, node: &Arc<Tailnet>, suffix: Known) -> Arc<Tailscale> {
	Arc::new(Tailscale::with(
		disk.bench.storage(),
		disk.bench.luks(),
		node.clone(),
		suffix,
	))
}

/// A device holding an `eeprom` that passes at once, optionally a step that waits for `gate`, and
/// then the step.
async fn device(
	tailscale: Arc<Tailscale>,
	gate: Option<&Arc<Gate>>,
) -> (Provisioner, tempfile::TempDir) {
	let log = Log::default();
	let mut steps: Vec<Arc<dyn Step>> = vec![Fake::new("eeprom", &log, vec![])];
	if let Some(gate) = gate {
		steps.push(Fake::new("hold", &log, vec![Do::Hold(gate.clone())]));
	}
	steps.push(tailscale);
	let medium = tempfile::tempdir().unwrap();
	let store = Store::new(medium.path(), &[0xaa, 0xbb]);
	(Provisioner::start(steps, Some(store), BUILT).await, medium)
}

fn start(provisioner: &Provisioner, held: bool) {
	let mut steps = vec!["eeprom"];
	if held {
		steps.push("hold");
	}
	steps.push("tailscale");
	provisioner
		.act(&Act {
			act: "start".to_owned(),
			steps: Some(steps.into_iter().map(str::to_owned).collect()),
			prompt: None,
			value: None,
		})
		.unwrap();
}

fn answer(
	provisioner: &Provisioner,
	prompt: &str,
	text: &str,
) -> Result<(), crate::provision::CannotAct> {
	provisioner.act(&Act {
		act: "answer".to_owned(),
		steps: None,
		prompt: Some(prompt.to_owned()),
		value: Some(Answer::text(text)),
	})
}

async fn until(provisioner: &Provisioner, until: impl Fn(&Snapshot) -> bool) -> Snapshot {
	let mut changes = provisioner.watch();
	tokio::time::timeout(
		std::time::Duration::from_secs(10),
		changes.wait_for(|snapshot| until(snapshot)),
	)
	.await
	.expect("the state never came to what was waited for")
	.expect("the provisioner outlives its watchers")
	.clone()
}

fn prompt<'a>(
	snapshot: &'a Snapshot,
	id: &str,
) -> &'a bliti_core::provisioning::report::PromptReport {
	snapshot
		.prompts
		.iter()
		.find(|prompt| prompt.id == id)
		.unwrap_or_else(|| panic!("no prompt {id}: {:?}", snapshot.prompts))
}

fn tailscale_step(snapshot: &Snapshot) -> &bliti_core::provisioning::report::StepReport {
	snapshot
		.steps
		.iter()
		.find(|step| step.id == "tailscale")
		.unwrap()
}

#[tokio::test]
async fn the_key_and_the_name_are_asked_for_once_the_eeprom_has_passed_without_holding_the_run_up()
{
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (device, _medium) = device(step(&disk, &node, board()), Some(&gate)).await;
	start(&device, true);
	let asked = until(&device, |s| s.prompts.len() == 2).await;

	let key = prompt(&asked, "auth-key");
	assert_eq!(key.step, "tailscale");
	assert_eq!(
		key.kind,
		bliti_core::provisioning::report::PromptKind::Secret
	);
	assert!(!key.blocking);
	assert!(key.text.contains("reusable"));
	assert_eq!(key.default, None);

	let name = prompt(&asked, "name");
	assert_eq!(
		name.kind,
		bliti_core::provisioning::report::PromptKind::Text
	);
	assert!(!name.blocking);
	assert_eq!(name.default.as_deref(), Some("tamanu-iti-k7qa"));

	assert_eq!(asked.run, RunState::Running);
	assert_eq!(tailscale_step(&asked).state, StepState::Pending);
}

#[tokio::test]
async fn a_run_that_was_answered_early_signs_in_with_the_answers() {
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (device, _medium) = device(step(&disk, &node, board()), Some(&gate)).await;
	start(&device, true);
	until(&device, |s| s.prompts.len() == 2).await;
	answer(&device, "auth-key", KEY).unwrap();
	answer(&device, "name", "iti-front-desk").unwrap();
	gate.open();
	let done = until(&device, |s| s.run == RunState::Finished).await;

	let joined = node.joined();
	assert_eq!(joined.len(), 1);
	assert_eq!(joined[0].name, "iti-front-desk");
	assert_eq!(joined[0].key, KEY);
	assert_eq!(joined[0].tailnet, None);
	assert!(joined[0].state.starts_with(disk.files()));
	assert!(joined[0].state.ends_with("var/lib/tailscale"));
	assert_eq!(
		tailscale_step(&done).result.as_ref().unwrap()["needs-approval"],
		Json::Bool(false)
	);
	assert!(disk.released());
}

#[tokio::test]
async fn an_unanswered_name_takes_the_default_and_its_prompt_closes() {
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (device, _medium) = device(step(&disk, &node, board()), Some(&gate)).await;
	start(&device, true);
	until(&device, |s| s.prompts.len() == 2).await;
	answer(&device, "auth-key", KEY).unwrap();
	gate.open();
	let done = until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(node.joined()[0].name, "tamanu-iti-k7qa");
	assert!(done.prompts.is_empty());
}

#[tokio::test]
async fn a_key_not_given_by_the_steps_turn_holds_the_step_up_for_it() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	let waiting = until(&device, |s| s.run == RunState::Waiting).await;
	assert_eq!(tailscale_step(&waiting).state, StepState::Waiting);
	assert!(prompt(&waiting, "auth-key").blocking);
	assert!(node.joined().is_empty());

	answer(&device, "auth-key", KEY).unwrap();
	until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(node.joined()[0].key, KEY);
}

#[tokio::test]
async fn a_board_whose_id_could_not_be_established_offers_no_default_and_waits_for_a_name() {
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (device, _medium) =
		device(step(&disk, &node, known(Some(Suffix::Fault))), Some(&gate)).await;
	start(&device, true);
	let asked = until(&device, |s| s.prompts.len() == 2).await;
	assert_eq!(prompt(&asked, "name").default, None);

	answer(&device, "auth-key", KEY).unwrap();
	gate.open();
	let waiting = until(&device, |s| s.run == RunState::Waiting).await;
	assert!(prompt(&waiting, "name").blocking);
	answer(&device, "name", "iti-9").unwrap();
	until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(node.joined()[0].name, "iti-9");
}

#[tokio::test]
async fn the_step_waits_for_the_boards_id_before_offering_a_name() {
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (sender, receiver) = watch::channel(None);
	let (device, _medium) = device(step(&disk, &node, receiver), Some(&gate)).await;
	start(&device, true);
	// The name is not offered while the board's ID is still being derived.
	until(&device, |s| s.prompts.len() == 1).await;
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;
	assert_eq!(device.watch().borrow().prompts.len(), 1);

	sender.send_replace(Some(named(Suffix::Board("AB23".to_owned()))));
	let asked = until(&device, |s| s.prompts.len() == 2).await;
	assert_eq!(
		prompt(&asked, "name").default.as_deref(),
		Some("tamanu-iti-ab23")
	);
}

#[tokio::test]
async fn a_name_that_is_no_hostname_is_refused_at_the_value() {
	let (disk, node, gate) = (Disk::new(), Tailnet::new(), Gate::new());
	let (device, _medium) = device(step(&disk, &node, board()), Some(&gate)).await;
	start(&device, true);
	until(&device, |s| s.prompts.len() == 2).await;
	for bad in ["", "Iti", "-iti", "iti-", "iti one", &"a".repeat(64)] {
		let refused = answer(&device, "name", bad).unwrap_err();
		assert_eq!(refused.at, "$['value']", "{bad:?}");
	}
	answer(&device, "name", "iti-1").unwrap();
}

#[tokio::test]
async fn the_system_s_tailnet_restriction_is_carried_to_the_sign_in() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	disk.write(
		"etc/bliti/daemon.env",
		b"BLITI_DAEMON_ARGS=\"--tailnet fleet.example\"\n",
	);
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(node.joined()[0].tailnet.as_deref(), Some("fleet.example"));
}

#[tokio::test]
async fn daemon_options_that_cannot_be_read_fail_the_step_without_signing_in() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	disk.write(
		"etc/bliti/daemon.env",
		b"BLITI_DAEMON_ARGS=\"--nonsense\"\n",
	);
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	let failed = until(&device, |s| s.run == RunState::Failed).await;
	assert!(
		tailscale_step(&failed)
			.reason
			.as_ref()
			.unwrap()
			.contains("daemon options")
	);
	assert!(node.joined().is_empty());
	assert!(disk.released());
}

#[tokio::test]
async fn a_sign_in_that_waits_on_approval_passes_and_says_so() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	node.will(Ok(true));
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	let done = until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(
		tailscale_step(&done).result.as_ref().unwrap()["needs-approval"],
		Json::Bool(true)
	);
}

#[tokio::test]
async fn a_failed_sign_in_discards_the_key_and_asks_again_on_a_retry() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	node.will(Err("Tailscale did not sign in in time".to_owned()));
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	let failed = until(&device, |s| s.run == RunState::Failed).await;
	let reason = tailscale_step(&failed).reason.clone().unwrap();
	assert!(reason.contains("did not sign in"), "{reason}");
	assert!(!reason.contains(KEY));
	// Unmounted though it failed.
	assert!(disk.released());

	node.will(Ok(false));
	device
		.act(&Act {
			act: "retry".to_owned(),
			steps: None,
			prompt: None,
			value: None,
		})
		.unwrap();
	let again = until(&device, |s| s.run == RunState::Waiting).await;
	assert!(prompt(&again, "auth-key").blocking);
	answer(&device, "auth-key", "tskey-auth-another").unwrap();
	until(&device, |s| s.run == RunState::Finished).await;
	assert_eq!(node.joined().last().unwrap().key, "tskey-auth-another");
}

#[tokio::test]
async fn a_disk_whose_root_volume_cannot_be_opened_fails_the_step_with_the_reason() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	disk.lock();
	let (device, _medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	let failed = until(&device, |s| s.run == RunState::Failed).await;
	assert!(
		tailscale_step(&failed)
			.reason
			.as_ref()
			.unwrap()
			.contains("could not be opened")
	);
	assert!(node.joined().is_empty());
}

#[tokio::test]
async fn the_key_never_reaches_the_disk_or_the_medium() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	let (device, medium) = device(step(&disk, &node, board()), None).await;
	start(&device, false);
	until(&device, |s| s.run == RunState::Waiting).await;
	answer(&device, "auth-key", KEY).unwrap();
	until(&device, |s| s.run == RunState::Finished).await;

	fn holds(dir: &std::path::Path, text: &str) -> bool {
		fs::read_dir(dir)
			.into_iter()
			.flatten()
			.flatten()
			.any(|entry| {
				let path = entry.path();
				if path.is_dir() {
					holds(&path, text)
				} else {
					fs::read(&path)
						.is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(text))
				}
			})
	}
	assert!(!holds(disk.files(), KEY));
	assert!(!holds(medium.path(), KEY));
}

#[tokio::test]
async fn the_done_state_is_read_from_the_state_the_disk_keeps_without_writing() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	let step = step(&disk, &node, board());

	// A disk with no Tailscale state has not been signed in.
	let blank = step.done().await;
	assert_eq!(blank.state, DoneState::NotDone);

	let prefs = r#"{"LoggedOut":false,"Config":{"NodeID":"nABC"}}"#;
	disk.write(
		state::PATH,
		&state::file(Some("profile-1"), &[("profile-1", prefs)]),
	);
	let signed_in = step.done().await;
	assert_eq!(signed_in.state, DoneState::Done);

	disk.write(
		state::PATH,
		&state::file(Some("profile-1"), &[("profile-1", r#"{"LoggedOut":true}"#)]),
	);
	assert_eq!(step.done().await.state, DoneState::NotDone);

	// Always closed again.
	assert!(disk.released());
	assert!(node.joined().is_empty());
}

#[tokio::test]
async fn a_disk_that_cannot_be_opened_is_not_known_to_be_signed_in() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	disk.remove();
	let done = step(&disk, &node, board()).done().await;
	assert_eq!(done.state, DoneState::Unknown);
	assert_eq!(done.why.as_deref(), Some("the board has no NVMe drive"));
}

#[tokio::test]
async fn unreadable_state_is_not_known_to_be_signed_in() {
	let (disk, node) = (Disk::new(), Tailnet::new());
	disk.write(state::PATH, b"{");
	let done = step(&disk, &node, board()).done().await;
	assert_eq!(done.state, DoneState::Unknown);
}
