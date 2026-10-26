use super::*;

fn sydney() -> BTreeMap<u32, String> {
	BTreeMap::from([(5, "Sydney".to_owned()), (9, "Dallas".to_owned())])
}

#[test]
fn a_name_found_survives_a_restart() {
	let directory = tempfile::tempdir().unwrap();
	Names::load(directory.path()).replace(sydney());
	let reloaded = Names::load(directory.path());
	assert_eq!(reloaded.name(5).as_deref(), Some("Sydney"));
}

#[test]
fn an_id_not_in_the_cache_has_no_name() {
	let names = Names::in_memory();
	names.replace(sydney());
	assert_eq!(names.name(77), None);
}

#[test]
fn a_corrupt_file_is_an_empty_cache() {
	let directory = tempfile::tempdir().unwrap();
	std::fs::write(directory.path().join(FILE), "not json").unwrap();
	assert_eq!(Names::load(directory.path()).name(5), None);
	std::fs::write(directory.path().join(FILE), r#"{"five":"Sydney"}"#).unwrap();
	assert_eq!(Names::load(directory.path()).name(5), None);
}

#[test]
fn a_missing_directory_is_made_when_names_are_kept() {
	let directory = tempfile::tempdir().unwrap();
	let nested = directory.path().join("state");
	Names::load(&nested).replace(sydney());
	assert!(nested.join(FILE).exists());
}

#[test]
fn a_refresh_replaces_the_names() {
	let names = Names::in_memory();
	names.replace(sydney());
	names.replace(BTreeMap::from([(5, "Auckland".to_owned())]));
	assert_eq!(names.name(5).as_deref(), Some("Auckland"));
	assert_eq!(names.name(9), None);
}

#[tokio::test]
async fn a_region_the_map_leaves_unnamed_has_no_name() {
	let fake = crate::tailscale::fake::Fake::in_state("Running");
	fake.relays(&[(5, "Sydney"), (9, "")]);
	let names = Names::in_memory();
	names.refresh(&*fake).await;
	assert_eq!(names.name(5).as_deref(), Some("Sydney"));
	assert_eq!(names.name(9), None);
}
