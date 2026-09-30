//! The command line, against a daemon's socket served in-process and against the curve file with no
//! daemon (CRV, "At the device").

use serde_json::{Value as Json, json};
use tempfile::TempDir;

use super::cli::{Action, Paths, run};
use crate::facts::{
	Supply,
	curve::{
		Document,
		store::{Store, Stored},
	},
};

fn learnt() -> Json {
	json!({
		"discharging": {
			"points": [[2.6, 0.0], [2.8, 0.2], [4.2, 1.0]],
			"learnt-from": 2,
			"error": 0.1,
			"duration": 20000.0,
		},
		"charging": {
			"points": [[3.6, 0.2], [4.2, 1.0]],
			"learnt-from": 1,
			"error": 0.15,
			"duration": 9000.0,
		},
	})
}

fn invalid() -> Json {
	let mut invalid = learnt();
	invalid["discharging"]["points"] = json!([[2.6, 0.0]]);
	invalid
}

fn paths(dir: &TempDir) -> Paths {
	Paths {
		socket: dir.path().join("battery.sock"),
		store: dir.path().join("battery-curve.json"),
	}
}

async fn export(paths: &Paths) -> anyhow::Result<Json> {
	let mut out = Vec::new();
	run(Action::Export, paths, &mut out).await?;
	Ok(serde_json::from_slice(&out).expect("the export is JSON"))
}

#[cfg(unix)]
mod through_the_daemon {
	use super::*;
	use crate::battery::socket::listen;

	/// A daemon managing a backup supply, listening on the socket of `paths`.
	fn daemon(paths: &Paths) -> (Supply, crate::session::AbortOnDrop) {
		let supply = Supply::managed_for_test(Store::new(&paths.store));
		let listening = listen(&paths.socket, supply.clone()).unwrap();
		(supply, listening)
	}

	/// Export, import and reset each take effect in the running daemon (CRV).
	#[tokio::test]
	async fn the_command_line_changes_the_running_daemons_curves() {
		let dir = tempfile::tempdir().unwrap();
		let paths = paths(&dir);
		let (supply, _listening) = daemon(&paths);

		assert_eq!(export(&paths).await.unwrap(), Document::shipped().to_json());

		run(Action::Import(learnt()), &paths, &mut Vec::new())
			.await
			.unwrap();
		let loaded = Document::from_json(&learnt()).unwrap();
		assert_eq!(supply.document(), Some(loaded.clone()));
		assert_eq!(export(&paths).await.unwrap(), loaded.to_json());

		run(Action::Reset, &paths, &mut Vec::new()).await.unwrap();
		assert_eq!(supply.document(), Some(Document::shipped()));
	}

	/// A refusal fails with the daemon's reason, and changes nothing (CRV).
	#[tokio::test]
	async fn a_refused_import_fails_with_the_reason() {
		let dir = tempfile::tempdir().unwrap();
		let paths = paths(&dir);
		let (supply, _listening) = daemon(&paths);

		let err = run(Action::Import(invalid()), &paths, &mut Vec::new())
			.await
			.unwrap_err();
		let reason = Document::from_json(&invalid()).unwrap_err().to_string();
		assert_eq!(err.to_string(), reason);
		assert_eq!(supply.document(), Some(Document::shipped()));
	}

	/// A daemon managing no backup supply holds no document to export, and refuses a reset.
	#[tokio::test]
	async fn a_daemon_with_no_backup_supply_has_nothing_to_change() {
		let dir = tempfile::tempdir().unwrap();
		let paths = paths(&dir);
		let _listening = listen(&paths.socket, Supply::default()).unwrap();

		assert!(export(&paths).await.is_err());
		let err = run(Action::Reset, &paths, &mut Vec::new())
			.await
			.unwrap_err();
		assert_eq!(err.to_string(), "this device manages no backup supply");
		assert!(
			!paths.store.exists(),
			"nothing is written behind the daemon"
		);
	}

	/// A socket a daemon left behind is replaced, and one a daemon still answers on is not.
	#[tokio::test]
	async fn a_stale_socket_is_replaced_and_a_live_one_kept() {
		let dir = tempfile::tempdir().unwrap();
		let paths = paths(&dir);
		drop(std::os::unix::net::UnixListener::bind(&paths.socket).unwrap());
		assert!(paths.socket.exists());

		let (_supply, _listening) = daemon(&paths);
		assert!(listen(&paths.socket, Supply::default()).is_err());
		assert_eq!(export(&paths).await.unwrap(), Document::shipped().to_json());
	}
}

/// With no daemon, export reads the curve file, the shipped curve where there is none.
#[tokio::test]
async fn with_no_daemon_export_reads_the_file() {
	let dir = tempfile::tempdir().unwrap();
	let paths = paths(&dir);
	assert_eq!(export(&paths).await.unwrap(), Document::shipped().to_json());

	let loaded = Document::from_json(&learnt()).unwrap();
	Store::new(&paths.store)
		.save(&Stored {
			document: loaded.clone(),
			gauge_full: None,
		})
		.unwrap();
	assert_eq!(export(&paths).await.unwrap(), loaded.to_json());
}

/// With no daemon, import and reset write the curve file, keeping the gauge's full reading, and an
/// invalid document is refused with its reason and writes nothing.
#[tokio::test]
async fn with_no_daemon_import_and_reset_write_the_file() {
	let dir = tempfile::tempdir().unwrap();
	let paths = paths(&dir);
	let store = Store::new(&paths.store);
	store
		.save(&Stored {
			document: Document::shipped(),
			gauge_full: Some(0.97),
		})
		.unwrap();

	run(Action::Import(learnt()), &paths, &mut Vec::new())
		.await
		.unwrap();
	assert_eq!(
		store.read().unwrap(),
		Some(Stored {
			document: Document::from_json(&learnt()).unwrap(),
			gauge_full: Some(0.97),
		})
	);

	let err = run(Action::Import(invalid()), &paths, &mut Vec::new())
		.await
		.unwrap_err();
	assert_eq!(
		err.to_string(),
		Document::from_json(&invalid()).unwrap_err().to_string()
	);
	assert_eq!(
		store.read().unwrap().map(|stored| stored.document),
		Some(Document::from_json(&learnt()).unwrap())
	);

	run(Action::Reset, &paths, &mut Vec::new()).await.unwrap();
	assert_eq!(
		store.read().unwrap(),
		Some(Stored {
			document: Document::shipped(),
			gauge_full: Some(0.97),
		})
	);
}
