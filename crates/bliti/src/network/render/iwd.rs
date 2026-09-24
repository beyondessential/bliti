//! iwd: one known-network file per wireless candidate, and the main configuration that leaves
//! addressing to networkd.

use std::fmt::Write as _;

use bliti_core::channel::config::{AttachmentKind, Document, Invalid, Security, Segment, Wireless};

use super::{File, Hardware, PUBLIC, Paths, SECRET, candidate_path, header, invalid_in};

mod enterprise;

/// The drivers iwd is told not to run SAE on, by name.
///
/// brcmfmac runs SAE by external authentication, and on the CYW43455 that fails against any access
/// point using hash-to-element, which WPA3 access points increasingly require. iwd joins a
/// transitional network over PSK on these drivers instead, and the probe offers no SAE on them.
pub(crate) const SAE_DISABLED: &[&str] = &["brcmfmac"];

/// The suffixes iwd gives its network files, all of which bliti owns in iwd's state directory.
const SUFFIXES: [&str; 3] = [".psk", ".8021x", ".open"];

pub(super) fn owns(name: &str) -> bool {
	!name.starts_with('.') && SUFFIXES.iter().any(|suffix| name.ends_with(suffix))
}

/// A network file per wireless candidate turned on, whether or not it is selected: iwd joins only
/// what bliti tells it to, so every candidate can be known to it at once. One turned off is checked
/// as any other and left out, so iwd does not know it and nothing joins it (LINK).
///
/// iwd holds one network per SSID and kind of key, for every radio at once, so candidates for one
/// SSID on different radios share a file. They may only where the file would say the same for each.
pub(super) fn networks(document: &Document, hardware: &Hardware) -> Result<Vec<File>, Invalid> {
	let mut files: Vec<(usize, &Wireless, File)> = Vec::new();
	for (rank, attachment) in document.attachments.iter().enumerate() {
		let AttachmentKind::Wireless(wireless) = &attachment.kind else {
			continue;
		};
		if hardware.radios.is_empty() {
			return Err(invalid_in(rank, &[], "this device has no wireless client"));
		}
		let ssid_at = [Segment::Name("ssid")];
		if wireless.ssid.is_empty() || wireless.ssid.len() > 32 {
			return Err(invalid_in(rank, &ssid_at, "an SSID is 1 to 32 bytes"));
		}

		let mut settings = format!(
			"[Settings]\nAutoConnect=false\nHidden={}\n",
			wireless.hidden.unwrap_or(false)
		);
		let (suffix, security) = match &wireless.security {
			Security::Psk { passphrase } | Security::PskSae { passphrase } => {
				(".psk", psk(passphrase, rank)?)
			}
			Security::Sae { passphrase } => {
				// Without this iwd falls back to WPA2 on an access point that offers both. iwd
				// ignores it on a radio lacking CCMP or BIP-CMAC, so `sae` is only offered where
				// the radio has both.
				settings.push_str("TransitionDisable=true\nDisabledTransitionModes=personal\n");
				(".psk", psk(passphrase, rank)?)
			}
			Security::Enterprise { members } => (".8021x", enterprise::security(members, rank)?),
		};

		if !attachment.enabled {
			continue;
		}
		let path = hardware
			.paths
			.iwd_state
			.join(format!("{}{suffix}", encode_ssid(&wireless.ssid)));
		if let Some((earlier, first, _)) = files.iter().find(|(_, _, file)| file.path == path) {
			let differs = if first.security != wireless.security {
				"security"
			} else if first.hidden.unwrap_or(false) != wireless.hidden.unwrap_or(false) {
				"hidden"
			} else {
				continue;
			};
			return Err(invalid_in(
				rank,
				&ssid_at,
				format!(
					"candidate {earlier} joins {:?} with a different {differs}, and iwd holds one \
					 such network per SSID, so every candidate for it has to carry the same",
					wireless.ssid
				),
			));
		}

		let contents = format!("{}\n{settings}\n{security}", header(&candidate_path(rank)));
		files.push((
			rank,
			wireless,
			File {
				path,
				contents,
				mode: SECRET,
			},
		));
	}
	Ok(files.into_iter().map(|(_, _, file)| file).collect())
}

/// iwd's main configuration: networkd addresses every link, so iwd configures none, and SAE stays
/// off the drivers in [`SAE_DISABLED`].
pub(super) fn main_conf(paths: &Paths, domain: Option<&str>) -> File {
	let mut contents = format!(
		"{}\n[General]\nEnableNetworkConfiguration=false\n",
		header("the document")
	);
	if let Some(domain) = domain {
		let _ = writeln!(contents, "Country={domain}");
	}
	let _ = write!(
		contents,
		"\n[DriverQuirks]\nSaeDisable={}\n",
		SAE_DISABLED.join(",")
	);
	File {
		path: paths.iwd_config.clone(),
		contents,
		mode: PUBLIC,
	}
}

