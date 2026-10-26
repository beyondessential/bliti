use super::*;
use crate::tailscale::state::Snapshot;

fn observed(state: State) -> Observed {
	Observed::Standing(Snapshot {
		state,
		name: None,
		tailnet: None,
		key_expiry: None,
		warning: None,
	})
}

fn names(acts: Vec<Act>) -> Vec<&'static str> {
	acts.into_iter().map(Act::name).collect()
}

/// The table of TSC, row for row.
#[test]
fn the_table_of_acts_by_state() {
	let table = [
		(
			State::Running,
			vec!["turn-off", "log-out", "netcheck", "ping"],
		),
		(State::Starting, vec!["turn-off", "log-out", "netcheck"]),
		(State::Stopped, vec!["turn-on", "log-out", "netcheck"]),
		(State::NeedsLogin, vec!["sign-in", "netcheck"]),
		(State::Expired, vec!["sign-in", "netcheck"]),
		(State::NeedsApproval, vec!["log-out", "netcheck"]),
	];
	for (state, expected) in table {
		assert_eq!(
			names(listed(Some(&observed(state)), true)),
			expected,
			"{}",
			state.name()
		);
	}
}

#[test]
fn a_broken_daemon_lists_only_netcheck() {
	let broken = Observed::Broken("down".into());
	assert_eq!(names(listed(Some(&broken), true)), ["netcheck"]);
	assert_eq!(names(listed(Some(&broken), false)), Vec::<&str>::new());
}

#[test]
fn a_device_without_tailscale_lists_only_netcheck() {
	assert_eq!(names(listed(None, true)), ["netcheck"]);
	assert_eq!(names(listed(None, false)), Vec::<&str>::new());
}

#[test]
fn netcheck_is_listed_in_every_state_where_it_can_run() {
	for state in [
		State::Running,
		State::Starting,
		State::Stopped,
		State::NeedsLogin,
		State::Expired,
		State::NeedsApproval,
	] {
		assert!(listed(Some(&observed(state)), true).contains(&Act::Netcheck));
		assert!(!listed(Some(&observed(state)), false).contains(&Act::Netcheck));
	}
}

#[test]
fn an_act_round_trips_through_its_name() {
	for act in Act::ALL {
		assert_eq!(Act::from_name(act.name()), Some(act));
	}
	assert_eq!(Act::from_name("restart"), None);
}

#[test]
fn a_sign_in_needs_a_method_it_knows() {
	let parse = |method: Option<&str>, key: Option<&str>| {
		Request::parse(Act::SignIn, method, key.map(Secret::new), None, None)
	};
	assert_eq!(parse(None, None).unwrap_err().at, "$['method']");
	assert_eq!(
		parse(Some("carrier-pigeon"), None).unwrap_err().at,
		"$['method']"
	);
	assert_eq!(parse(Some("auth-key"), None).unwrap_err().at, "$['key']");
	assert!(parse(Some("auth-key"), Some("k")).is_ok());
	assert!(parse(Some("login-link"), None).is_ok());
}

#[test]
fn only_waiting_acts_end_with_their_stream() {
	let waiting = |request: Request| request.ends_with_its_stream();
	assert!(waiting(Request::Netcheck));
	assert!(waiting(Request::Ping { peer: None }));
	assert!(waiting(Request::SignIn {
		method: Method::LoginLink,
		name: None
	}));
	assert!(!waiting(Request::SignIn {
		method: Method::AuthKey(Secret::new("k")),
		name: None
	}));
	assert!(!waiting(Request::TurnOn));
	assert!(!waiting(Request::TurnOff));
	assert!(!waiting(Request::LogOut));
}

#[test]
fn the_slot_is_held_until_dropped() {
	let held = Arc::new(Mutex::new(None));
	let slot = Slot::take(&held, Act::TurnOn).unwrap();
	assert_eq!(Slot::take(&held, Act::LogOut).unwrap_err(), Act::TurnOn);
	drop(slot);
	assert!(Slot::take(&held, Act::LogOut).is_ok());
}
