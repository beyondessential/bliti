use super::*;

fn report(json: &str) -> Report {
	serde_json::from_str(json).unwrap()
}

fn names() -> Names {
	let names = Names::in_memory();
	names.replace(BTreeMap::from([
		(5, "Sydney".to_owned()),
		(9, "Dallas".to_owned()),
	]));
	names
}

#[test]
fn every_relay_answering_is_named_and_sorted_nearest_first() {
	let result = result(
		&report(
			r#"{"UDP":true,"IPv4":true,"IPv6":true,"MappingVariesByDestIP":false,"CaptivePortal":false,
			"RegionLatency":{"9":150000000,"5":38400000,"12":90000000}}"#,
		),
		&names(),
	);
	assert_eq!(
		result["relays"],
		json!([
			{"id": 5, "name": "Sydney", "latency": 0.0384},
			{"id": 12, "name": null, "latency": 0.09},
			{"id": 9, "name": "Dallas", "latency": 0.15},
		])
	);
	assert_eq!(result["udp"], json!(true));
	assert_eq!(result["nat-varies"], json!(false));
}

#[test]
fn https_only_relays_answer_with_udp_false() {
	let result = result(
		&report(r#"{"UDP":false,"IPv4":true,"RegionLatency":{"5":120000000}}"#),
		&names(),
	);
	assert_eq!(result["udp"], json!(false));
	assert_eq!(result["relays"].as_array().unwrap().len(), 1);
}

#[test]
fn no_relay_answering_is_a_result_with_udp_false() {
	let result = result(
		&report(r#"{"UDP":true,"IPv4":true,"RegionLatency":{}}"#),
		&names(),
	);
	assert_eq!(
		result["udp"],
		json!(false),
		"no relay means no UDP got through"
	);
	assert_eq!(result["relays"], json!([]));
	let absent = super::result(&report(r#"{"IPv4":true}"#), &names());
	assert_eq!(absent["relays"], json!([]));
}

#[test]
fn an_unknown_ipv6_and_nat_are_null_or_false_as_the_report_says() {
	let result = result(&report(r#"{"UDP":true,"RegionLatency":{"5":1}}"#), &names());
	assert_eq!(result["ipv6"], json!(false));
	assert_eq!(result["nat-varies"], json!(null));
	assert_eq!(result["captive-portal"], json!(null));
}

#[test]
fn a_captive_portal_is_reported() {
	let result = result(
		&report(r#"{"CaptivePortal":true,"RegionLatency":{}}"#),
		&names(),
	);
	assert_eq!(result["captive-portal"], json!(true));
}

#[tokio::test]
async fn a_command_that_cannot_start_fails_with_its_own_reason() {
	let runner = Command::new("/nonexistent/tailscale");
	assert!(!runner.available());
	let reason = runner.run().await.unwrap_err();
	assert!(
		reason.starts_with("cannot run the network check"),
		"{reason}"
	);
}
