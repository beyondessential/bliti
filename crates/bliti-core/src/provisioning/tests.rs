use super::{report::*, *};
use crate::{
	channel::stream::connect_initiator_unauthenticated, key_schedule::Root, qr::QrPayload,
};

#[test]
fn the_token_is_the_text_the_spec_gives() {
	assert_eq!(&TOKEN, b"ITI PROVISIONING");
	assert_eq!(token().as_bytes(), &TOKEN);
	assert_eq!(pre_shared_key(), token().pre_shared_key());
}

#[test]
fn a_name_is_the_prefix_the_marker_and_the_suffix() {
	let name = Name {
		marker: 1,
		suffix: Suffix::Board("K7QA".to_owned()),
	};
	assert_eq!(name.render(), "ITI_PROVISION_1_K7QA");
	assert_eq!(Name::filter(1), "ITI_PROVISION_1_");
	assert!(name.render().starts_with(&Name::filter(1)));
	assert_eq!(Name::parse(&name.render()), Some(name));
}

#[test]
fn a_device_that_cannot_name_its_board_says_fault() {
	let name = Name {
		marker: 1,
		suffix: Suffix::Fault,
	};
	assert_eq!(name.render(), "ITI_PROVISION_1_FAULT");
	assert_eq!(Name::parse("ITI_PROVISION_1_FAULT"), Some(name));
}

#[test]
fn the_suffix_is_the_one_of_the_qr_payload() {
	let keys = Root::from_bytes([7; 32]).device_keys();
	let payload = QrPayload::new(keys.presence_token.clone(), keys.fingerprint());
	let Suffix::Board(suffix) = Suffix::of(&payload) else {
		panic!("a board's suffix names it");
	};
	assert_eq!(suffix, payload.suffix());
	assert!(payload.encoded().ends_with(&suffix));
}

#[test]
fn names_that_are_not_a_provisioning_device_are_passed_over() {
	for name in [
		"",
		"ITI_PROVISION_",
		"ITI_PROVISION_1",
		"ITI_PROVISION_1_",
		"ITI_PROVISION__K7QA",
		"ITI_PROVISION_x_K7QA",
		"ITI_PROVISION_01_K7QA",
		"ITI_PROVISION_256_K7QA",
		"ITI_PROVISION_1_k7qa",
		"ITI_PROVISION_1_K7Q",
		"ITI_PROVISION_1_K7QAA",
		"ITI_PROVISION_1_K1QA",
		"ITI_PROVISION_1_FAULTY",
		"AAAAAAAAAAAAAAA",
	] {
		assert_eq!(Name::parse(name), None, "{name:?}");
	}
}

#[test]
fn the_longest_name_fits_the_advertisement() {
	let name = Name {
		marker: u8::MAX,
		suffix: Suffix::Fault,
	}
	.render();
	assert_eq!(name.len(), LONGEST_NAME);
}

#[test]
fn provisioning_keys_are_clamped_and_carry_the_token() {
	let keys = DeviceKeys::provisioning([0xff; 32], [0x11; 32]);
	assert_eq!(keys.presence_token.as_bytes(), &TOKEN);
	assert_eq!(keys.pre_shared_key, pre_shared_key());
	let key = keys.static_key.as_bytes();
	assert_eq!(key[0] & 0b111, 0);
	assert_eq!(key[31] & 0b1100_0000, 0b0100_0000);
}

/// A client holding nothing but the provisioning token completes the handshake against a device that
/// generated its keys at random, and the two can talk.
#[tokio::test]
async fn a_client_holding_only_the_token_completes_the_handshake() {
	use futures::{AsyncReadExt, AsyncWriteExt};
	use tokio_util::compat::TokioAsyncReadCompatExt;

	use crate::channel::stream::accept_responder;

	let keys = DeviceKeys::provisioning([0x42; 32], [0x24; 32]);
	let psk = pre_shared_key();
	let (a, b) = tokio::io::duplex(1 << 16);
	let (client, device) = tokio::join!(
		connect_initiator_unauthenticated(a.compat(), &psk),
		accept_responder(b.compat(), &keys)
	);
	let (mut client, mut device) = (client.unwrap(), device.unwrap());
	client.write_all(b"hello").await.unwrap();
	client.flush().await.unwrap();
	let mut buf = [0u8; 5];
	device.read_exact(&mut buf).await.unwrap();
	assert_eq!(&buf, b"hello");
}

