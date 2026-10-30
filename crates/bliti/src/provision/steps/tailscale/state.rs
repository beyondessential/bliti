//! Reading the Tailscale state a system keeps, to tell whether it records a sign-in (PTS).
//!
//! tailscaled keeps a JSON object whose members are state keys and whose values are base64: the
//! key of the current profile under `_current-profile`, and under that key the profile's
//! preferences, also JSON, holding whether the user logged out and the node's persisted identity
//! under `Config`.

use std::collections::HashMap;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;

/// Where the state is, under the root filesystem.
pub const PATH: &str = "var/lib/tailscale/tailscaled.state";

const CURRENT_PROFILE: &str = "_current-profile";

#[derive(Deserialize)]
struct Prefs {
	#[serde(rename = "LoggedOut", default)]
	logged_out: bool,
	#[serde(rename = "Config", default)]
	persisted: Option<Persisted>,
}

#[derive(Deserialize)]
struct Persisted {
	#[serde(rename = "NodeID", default)]
	node_id: String,
}

/// Whether the state in `file` records a sign-in: the current profile exists, has not been logged
/// out, and has a node.
pub fn records_sign_in(file: &[u8]) -> Result<bool, String> {
	let state: HashMap<String, String> = serde_json::from_slice(file)
		.map_err(|err| format!("the Tailscale state is not readable: {err}"))?;
	let decode = |key: &str| -> Result<Option<Vec<u8>>, String> {
		state
			.get(key)
			.map(|value| {
				STANDARD
					.decode(value)
					.map_err(|err| format!("the Tailscale state holds an unreadable {key}: {err}"))
			})
			.transpose()
	};
	let Some(current) = decode(CURRENT_PROFILE)? else {
		return Ok(false);
	};
	let current = String::from_utf8(current)
		.map_err(|_| "the Tailscale state names its current profile unreadably".to_owned())?;
	let Some(prefs) = decode(&current)? else {
		return Ok(false);
	};
	if prefs.is_empty() {
		return Ok(false);
	}
	let prefs: Prefs = serde_json::from_slice(&prefs)
		.map_err(|err| format!("the Tailscale state holds unreadable preferences: {err}"))?;
	Ok(!prefs.logged_out
		&& prefs
			.persisted
			.is_some_and(|persisted| !persisted.node_id.is_empty()))
}

#[cfg(test)]
pub fn file(current: Option<&str>, profiles: &[(&str, &str)]) -> Vec<u8> {
	let mut state = serde_json::Map::new();
	if let Some(current) = current {
		state.insert(CURRENT_PROFILE.into(), STANDARD.encode(current).into());
	}
	for (key, prefs) in profiles {
		state.insert((*key).into(), STANDARD.encode(prefs).into());
	}
	serde_json::to_vec(&state).unwrap()
}

#[cfg(test)]
mod tests {
	use super::*;

	const SIGNED_IN: &str = r#"{"WantRunning":true,"LoggedOut":false,"Hostname":"tamanu-iti-k7qa","Config":{"NodeID":"nABC123CNTRL","PrivateNodeKey":"privkey:00"}}"#;

	#[test]
	fn a_current_profile_with_a_node_is_a_sign_in() {
		let file = file(Some("profile-1a2b"), &[("profile-1a2b", SIGNED_IN)]);
		assert_eq!(records_sign_in(&file), Ok(true));
	}

	#[test]
	fn a_profile_that_was_logged_out_is_not() {
		let prefs = r#"{"LoggedOut":true,"Config":{"NodeID":"nABC123CNTRL"}}"#;
		let file = file(Some("profile-1a2b"), &[("profile-1a2b", prefs)]);
		assert_eq!(records_sign_in(&file), Ok(false));
	}

	#[test]
	fn a_profile_with_no_node_is_not() {
		let prefs = r#"{"LoggedOut":false,"Config":{"NodeID":""}}"#;
		assert_eq!(
			records_sign_in(&file(Some("profile-1"), &[("profile-1", prefs)])),
			Ok(false)
		);
		assert_eq!(
			records_sign_in(&file(Some("profile-1"), &[("profile-1", "{}")])),
			Ok(false)
		);
	}

	#[test]
	fn state_that_names_no_current_profile_or_whose_profile_is_missing_is_not() {
		assert_eq!(records_sign_in(&file(None, &[])), Ok(false));
		assert_eq!(records_sign_in(b"{}"), Ok(false));
		assert_eq!(
			records_sign_in(&file(Some("profile-1"), &[("profile-2", SIGNED_IN)])),
			Ok(false)
		);
	}

	#[test]
	fn unreadable_state_is_an_error() {
		assert!(records_sign_in(b"not json").is_err());
		let bad = br#"{"_current-profile": "!!!"}"#;
		assert!(records_sign_in(bad).is_err());
		let file = file(Some("profile-1"), &[("profile-1", "{")]);
		assert!(records_sign_in(&file).is_err());
	}
}
