use serde_json::{Value as Json, json};

use super::*;

const CA: &str = "-----BEGIN CERTIFICATE-----\nMIIBszCCAVmgAwIBAgIU\n-----END CERTIFICATE-----";

fn document(json: Json) -> Document {
	let Json::Object(map) = json else {
		panic!("a document is an object")
	};
	Document::parse(&map).unwrap()
}

fn hardware() -> Hardware {
	Hardware {
		wired: vec!["eth0".into(), "eth1".into()],
		station: Some("wlan0".into()),
		access_point: Some("ap0".into()),
		shared_channel: false,
		paths: Paths {
			networkd: "/tmp/bliti-test/network".into(),
			iwd_state: "/tmp/bliti-test/iwd".into(),
			iwd_config: "/tmp/bliti-test/iwd.conf".into(),
			hostapd: "/tmp/bliti-test/hostapd.conf".into(),
			modprobe: "/tmp/bliti-test/regdom.conf".into(),
			resolved: "/tmp/bliti-test/dns-delegate.d".into(),
		},
	}
}

fn active(active: &[usize]) -> Selection {
	Selection {
		active: active.to_vec(),
		station_channel: None,
	}
}

fn rendered(document: &Document, hardware: &Hardware, selection: &Selection) -> Rendered {
	render(document, hardware, selection).unwrap()
}

/// The file at `path` under the test roots, which the render must hold.
fn file<'a>(rendered: &'a Rendered, path: &str) -> &'a File {
	let path = Path::new("/tmp/bliti-test").join(path);
	rendered
		.files
		.iter()
		.find(|file| file.path == path)
		.unwrap_or_else(|| panic!("no {path:?} among {:?}", paths(rendered)))
}

fn paths(rendered: &Rendered) -> Vec<&Path> {
	rendered
		.files
		.iter()
		.map(|file| file.path.as_path())
		.collect()
}

fn invalid_at(document: &Document, hardware: &Hardware) -> String {
	match render(document, hardware, &Selection::default()) {
		Err(Error::Invalid(invalid)) => invalid.at,
		other => panic!("expected an invalid document, got {other:?}"),
	}
}

fn wireless(ssid: &str, security: Json) -> Json {
	json!({ "kind": "wireless", "label": ssid, "ssid": ssid, "security": security })
}

fn peap() -> serde_json::Map<String, Json> {
	let Json::Object(map) = json!({
		"kind": "enterprise", "eap": "peap", "identity": "nurse@clinic",
		"anonymous-identity": "anonymous@clinic", "password": "hunter2",
		"phase2": "mschapv2", "ca-certificate": CA, "domain": "radius.clinic.example"
	}) else {
		unreachable!()
	};
	map
}