fn psk(passphrase: &str, rank: usize) -> Result<String, Invalid> {
	if !(8..=63).contains(&passphrase.len())
		|| !passphrase.bytes().all(|b| (b' '..=b'~').contains(&b))
	{
		return Err(invalid_in(
			rank,
			&[Segment::Name("security"), Segment::Name("passphrase")],
			"a passphrase is 8 to 63 printable ASCII characters",
		));
	}
	Ok(format!("[Security]\nPassphrase={}\n", escape(passphrase)))
}

/// The name iwd gives an SSID's file: the SSID itself where it holds only alphanumerics, spaces,
/// underscores and hyphens, else `=` and the SSID's bytes in lower-case hex.
pub(super) fn encode_ssid(ssid: &str) -> String {
	if ssid
		.bytes()
		.all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b'_' | b'-'))
	{
		ssid.to_owned()
	} else {
		format!("={}", hex::encode(ssid))
	}
}

/// A value as ell's settings parser reads it back: backslashes, line breaks and leading blanks
/// escaped, everything else as it is.
fn escape(value: &str) -> String {
	let mut out = String::with_capacity(value.len());
	let mut leading = true;
	for c in value.chars() {
		match c {
			' ' if leading => out.push_str("\\s"),
			'\t' if leading => out.push_str("\\t"),
			'\n' => out.push_str("\\n"),
			'\r' => out.push_str("\\r"),
			'\\' => out.push_str("\\\\"),
			c => out.push(c),
		}
		if !matches!(c, ' ' | '\t') {
			leading = false;
		}
	}
	out
}

/// The passphrase in a pre-shared-key network file, as ell reads it back: iwd writes one there
/// when WPS hands it a passphrase rather than a raw key.
pub(super) fn passphrase(contents: &str) -> Option<String> {
	let mut section = "";
	for line in contents.lines() {
		let line = line.trim_end();
		if let Some(name) = line
			.strip_prefix('[')
			.and_then(|rest| rest.strip_suffix(']'))
		{
			section = name;
		} else if section == "Security"
			&& let Some(value) = line.strip_prefix("Passphrase=")
		{
			return Some(unescape(value));
		}
	}
	None
}

/// The inverse of [`escape`].
fn unescape(value: &str) -> String {
	let mut out = String::with_capacity(value.len());
	let mut chars = value.chars();
	while let Some(c) = chars.next() {
		if c != '\\' {
			out.push(c);
			continue;
		}
		match chars.next() {
			Some('s') => out.push(' '),
			Some('t') => out.push('\t'),
			Some('n') => out.push('\n'),
			Some('r') => out.push('\r'),
			Some(other) => out.push(other),
			None => out.push('\\'),
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	/// iwd.network(5): plain where the SSID is alphanumerics, space, `_` and `-`, else `=` and hex.
	#[test]
	fn ssids_encode_as_iwd_names_them() {
		assert_eq!(encode_ssid("Clinic WiFi_2-G"), "Clinic WiFi_2-G");
		assert_eq!(encode_ssid("Café"), "=436166c3a9");
		assert_eq!(encode_ssid("a.b"), "=612e62");
	}

	/// Only what ell would misread is escaped.
	#[test]
	fn values_escape_as_ell_reads_them() {
		assert_eq!(escape("  a b\\c\nd"), "\\s\\sa b\\\\c\\nd");
		assert_eq!(escape("plain"), "plain");
	}

	/// What iwd wrote after WPS reads back as the passphrase it holds.
	#[test]
	fn a_passphrase_reads_back_unescaped() {
		let file = "[Settings]\nAutoConnect=true\n\n[Security]\nPreSharedKey=00ff\nPassphrase=\\s\\sa b\\\\c\n";
		assert_eq!(passphrase(file).as_deref(), Some("  a b\\c"));
		assert_eq!(passphrase("[Security]\nPreSharedKey=00ff\n"), None);
		assert_eq!(passphrase(&escape("  x\\y")), None, "outside [Security]");
		assert_eq!(unescape(&escape(" \tw\\x\ny")), " \tw\\x\ny");
	}

	/// Network files are bliti's whatever their SSID; iwd's own hidden files are not.
	#[test]
	fn network_files_are_owned() {
		assert!(owns("Clinic.psk"));
		assert!(owns("=436166c3a9.8021x"));
		assert!(owns("Guest.open"));
		assert!(!owns(".known_network.freq"));
		assert!(!owns("main.conf"));
	}
}
