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

/// Each radio able to run an access point has one named for its place among every radio probed,
/// so a radio's name does not turn on what the others can do.
#[test]
fn rendering_runs_on_every_radio_with_an_access_point_named_for_its_place() {
	let radios = [
		radio("wlan0", None),
		radio("wlan1", Some(Alongside::SharedChannel)),
		radio("wlx00c0ca123456", Some(Alongside::Independent)),
	];
	let hardware = render_hardware(&radios, &["eth0".into()], render::Paths::system());
	assert_eq!(hardware.wired, ["eth0"]);
	assert_eq!(
		hardware.radios,
		[
			render::Radio {
				station: "wlan0".into(),
				access_point: None,
			},
			render::Radio {
				station: "wlan1".into(),
				access_point: Some(render::AccessPoint {
					interface: "ap1".into(),
					alongside: Alongside::SharedChannel,
				}),
			},
			render::Radio {
				station: "wlx00c0ca123456".into(),
				access_point: Some(render::AccessPoint {
					interface: "ap2".into(),
					alongside: Alongside::Independent,
				}),
			},
		]
	);
	assert_eq!(hardware.access_point_radio("ap2"), Some("wlx00c0ca123456"));
	assert_eq!(hardware.access_point_radio("ap0"), None);

	let none = render_hardware(&[], &["eth0".into()], render::Paths::system());
	assert!(none.radios.is_empty());
}