/// A dynamic wired candidate takes DHCPv4 and router advertisements, with the top rank's metric.
#[test]
fn wired_dynamic_only() {
	let doc = document(json!({
		"attachments": [{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0" }]
	}));
	let out = rendered(&doc, &hardware(), &active(&[0]));
	let network = &file(&out, "network/50-bliti-eth0.network").contents;
	assert!(network.contains("[Match]\nName=eth0\n"));
	assert!(network.contains("DHCP=ipv4\nIPv6AcceptRA=yes\n"));
	assert!(network.contains("DNSDefaultRoute=yes\n"));
	assert!(network.contains("[DHCPv4]\nUseDNS=yes\nUseDomains=route\nRouteMetric=100\n"));
	assert!(network.contains("[IPv6AcceptRA]\nUseDNS=yes\nUseDomains=route\nRouteMetric=100\n"));
	assert!(!network.contains("DNS=1"));
	assert_eq!(
		paths(&out),
		[
			Path::new("/tmp/bliti-test/iwd.conf"),
			Path::new("/tmp/bliti-test/network/50-bliti-eth0.network"),
			Path::new("/tmp/bliti-test/regdom.conf"),
		]
	);
}

/// Of two statics on one interface, the selected one is rendered, and selecting both is refused.
#[test]
fn two_statics_on_one_interface() {
	let doc = document(json!({
		"attachments": [
			{ "kind": "wired-static", "label": "site a", "interface": "eth0",
			  "addresses": ["10.1.0.5/24"], "gateway": "10.1.0.1" },
			{ "kind": "wired-static", "label": "site b", "interface": "eth0",
			  "addresses": ["10.2.0.5/24", "fd00:2::5/64"], "gateway": "10.2.0.1" }
		]
	}));
	let out = rendered(&doc, &hardware(), &active(&[1]));
	let network = &file(&out, "network/50-bliti-eth0.network").contents;
	assert!(network.contains("DHCP=no\nIPv6AcceptRA=no\n"));
	assert!(network.contains("Address=10.2.0.5/24\nAddress=fd00:2::5/64\n"));
	assert!(network.contains("[Route]\nGateway=10.2.0.1\nMetric=101\n"));
	assert!(!network.contains("10.1.0"));

	// With neither brought up, the link is only brought up, so its carrier can be seen.
	let nothing = rendered(&doc, &hardware(), &active(&[]));
	let idle = &file(&nothing, "network/50-bliti-eth0.network").contents;
	assert!(idle.contains("[Match]\nName=eth0\n"), "{idle}");
	assert!(
		idle.contains("DHCP=no\nIPv6AcceptRA=no\nLinkLocalAddressing=no\n"),
		"{idle}"
	);
	assert!(!idle.contains("Address="), "{idle}");
	assert!(
		!paths(&nothing)
			.iter()
			.any(|path| path.ends_with("50-bliti-eth1.network")),
		"an interface no candidate names is left alone"
	);

	assert!(matches!(
		render(&doc, &hardware(), &active(&[0, 1])),
		Err(Error::Selection(_))
	));
	assert!(matches!(
		render(&doc, &hardware(), &active(&[2])),
		Err(Error::Selection(_))
	));
}

/// Every candidate is checked, selected or not.
#[test]
fn an_unselected_candidate_is_still_checked() {
	let doc = document(json!({
		"attachments": [
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0" },
			{ "kind": "wired-static", "label": "bad", "interface": "eth1",
			  "addresses": ["10.2.0.5"], "gateway": "10.2.0.1" }
		]
	}));
	assert_eq!(
		invalid_at(&doc, &hardware()),
		"$['attachments'][1]['addresses'][0]"
	);
}

/// Wireless above wired: both up, the wireless link carries the lower metric, so the default route.
#[test]
fn wireless_and_wired_order_by_metric() {
	let doc = document(json!({
		"attachments": [
			wireless("Clinic", json!({ "kind": "psk-sae", "passphrase": "a long passphrase" })),
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0" }
		]
	}));
	let out = rendered(&doc, &hardware(), &active(&[0, 1]));
	let station = &file(&out, "network/50-bliti-wlan0.network").contents;
	assert!(station.contains("Name=wlan0\n"));
	assert!(station.contains("IgnoreCarrierLoss=3s\n"));
	assert!(station.contains("RouteMetric=100\n"));
	let wired = &file(&out, "network/50-bliti-eth0.network").contents;
	assert!(wired.contains("RouteMetric=101\n"));
	assert!(!wired.contains("IgnoreCarrierLoss"));
}

/// A dynamic candidate's own resolvers answer from a delegate bound to its link, leaving the link
/// the resolvers its network supplies and the site's domains routed to them. A static candidate has
/// nothing supplied, so its resolvers sit on the link (LINK).
#[test]
fn per_link_nameservers() {
	let doc = document(json!({
		"attachments": [
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0",
			  "nameservers": ["10.0.9.53", "2606:4700:4700::1111"] },
			{ "kind": "wired-static", "label": "lab", "interface": "eth1",
			  "addresses": ["192.0.2.5/24"], "gateway": "192.0.2.1", "nameservers": ["192.0.2.53"] }
		]
	}));
	let out = rendered(&doc, &hardware(), &active(&[0, 1]));
	let dynamic = &file(&out, "network/50-bliti-eth0.network").contents;
	assert!(!dynamic.contains("DNS=10.0.9.53"));
	assert!(dynamic.contains("DNSDefaultRoute=no\n"));
	assert!(dynamic.contains("[DHCPv4]\nUseDNS=yes\nUseDomains=route\n"));
	let delegate = &file(&out, "dns-delegate.d/50-bliti-eth0.dns-delegate");
	assert!(delegate.contents.contains(
		"[Delegate]\nDNS=10.0.9.53%eth0\nDNS=2606:4700:4700::1111%eth0\nDomains=~.\nDefaultRoute=yes\n"
	));
	assert_eq!(delegate.mode, PUBLIC);
	assert!(Paths::system().owns(std::path::Path::new(
		"/run/systemd/dns-delegate.d/50-bliti-eth0.dns-delegate"
	)));
	let fixed = &file(&out, "network/50-bliti-eth1.network").contents;
	assert!(fixed.contains("DNS=192.0.2.53\n"));

	let bad = document(json!({
		"attachments": [{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0",
		  "nameservers": ["1.1.1.1", "dns.example"] }]
	}));
	assert_eq!(
		invalid_at(&bad, &hardware()),
		"$['attachments'][0]['nameservers'][1]"
	);
}

/// PEAP renders its outer and inner identities and embeds the CA; without the CA, or the domain, the
/// candidate is refused naming the member.
#[test]
fn enterprise_with_and_without_a_ca() {
	let doc = document(json!({ "attachments": [wireless("eduroam", Json::Object(peap()))] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	let network = file(&out, "iwd/eduroam.8021x");
	assert_eq!(network.mode, SECRET);
	for line in [
		"EAP-Method=PEAP\n",
		"EAP-Identity=anonymous@clinic\n",
		"EAP-PEAP-CACert=embed:ca\n",
		"EAP-PEAP-ServerDomainMask=radius.clinic.example\n",
		"EAP-PEAP-Phase2-Method=MSCHAPV2\n",
		"EAP-PEAP-Phase2-Identity=nurse@clinic\n",
		"EAP-PEAP-Phase2-Password=hunter2\n",
		"[@pem@ca]\n-----BEGIN CERTIFICATE-----\n",
	] {
		assert!(
			network.contents.contains(line),
			"{line:?} in {}",
			network.contents
		);
	}

	for member in ["ca-certificate", "domain"] {
		let mut security = peap();
		security.remove(member);
		let doc = document(json!({ "attachments": [wireless("eduroam", Json::Object(security))] }));
		assert_eq!(
			invalid_at(&doc, &hardware()),
			format!("$['attachments'][0]['security']['{member}']")
		);
	}
}

/// TLS embeds the client's certificate and key beside the CA.
#[test]
fn enterprise_tls_embeds_the_client_credentials() {
	let doc = document(json!({ "attachments": [wireless("corp", json!({
		"kind": "enterprise", "eap": "tls", "identity": "device-7",
		"ca-certificate": CA, "domain": "*.corp.example",
		"client-certificate": CA, "client-key": "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----",
		"client-key-passphrase": "unlock"
	}))] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	let contents = &file(&out, "iwd/corp.8021x").contents;
	assert!(contents.contains("EAP-TLS-ClientCert=embed:client-certificate\n"));
	assert!(contents.contains("EAP-TLS-ClientKey=embed:client-key\n"));
	assert!(contents.contains("EAP-TLS-ClientKeyPassphrase=unlock\n"));
	assert!(contents.contains("[@pem@client-key]\n-----BEGIN PRIVATE KEY-----\n"));
}

/// A member the method does not use is refused rather than quietly dropped, and so is an unknown
/// method.
#[test]
fn enterprise_members_are_checked() {
	let mut security = peap();
	security.insert("client-key".into(), json!(CA));
	let doc = document(json!({ "attachments": [wireless("eduroam", Json::Object(security))] }));
	assert_eq!(
		invalid_at(&doc, &hardware()),
		"$['attachments'][0]['security']['client-key']"
	);

	let mut security = peap();
	security.insert("eap".into(), json!("leap"));
	let doc = document(json!({ "attachments": [wireless("eduroam", Json::Object(security))] }));
	assert_eq!(
		invalid_at(&doc, &hardware()),
		"$['attachments'][0]['security']['eap']"
	);

	let pwd = document(json!({ "attachments": [wireless("eduroam", json!({
		"kind": "enterprise", "eap": "pwd", "identity": "nurse", "password": "hunter2"
	}))] }));
	let out = rendered(&pwd, &hardware(), &active(&[]));
	assert!(
		file(&out, "iwd/eduroam.8021x")
			.contents
			.contains("EAP-Method=PWD\n")
	);
}

/// Every network is left for bliti to join, and a hidden one is scanned for by name.
#[test]
fn a_hidden_network() {
	let doc = document(json!({ "attachments": [
		{ "kind": "wireless", "label": "back office", "ssid": "Office", "hidden": true,
		  "security": { "kind": "psk", "passphrase": "a long passphrase" } },
		wireless("Front", json!({ "kind": "psk", "passphrase": "another passphrase" }))
	] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	let hidden = &file(&out, "iwd/Office.psk").contents;
	assert!(hidden.contains("[Settings]\nAutoConnect=false\nHidden=true\n"));
	assert!(hidden.contains("[Security]\nPassphrase=a long passphrase\n"));
	let shown = &file(&out, "iwd/Front.psk").contents;
	assert!(shown.contains("AutoConnect=false\nHidden=false\n"));
}

/// iwd's file names: plain where it can be, hex where it cannot.
#[test]
fn ssid_file_names() {
	let doc = document(json!({ "attachments": [
		wireless("Clinic WiFi_5-G", json!({ "kind": "psk", "passphrase": "a long passphrase" })),
		wireless("Café/2", json!({ "kind": "psk", "passphrase": "a long passphrase" }))
	] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	file(&out, "iwd/Clinic WiFi_5-G.psk");
	file(&out, "iwd/=436166c3a92f32.psk");
}

/// Two candidates that would share an iwd file are refused at the later one.
#[test]
fn two_key_candidates_for_one_ssid_are_refused() {
	let doc = document(json!({ "attachments": [
		wireless("Clinic", json!({ "kind": "psk", "passphrase": "a long passphrase" })),
		wireless("Clinic", json!({ "kind": "sae", "passphrase": "a long passphrase" }))
	] }));
	assert_eq!(invalid_at(&doc, &hardware()), "$['attachments'][1]['ssid']");
}

/// `sae` holds iwd to WPA3 on an access point that also offers WPA2; `psk-sae` does not.
#[test]
fn sae_disables_the_transition() {
	let doc = document(json!({ "attachments": [
		wireless("Strict", json!({ "kind": "sae", "passphrase": "a long passphrase" })),
		wireless("Either", json!({ "kind": "psk-sae", "passphrase": "a long passphrase" }))
	] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	assert!(
		file(&out, "iwd/Strict.psk")
			.contents
			.contains("TransitionDisable=true\nDisabledTransitionModes=personal\n")
	);
	assert!(
		!file(&out, "iwd/Either.psk")
			.contents
			.contains("TransitionDisable")
	);

	let short = document(json!({ "attachments": [
		wireless("Strict", json!({ "kind": "sae", "passphrase": "short" }))
	] }));
	assert_eq!(
		invalid_at(&short, &hardware()),
		"$['attachments'][0]['security']['passphrase']"
	);
}

/// An unset hotspot isolates its clients, shares the device's network and hands out the default range.
#[test]
fn hotspot_defaults() {
	let doc = document(json!({
		"attachments": [],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let out = rendered(&doc, &hardware(), &active(&[]));
	let network = &file(&out, "network/50-bliti-ap0.network").contents;
	assert!(network.contains("Address=10.41.0.1/24\nDHCPServer=yes\nIPMasquerade=ipv4\n"));
	assert!(network.contains("IPv4Forwarding=yes\n"));
	assert!(network.contains("PoolOffset=2\nPoolSize=253\nEmitRouter=yes\nEmitDNS=yes\n"));

	let hostapd = file(&out, "hostapd.conf");
	assert_eq!(hostapd.mode, SECRET);
	for line in [
		"interface=ap0\n",
		&format!("ssid2={}\n", hex::encode("bliti-setup")),
		"hw_mode=g\nchannel=6\n",
		"wpa_key_mgmt=WPA-PSK SAE\n",
		"ieee80211w=1\n",
		"wpa_passphrase=read this aloud\n",
		"ap_isolate=1\n",
		"wps_state=0\n",
	] {
		assert!(
			hostapd.contents.contains(line),
			"{line:?} in {}",
			hostapd.contents
		);
	}
	assert!(!hostapd.contents.contains("country_code"));
}

/// Every hotspot switch the document sets is carried through.
#[test]
fn hotspot_overrides() {
	let doc = document(json!({
		"attachments": [],
		"hotspot": {
			"ssid": "bliti-setup", "passphrase": "read this aloud",
			"share-upstream": false, "isolate-clients": false, "dhcp-range": "172.30.5.0/24",
			"band": "5ghz", "channel": 44, "channel-width": 80
		},
		"regulatory-domain": "NZ"
	}));
	let out = rendered(&doc, &hardware(), &active(&[]));
	let network = &file(&out, "network/50-bliti-ap0.network").contents;
	assert!(network.contains("Address=172.30.5.1/24\n"));
	assert!(network.contains("IPMasquerade=no\nIPv4Forwarding=no\n"));
	assert!(network.contains("EmitRouter=no\nEmitDNS=no\n"));

	let hostapd = &file(&out, "hostapd.conf").contents;
	for line in [
		"country_code=NZ\nieee80211d=1\n",
		"hw_mode=a\nchannel=44\n",
		"ieee80211ac=1\n",
		"vht_oper_centr_freq_seg0_idx=42\n",
		"ap_isolate=0\n",
	] {
		assert!(hostapd.contains(line), "{line:?} in {hostapd}");
	}

	let bad = document(json!({
		"attachments": [],
		"hotspot": { "ssid": "s", "passphrase": "read this aloud", "dhcp-range": "10.41.0.9/24" }
	}));
	assert_eq!(invalid_at(&bad, &hardware()), "$['hotspot']['dhcp-range']");
}

/// On a shared-channel radio the hotspot follows the station and takes no channel of its own.
#[test]
fn shared_channel_hardware_follows_the_station() {
	let shared = Hardware {
		shared_channel: true,
		..hardware()
	};
	let doc = document(json!({
		"attachments": [],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let following = Selection {
		active: vec![],
		station_channel: Some(Channel {
			band: Band::Five,
			number: 149,
		}),
	};
	let out = rendered(&doc, &shared, &following);
	let hostapd = &file(&out, "hostapd.conf").contents;
	assert!(hostapd.contains("hw_mode=a\nchannel=149\n"));
	assert!(!hostapd.contains("ht_capab"));

	let alone = rendered(&doc, &shared, &active(&[]));
	assert!(
		file(&alone, "hostapd.conf")
			.contents
			.contains("channel=6\n")
	);

	for member in ["band", "channel", "channel-width"] {
		let value = if member == "band" {
			json!("2.4ghz")
		} else {
			json!(1)
		};
		let mut hotspot = json!({ "ssid": "bliti-setup", "passphrase": "read this aloud" });
		hotspot[member] = value;
		let doc = document(json!({ "attachments": [], "hotspot": hotspot }));
		assert_eq!(
			invalid_at(&doc, &shared),
			format!("$['hotspot']['{member}']")
		);
	}
}

/// Every file carrying a secret is readable by root alone, and the rest by anyone.
#[test]
fn secrets_are_0600() {
	let doc = document(json!({
		"attachments": [
			wireless("Clinic", json!({ "kind": "psk", "passphrase": "a long passphrase" })),
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0" }
		],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let out = rendered(&doc, &hardware(), &active(&[0, 1]));
	for file in &out.files {
		let secret = file.contents.contains("Passphrase=a long")
			|| file.contents.contains("wpa_passphrase=");
		let expected = if secret { SECRET } else { PUBLIC };
		assert_eq!(file.mode, expected, "{:?}", file.path);
	}
	assert_eq!(file(&out, "iwd/Clinic.psk").mode, SECRET);
	assert_eq!(file(&out, "hostapd.conf").mode, SECRET);
}

/// iwd runs no SAE on the drivers whose SAE fails, whatever the document.
#[test]
fn sae_is_disabled_on_brcmfmac() {
	let doc = document(json!({ "attachments": [] }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	assert!(
		file(&out, "iwd.conf")
			.contents
			.ends_with("\n[DriverQuirks]\nSaeDisable=brcmfmac\n")
	);
}

/// The regulatory domain reaches iwd, hostapd and cfg80211; unset, cfg80211 stays in the world domain.
#[test]
fn regulatory_domain() {
	let doc = document(json!({ "attachments": [], "regulatory-domain": "NZ" }));
	let out = rendered(&doc, &hardware(), &active(&[]));
	assert!(
		file(&out, "iwd.conf")
			.contents
			.contains("[General]\nEnableNetworkConfiguration=false\nCountry=NZ\n")
	);
	assert!(
		file(&out, "regdom.conf")
			.contents
			.ends_with("options cfg80211 ieee80211_regdom=NZ\n")
	);

	let unset = document(json!({ "attachments": [] }));
	let out = rendered(&unset, &hardware(), &active(&[]));
	assert!(!file(&out, "iwd.conf").contents.contains("Country"));
	assert!(
		file(&out, "regdom.conf")
			.contents
			.ends_with("ieee80211_regdom=00\n")
	);

	let lower = document(json!({ "attachments": [], "regulatory-domain": "nz" }));
	assert_eq!(invalid_at(&lower, &hardware()), "$['regulatory-domain']");
}

/// A candidate or hotspot the hardware has nowhere to put is refused.
#[test]
fn what_the_hardware_lacks_is_refused() {
	let wired_only = Hardware {
		station: None,
		access_point: None,
		..hardware()
	};
	let wifi = document(json!({ "attachments": [
		wireless("Clinic", json!({ "kind": "psk", "passphrase": "a long passphrase" }))
	] }));
	assert_eq!(invalid_at(&wifi, &wired_only), "$['attachments'][0]");

	let hotspot = document(json!({
		"attachments": [],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	assert_eq!(invalid_at(&hotspot, &wired_only), "$['hotspot']");

	let port = document(json!({ "attachments": [
		{ "kind": "wired-dynamic", "label": "wall", "interface": "eth7" }
	] }));
	assert_eq!(
		invalid_at(&port, &hardware()),
		"$['attachments'][0]['interface']"
	);

	let out = rendered(
		&document(json!({ "attachments": [] })),
		&wired_only,
		&active(&[]),
	);
	assert!(out.files.is_empty());
}

/// An applier can tell every rendered file as bliti's, and leaves everything else alone.
#[test]
fn rendered_files_are_owned() {
	let hardware = hardware();
	let doc = document(json!({
		"attachments": [
			wireless("Clinic", json!({ "kind": "psk", "passphrase": "a long passphrase" })),
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth0" }
		],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud" }
	}));
	let out = rendered(&doc, &hardware, &active(&[0, 1]));
	for file in &out.files {
		assert!(hardware.paths.owns(&file.path), "{:?}", file.path);
	}
	for foreign in [
		"/tmp/bliti-test/network/10-eth0.network",
		"/tmp/bliti-test/network/50-bliti-eth0.link",
		"/tmp/bliti-test/iwd/.known_network.freq",
		"/tmp/bliti-test/elsewhere/Clinic.psk",
	] {
		assert!(!hardware.paths.owns(Path::new(foreign)), "{foreign}");
	}
	assert!(Paths::system().owns(Path::new("/run/bliti/iwd/Old.psk")));
	assert!(!Paths::system().owns(Path::new("/var/lib/iwd/Old.psk")));
}

/// Rendering the same state twice gives the same files.
#[test]
fn rendering_is_deterministic() {
	let doc = document(json!({
		"attachments": [
			{ "kind": "wired-dynamic", "label": "wall", "interface": "eth1" },
			wireless("Clinic", json!({ "kind": "psk", "passphrase": "a long passphrase" })),
			{ "kind": "wired-dynamic", "label": "spare", "interface": "eth0" }
		]
	}));
	let a = rendered(&doc, &hardware(), &active(&[2, 0, 1]));
	let b = rendered(&doc, &hardware(), &active(&[0, 1, 2]));
	assert_eq!(a, b);
}

/// A wireless candidate or hotspot may name the one radio there is, and naming another is refused
/// rather than carried on the wrong adapter (LINK, HOT).
#[test]
fn a_radio_other_than_the_station_is_refused() {
	let pinned = document(json!({
		"attachments": [{ "kind": "wireless", "label": "w", "ssid": "Clinic", "interface": "wlan0",
			"security": { "kind": "sae", "passphrase": "a good long passphrase" } }],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud", "interface": "wlan0" }
	}));
	assert!(render(&pinned, &hardware(), &active(&[0])).is_ok());

	let elsewhere = document(json!({
		"attachments": [{ "kind": "wireless", "label": "w", "ssid": "Clinic", "interface": "wlan1",
			"security": { "kind": "sae", "passphrase": "a good long passphrase" } }]
	}));
	let Err(Error::Invalid(invalid)) = render(&elsewhere, &hardware(), &active(&[])) else {
		panic!("a candidate on another radio is refused")
	};
	assert_eq!(invalid.at, "$['attachments'][0]['interface']");

	let hotspot = document(json!({
		"attachments": [],
		"hotspot": { "ssid": "bliti-setup", "passphrase": "read this aloud", "interface": "wlan1" }
	}));
	let Err(Error::Invalid(invalid)) = render(&hotspot, &hardware(), &active(&[])) else {
		panic!("a hotspot on another radio is refused")
	};
	assert_eq!(invalid.at, "$['hotspot']['interface']");
}