#[test]
fn run_states_read_back_by_their_wire_names() {
	for state in RunState::ALL {
		assert_eq!(RunState::read(state.as_str()), Some(*state));
	}
	assert_eq!(RunState::read("paused"), None);
	assert_eq!(DoneState::read("not-done"), Some(DoneState::NotDone));
	assert_eq!(StepState::read("unchosen"), Some(StepState::Unchosen));
}

#[test]
fn a_prompt_kind_a_newer_device_defines_is_kept() {
	assert_eq!(
		PromptKind::read("photo"),
		PromptKind::Other("photo".to_owned())
	);
	assert_eq!(PromptKind::read("photo").as_str(), "photo");
	assert_eq!(PromptKind::read("secret"), PromptKind::Secret);
}

#[test]
fn an_answer_does_not_show_in_a_log() {
	assert_eq!(format!("{:?}", Answer::text("hunter2")), "Answer(..)");
	assert_eq!(Answer::text("x").as_text(), Some("x"));
	assert_eq!(Answer::boolean(true).as_bool(), Some(true));
	assert_eq!(Answer::boolean(true).as_text(), None);
}

fn step_with(result: Option<serde_json::Value>) -> StepReport {
	StepReport {
		id: "disk".to_owned(),
		title: "Disk".to_owned(),
		done: DoneState::NotDone,
		why: None,
		state: StepState::Passed,
		progress: None,
		reason: None,
		result: result.map(|result| result.as_object().unwrap().clone()),
	}
}

#[test]
fn the_record_is_found_among_the_steps_results() {
	let record = serde_json::json!({
		"record": {
			"suffix": "K7QA",
			"version": "4.0",
			"disk": "3f6c0a52-4a8a-4b6f-9b1c-0f6f1d9d6a11",
			"passphrase": "hunter2-hunter2",
			"at": "2026-10-29T03:04:05Z",
		}
	});
	let steps = [
		step_with(None),
		step_with(Some(serde_json::json!({"serial": "abc"}))),
		step_with(Some(record)),
	];
	let found = report::Record::find(&steps).unwrap();
	assert_eq!(found.suffix, "K7QA");
	assert_eq!(found.version.as_deref(), Some("4.0"));
	assert_eq!(found.passphrase.as_deref(), Some("hunter2-hunter2"));
	assert_eq!(found.at.to_string(), "2026-10-29T03:04:05Z");
	assert_eq!(report::Record::find(&steps[..2]), None);
}

#[test]
fn a_record_may_leave_out_what_the_run_did_not_establish() {
	let steps = [step_with(Some(serde_json::json!({
		"record": {"suffix": "FAULT", "at": "2026-10-29T03:04:05+13:00"}
	})))];
	let found = report::Record::find(&steps).unwrap();
	assert_eq!(
		(found.version, found.disk, found.passphrase),
		(None, None, None)
	);
}

#[test]
fn a_record_does_not_show_its_passphrase_in_a_log() {
	let steps = [step_with(Some(serde_json::json!({
		"record": {"suffix": "K7QA", "passphrase": "hunter2", "at": "2026-10-29T03:04:05Z"}
	})))];
	let found = report::Record::find(&steps).unwrap();
	assert!(!format!("{found:?}").contains("hunter2"));
}

#[test]
fn the_revision_a_device_sends_is_one_a_client_implements() {
	assert!(CLIENT_REVISIONS.contains(&REVISION));
}
