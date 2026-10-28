//! The name a device in provisioning mode advertises (PRV, "The advertised name").

use bliti_core::{
	board_id::SourceKind,
	provisioning::{Name, Suffix},
	qr::QrPayload,
	version::VERSION_MARKER,
};

use crate::identity::{self, IdentityError};

/// The device's name, from its board ID taken from a source of the `expected` kind.
///
/// Derives the board's root, which is memory-hard and slow, so it runs on a blocking task. It is
/// called once as the daemon starts, ahead of any press of the USER button, and the name it gives
/// stands for as long as the process lives.
pub async fn establish(expected: SourceKind) -> Name {
	let established =
		tokio::task::spawn_blocking(move || identity::establish_provisioning(expected)).await;
	match established {
		Ok(result) => named(result),
		Err(err) => {
			tracing::error!(%err, "establishing this board's ID panicked; the suffix is FAULT");
			Name {
				marker: VERSION_MARKER,
				suffix: Suffix::Fault,
			}
		}
	}
}

/// The name of a device whose board ID gave `result`.
///
/// Where it could not be established the suffix is `FAULT`, and the cause is reported where the device
/// is rather than to a client (PRV, DEV).
fn named(result: Result<QrPayload, IdentityError>) -> Name {
	let suffix = match result {
		Ok(payload) => Suffix::of(&payload),
		Err(err) => {
			tracing::error!(%err, "cannot establish this board's ID; the suffix is FAULT");
			Suffix::Fault
		}
	};
	Name {
		marker: VERSION_MARKER,
		suffix,
	}
}

#[cfg(test)]
mod tests {
	use bliti_core::{
		board_id::{BoardId, BoardIdSource, TestSource},
		key_schedule::{KeyError, ROOT_LEN, Root},
		provisioning::FAULT,
	};

	use super::*;

	/// A root that depends on the board ID, without the memory-hard derivation.
	fn cheap_root(board_id: &BoardId) -> Result<Root, KeyError> {
		let mut root = [board_id.kind().tag(); ROOT_LEN];
		root[1..=board_id.raw().len()].copy_from_slice(board_id.raw());
		Ok(Root::from_bytes(root))
	}

	fn name_of(expected: SourceKind, sources: &[TestSource], memory: u64) -> Name {
		let refs: Vec<&dyn BoardIdSource> = sources
			.iter()
			.map(|source| source as &dyn BoardIdSource)
			.collect();
		named(identity::provisioning_payload(
			expected, &refs, memory, cheap_root,
		))
	}

	fn pi(serial: [u8; 4]) -> [TestSource; 3] {
		[
			TestSource::absent(SourceKind::TpmEndorsementKey),
			TestSource::absent(SourceKind::OneTimeProgrammable),
			TestSource::present(SourceKind::RaspberryPiSerial, serial),
		]
	}

	#[test]
	fn a_good_source_gives_the_suffix_of_its_qr_payload() {
		let name = name_of(SourceKind::RaspberryPiSerial, &pi([1, 2, 3, 4]), u64::MAX);
		let board_id = BoardId::new(SourceKind::RaspberryPiSerial, [1, 2, 3, 4]).unwrap();
		let keys = cheap_root(&board_id).unwrap().device_keys();
		let payload = QrPayload::new(keys.presence_token.clone(), keys.fingerprint());

		assert_eq!(name.suffix, Suffix::Board(payload.suffix()));
		assert_eq!(name.marker, VERSION_MARKER);
		assert_eq!(
			name.render(),
			format!("ITI_PROVISION_{VERSION_MARKER}_{}", payload.suffix())
		);
	}

	#[test]
	fn two_boards_are_told_apart() {
		let one = name_of(SourceKind::RaspberryPiSerial, &pi([1, 2, 3, 4]), u64::MAX);
		let other = name_of(SourceKind::RaspberryPiSerial, &pi([4, 3, 2, 1]), u64::MAX);
		assert_ne!(one, other);
	}

	#[test]
	fn a_board_whose_expected_source_is_missing_is_a_fault() {
		let name = name_of(SourceKind::OneTimeProgrammable, &pi([1, 2, 3, 4]), u64::MAX);
		assert_eq!(name.suffix, Suffix::Fault);
		assert!(name.render().ends_with(FAULT));
	}

	#[test]
	fn a_weaker_source_is_never_used() {
		let weaker = name_of(SourceKind::OneTimeProgrammable, &pi([1, 2, 3, 4]), u64::MAX);
		let board = name_of(SourceKind::RaspberryPiSerial, &pi([1, 2, 3, 4]), u64::MAX);
		assert_eq!(weaker.suffix, Suffix::Fault);
		assert_ne!(weaker, board);
	}

	#[test]
	fn a_stronger_source_than_expected_is_a_fault() {
		let sources = [
			TestSource::absent(SourceKind::TpmEndorsementKey),
			TestSource::present(SourceKind::OneTimeProgrammable, [9, 9, 9, 9]),
			TestSource::present(SourceKind::RaspberryPiSerial, [1, 2, 3, 4]),
		];
		let name = name_of(SourceKind::RaspberryPiSerial, &sources, u64::MAX);
		assert_eq!(name.suffix, Suffix::Fault);
	}

	#[test]
	fn too_little_memory_is_a_fault() {
		let name = name_of(SourceKind::RaspberryPiSerial, &pi([1, 2, 3, 4]), 1 << 20);
		assert_eq!(name.suffix, Suffix::Fault);
	}

	#[test]
	fn a_name_made_here_is_one_a_client_reads() {
		let name = name_of(SourceKind::RaspberryPiSerial, &pi([1, 2, 3, 4]), u64::MAX);
		assert_eq!(Name::parse(&name.render()), Some(name));
	}
}
