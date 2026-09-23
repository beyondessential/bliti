use super::*;

fn radio(station: &str, alongside: Option<Alongside>) -> RadioInfo {
	RadioInfo {
		station: station.into(),
		model: "test".into(),
		bands: BTreeMap::new(),
		alongside,
		sae: false,
		scan: true,
		survey: false,
	}
}

#[test]
fn selection_runs_on_every_radio_in_probe_order() {
	let radios = [
		radio("wlan0", Some(Alongside::SharedChannel)),
		radio("wlan1", None),
	];
	let hardware = select_hardware(&radios, &["eth0".into()]);
	assert_eq!(
		hardware,
		select::Hardware {
			wired: vec!["eth0".into()],
			radios: vec![
				select::Radio {
					station: "wlan0".into(),
					access_point: Some(Alongside::SharedChannel),
				},
				select::Radio {
					station: "wlan1".into(),
					access_point: None,
				},
			],
		}
	);
}

#[test]
fn rendering_runs_on_the_radio_named() {
	let shared = radio("wlan0", Some(Alongside::SharedChannel));
	let hardware = render_hardware(
		Some(&shared),
		&["eth0".into()],
		"ap0",
		render::Paths::system(),
	);
	assert_eq!(hardware.station.as_deref(), Some("wlan0"));
	assert_eq!(hardware.access_point.as_deref(), Some("ap0"));
	assert!(hardware.shared_channel);

	let independent = radio("wlan0", Some(Alongside::Independent));
	let hardware = render_hardware(Some(&independent), &[], "ap0", render::Paths::system());
	assert!(!hardware.shared_channel);
}

#[test]
fn a_radio_without_access_point_mode_renders_no_access_point() {
	let client = radio("wlan1", None);
	let hardware = render_hardware(Some(&client), &[], "ap0", render::Paths::system());
	assert_eq!(hardware.station.as_deref(), Some("wlan1"));
	assert_eq!(hardware.access_point, None);

	let none = render_hardware(None, &["eth0".into()], "ap0", render::Paths::system());
	assert_eq!((none.station, none.access_point), (None, None));
}
