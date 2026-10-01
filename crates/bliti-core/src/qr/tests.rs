use super::*;
use crate::testing::scan_svg;

fn sample() -> QrPayload {
	let mut token = [0u8; PRESENCE_TOKEN_LEN];
	for (i, b) in token.iter_mut().enumerate() {
		*b = i as u8;
	}
	let mut fingerprint = [0u8; KEY_FINGERPRINT_LEN];
	for (i, b) in fingerprint.iter_mut().enumerate() {
		*b = 0x80 | i as u8;
	}
	QrPayload::new(
		PresenceToken::from_bytes(token),
		KeyFingerprint::from_bytes(fingerprint),
	)
}

/// Payloads drawn from a fixed stream, so a failure names one that can be reproduced.
fn payloads() -> impl Iterator<Item = QrPayload> {
	let mut stream = blake3::Hasher::new_derive_key("bliti qr test payloads").finalize_xof();
	let drawn = (0..200).map(move |_| {
		let mut token = [0u8; PRESENCE_TOKEN_LEN];
		let mut fingerprint = [0u8; KEY_FINGERPRINT_LEN];
		stream.fill(&mut token);
		stream.fill(&mut fingerprint);
		QrPayload::new(
			PresenceToken::from_bytes(token),
			KeyFingerprint::from_bytes(fingerprint),
		)
	});
	// All ones encodes as base32's digits wherever the bytes allow, which is where an optimiser
	// would be tempted to split the text into segments.
	let digits = QrPayload::new(
		PresenceToken::from_bytes([0xff; PRESENCE_TOKEN_LEN]),
		KeyFingerprint::from_bytes([0xff; KEY_FINGERPRINT_LEN]),
	);
	[sample(), digits].into_iter().chain(drawn)
}

#[test]
fn payload_is_version_then_token_then_fingerprint_and_nothing_else() {
	let payload = sample();
	let bytes = payload.to_bytes();
	// The board ID is not carried: the payload is exactly these 35 bytes.
	assert_eq!(bytes.len(), 35);
	assert_eq!(bytes[0], PAYLOAD_VERSION);
	assert_eq!(&bytes[1..17], payload.presence_token().as_bytes());
	assert_eq!(&bytes[17..], payload.fingerprint().as_bytes());
	assert_eq!(QrPayload::from_bytes(&bytes).unwrap(), payload);
}

#[test]
fn the_text_is_the_prefix_then_56_characters_without_padding() {
	let payload = sample();
	let text = payload.to_text();
	let encoded = text.strip_prefix("BLITI:").unwrap();
	assert_eq!(encoded, payload.encoded());
	assert_eq!(encoded.len(), 56);
	assert!(
		encoded
			.chars()
			.all(|c| c.is_ascii_uppercase() || ('2'..='7').contains(&c))
	);
}

#[test]
fn text_known_answer() {
	// Pins the field order and the encoding end to end.
	assert_eq!(
		sample().to_text(),
		"BLITI:AEAACAQDAQCQMBYIBEFAWDANBYHYBAMCQOCILBUHRCEYVC4MRWHI7EER"
	);
}

#[test]
fn the_suffix_lies_within_the_fingerprint() {
	let payload = sample();
	let encoded = payload.encoded();
	assert_eq!(payload.suffix(), &encoded[52..]);
	// Whatever the token, the suffix is the same.
	for payload in payloads().take(20) {
		let mut bytes = payload.to_bytes();
		bytes[1..17].fill(0);
		assert_eq!(
			QrPayload::from_bytes(&bytes).unwrap().suffix(),
			payload.suffix()
		);
	}
}

#[test]
fn every_code_is_version_5_at_level_h() {
	for payload in payloads() {
		let code = payload.to_qr_code();
		assert_eq!(code.version(), Version::Normal(5), "{}", payload.to_text());
		assert_eq!(code.error_correction_level(), EcLevel::H);
	}
}

#[test]
fn the_svg_scans_to_the_text() {
	for payload in payloads().take(10) {
		let svg = payload.to_svg();
		assert!(svg.contains("<svg"));
		let scanned = scan_svg(&svg);
		assert_eq!(scanned, payload.to_text());
		assert_eq!(QrPayload::read(&scanned).unwrap(), payload);
	}
}

