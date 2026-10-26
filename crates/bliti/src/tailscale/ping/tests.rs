use std::collections::BTreeMap;

use super::*;
use crate::tailscale::fake::Fake;

fn relayed(latency: f64) -> Pong {
	Pong {
		latency_seconds: latency,
		relay_region: 5,
		..Pong::default()
	}
}

fn direct(latency: f64) -> Pong {
	Pong {
		latency_seconds: latency,
		endpoint: "192.0.2.1:41641".into(),
		..Pong::default()
	}
}

fn names() -> Names {
	let names = Names::in_memory();
	names.replace(BTreeMap::from([(5, "Sydney".to_owned())]));
	names
}

fn daemon() -> std::sync::Arc<Fake> {
	let fake = Fake::in_state("Running");
	fake.peer("canopy");
	fake
}

#[tokio::test]
async fn a_direct_path_on_the_first_probe_stops_there() {
	let fake = daemon();
	fake.pong(direct(0.0018));
	let result = run(&*fake, "canopy", &names()).await.unwrap();
	assert_eq!(result["path"], json!("direct"));
	assert_eq!(result["relay"], json!(null));
	assert_eq!(result["latency"], json!(0.0018));
	assert_eq!(result["peer"], json!("canopy.fleet.example"));
	assert_eq!(
		fake.calls()
			.iter()
			.filter(|call| call.starts_with("ping"))
			.count(),
		1
	);
}

#[tokio::test]
async fn relayed_then_direct_reports_direct_at_the_direct_latency() {
	let fake = daemon();
	fake.pong(relayed(0.04));
	fake.pong(direct(0.002));
	let result = run(&*fake, "canopy", &names()).await.unwrap();
	assert_eq!(result["path"], json!("direct"));
	assert_eq!(result["latency"], json!(0.002));
	assert_eq!(result["relay"], json!(null));
}

#[tokio::test]
async fn relayed_only_names_the_relay_and_spends_every_probe() {
	let fake = daemon();
	for _ in 0..PROBES {
		fake.pong(relayed(0.0384));
	}
	let result = run(&*fake, "canopy", &names()).await.unwrap();
	assert_eq!(result["path"], json!("relayed"));
	assert_eq!(result["relay"], json!({"id": 5, "name": "Sydney"}));
	assert_eq!(result["reachable"], json!(true));
	assert_eq!(
		fake.calls()
			.iter()
			.filter(|call| call.starts_with("ping"))
			.count(),
		PROBES
	);
}

#[tokio::test]
async fn no_answer_is_a_result_not_a_failure() {
	let fake = daemon();
	let result = run(&*fake, "canopy", &names()).await.unwrap();
	assert_eq!(result["reachable"], json!(false));
	assert_eq!(result["path"], json!(null));
	assert_eq!(result["relay"], json!(null));
	assert_eq!(result["latency"], json!(null));
}

#[tokio::test]
async fn an_unknown_peer_fails() {
	let fake = daemon();
	let reason = run(&*fake, "nowhere", &names()).await.unwrap_err();
	assert!(reason.contains("not a machine"), "{reason}");
	assert!(!fake.calls().iter().any(|call| call.starts_with("ping")));
}

#[tokio::test]
async fn an_address_is_not_a_peer() {
	let fake = daemon();
	assert!(run(&*fake, "100.64.0.2", &names()).await.is_err());
}

#[tokio::test]
async fn a_peer_is_found_bare_or_fully_qualified_with_or_without_the_dot() {
	for name in [
		"canopy",
		"canopy.fleet.example",
		"canopy.fleet.example.",
		"CANOPY",
	] {
		let fake = daemon();
		fake.pong(direct(0.001));
		assert!(run(&*fake, name, &names()).await.is_ok(), "{name}");
	}
}

#[tokio::test]
async fn a_peer_is_found_by_its_name_on_the_tailnet() {
	let fake = Fake::in_state("Running");
	fake.peer_named("a", "canopy-1", "100.64.0.3");
	fake.peer_named("b", "canopy", "100.64.0.4");
	fake.pong(direct(0.001));
	let result = run(&*fake, "canopy", &names()).await.unwrap();
	assert_eq!(result["peer"], json!("canopy.fleet.example"));
	assert!(fake.calls().contains(&"ping 100.64.0.4".to_owned()));
}
