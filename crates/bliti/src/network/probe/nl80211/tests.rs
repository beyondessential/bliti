use wl_nl80211::packet_core::Emitable as _;

use super::*;

fn bytes(message: &Nl80211Message) -> Vec<u8> {
	let mut buffer = vec![0; message.buffer_len()];
	message.emit(&mut buffer);
	buffer
}

fn interface(index: u32, name: &str, wiphy: u32, kind: Nl80211InterfaceType) -> Interface {
	Interface {
		index,
		name: name.into(),
		wiphy,
		kind,
	}
}

fn station_only() -> Vec<Interface> {
	vec![interface(3, "wlan0", 1, Nl80211InterfaceType::Station)]
}

#[test]
fn the_regulatory_domain_goes_as_a_nul_terminated_alpha2_attribute() {
	let message = set_regulatory_domain("NZ").unwrap();
	assert_eq!(message.cmd, Nl80211Command::ReqSetReg);
	// Length 7 (header and "NZ\0"), type 33, padded to four bytes.
	assert_eq!(bytes(&message), [7, 0, 33, 0, b'N', b'Z', 0, 0]);
	assert_eq!(
		bytes(&set_regulatory_domain("00").unwrap()),
		[7, 0, 33, 0, b'0', b'0', 0, 0],
		"the world domain"
	);
}

#[test]
fn a_regulatory_domain_neither_alpha2_nor_the_world_domain_is_refused() {
	for domain in ["nz", "NZL", "N", "", "0", "01", "N1"] {
		assert!(set_regulatory_domain(domain).is_err(), "{domain:?}");
	}
}

#[test]
fn the_access_point_is_created_on_the_station_s_wiphy() {
	let mut interfaces = station_only();
	interfaces.push(interface(4, "wlan1", 2, Nl80211InterfaceType::Station));
	let message = create_access_point(&interfaces, "wlan1", "ap0")
		.unwrap()
		.unwrap();
	assert_eq!(message.cmd, Nl80211Command::NewInterface);
	assert_eq!(
		message.attributes,
		[
			Nl80211Attr::Wiphy(2),
			Nl80211Attr::IfType(Nl80211InterfaceType::Ap),
			Nl80211Attr::IfName("ap0".into()),
		]
	);
}

#[test]
fn an_existing_interface_is_not_created_again_whatever_its_type() {
	for kind in [Nl80211InterfaceType::Ap, Nl80211InterfaceType::Station] {
		let mut interfaces = station_only();
		interfaces.push(interface(5, "ap0", 1, kind));
		assert!(
			create_access_point(&interfaces, "wlan0", "ap0")
				.unwrap()
				.is_none(),
			"{kind:?}"
		);
	}
}

#[test]
fn an_access_point_on_a_radio_that_is_not_wireless_is_refused() {
	assert!(create_access_point(&station_only(), "eth0", "ap0").is_err());
}

#[test]
fn the_access_point_is_deleted_by_its_index_where_it_exists() {
	let mut interfaces = station_only();
	interfaces.push(interface(5, "ap0", 1, Nl80211InterfaceType::Ap));
	let message = delete_access_point(&interfaces, "ap0").unwrap();
	assert_eq!(message.cmd, Nl80211Command::DelInterface);
	assert_eq!(message.attributes, [Nl80211Attr::IfIndex(5)]);
	assert!(delete_access_point(&station_only(), "ap0").is_none());
}