#[test]
fn the_svg_carries_the_code_alone_with_no_physical_size() {
	let payload = sample();
	let svg = payload.to_svg();
	assert!(!svg.contains("<text"));
	assert!(!svg.contains(&payload.encoded()));
	// A whole number of modules a side: the code's 37 and four of quiet zone either side.
	let side = |name: &str| -> usize {
		let start = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
		svg[start..start + svg[start..].find('"').unwrap()]
			.parse()
			.unwrap()
	};
	assert_eq!(side("width"), side("height"));
	assert_eq!(side("width") % (37 + 2 * 4), 0);
	for unit in ["mm\"", "cm\"", "in\"", "pt\"", "pc\""] {
		assert!(!svg.contains(unit), "{unit} in {svg}");
	}
}

#[test]
fn a_payload_produces_the_same_svg_every_time() {
	// A reprint is byte-identical with no record consulted, wherever it is produced.
	assert_eq!(sample().to_svg(), sample().to_svg());
}

#[test]
fn read_takes_the_text_however_it_arrives() {
	let payload = sample();
	let encoded = payload.encoded();
	let grouped = encoded
		.as_bytes()
		.chunks(4)
		.map(|chunk| std::str::from_utf8(chunk).unwrap())
		.collect::<Vec<_>>()
		.join("-");
	for form in [
		payload.to_text(),
		payload.to_text().to_lowercase(),
		encoded.clone(),
		encoded.to_lowercase(),
		format!("bliti:{encoded}"),
		format!("ANYTHING:{encoded}"),
		format!("  {}  ", payload.to_text()),
		grouped.clone(),
		format!("{PREFIX} {}", grouped.replace('-', " ")),
		format!("\t{encoded}\n"),
	] {
		assert_eq!(QrPayload::read(&form), Ok(payload.clone()), "{form:?}");
	}
}

#[test]
fn read_discards_only_up_to_the_last_colon() {
	let payload = sample();
	assert_eq!(
		QrPayload::read(&format!("bliti:v1:{}", payload.encoded())),
		Ok(payload.clone())
	);
	// A colon inside the payload's place means what follows it is all the payload there is.
	assert_eq!(
		QrPayload::read(&format!("{}:", payload.to_text())),
		Err(QrError::Malformed)
	);
}

#[test]
fn read_takes_the_digits_base32_never_uses_as_the_letters_they_resemble() {
	let payload = sample();
	let encoded = payload.encoded();
	let mistaken = encoded
		.replace('O', "0")
		.replace('I', "1")
		.replace('B', "8");
	assert_ne!(mistaken, encoded);
	assert_eq!(QrPayload::read(&mistaken), Ok(payload));
}

#[test]
fn the_payload_version_is_read_before_the_length() {
	// Whatever a later payload version lays out after its first byte, this build can say it is a
	// version it does not read rather than that it is not a code at all.
	for len in [1, 2, 20, 35, 49, 65] {
		let mut bytes = vec![0x5a; len];
		bytes[0] = 2;
		let text = format!("{PREFIX}{}", BASE32_NOPAD.encode(&bytes));
		assert_eq!(
			QrPayload::read(&text),
			Err(QrError::UnsupportedVersion(2)),
			"{len} bytes"
		);
	}
}

#[test]
fn text_that_is_not_a_payload_is_malformed() {
	for text in [
		"",
		"BLITI:",
		"not a code!",
		"https://bliti.tamanu.app/",
		"WIFI:S:site;T:WPA;P:passphrase;;",
	] {
		assert_eq!(QrPayload::read(text), Err(QrError::Malformed), "{text:?}");
	}
	// A payload at the version this build reads, of the wrong length, is malformed.
	let short = BASE32_NOPAD.encode(&[PAYLOAD_VERSION, 0, 0]);
	assert_eq!(QrPayload::read(&short), Err(QrError::Malformed));
	let mut long = sample().to_bytes();
	long.push(0);
	assert_eq!(QrPayload::from_bytes(&long), Err(QrError::Malformed));
	let token_only = &sample().to_bytes()[..1 + PRESENCE_TOKEN_LEN];
	assert_eq!(QrPayload::from_bytes(token_only), Err(QrError::Malformed));
}
