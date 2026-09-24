//! 802.1X credentials, from the members an enterprise candidate carries to iwd's `[Security]` keys.
//!
//! The vocabulary is bliti's own, pending its place in WLAN:
//!
//! | member | methods | meaning |
//! | --- | --- | --- |
//! | `eap` | all | `peap`, `ttls`, `tls` or `pwd` |
//! | `identity` | all | who authenticates; the inner identity under `peap` and `ttls` |
//! | `anonymous-identity` | `peap`, `ttls` | the outer identity, where it differs |
//! | `password` | `peap`, `ttls`, `pwd` | the password |
//! | `phase2` | `peap`, `ttls` | the inner method |
//! | `ca-certificate` | `peap`, `ttls`, `tls` | the authentication server's CA, as PEM |
//! | `domain` | `peap`, `ttls`, `tls` | the name the server's certificate is matched against |
//! | `client-certificate` | `tls` | the device's certificate, as PEM |
//! | `client-key` | `tls` | the device's private key, as PEM |
//! | `client-key-passphrase` | `tls` | what decrypts the key, where it is encrypted |
//!
//! WLAN has the device join only a network that authenticates its access point. The certificate
//! methods do that by the server's certificate, so they require both `ca-certificate` and `domain`;
//! `pwd` does it by the password itself.

use std::fmt::Write as _;

use bliti_core::channel::config::{Invalid, Segment};
use serde_json::{Map, Value as Json};

use super::escape;
use crate::network::render::invalid_in;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Method {
	Peap,
	Ttls,
	Tls,
	Pwd,
}

impl Method {
	fn iwd(self) -> &'static str {
		match self {
			Self::Peap => "PEAP",
			Self::Ttls => "TTLS",
			Self::Tls => "TLS",
			Self::Pwd => "PWD",
		}
	}

	/// Whether `member` is one this method uses.
	fn uses(self, member: &str) -> bool {
		match member {
			"kind" | "eap" | "identity" => true,
			"anonymous-identity" | "phase2" => matches!(self, Self::Peap | Self::Ttls),
			"password" => matches!(self, Self::Peap | Self::Ttls | Self::Pwd),
			"ca-certificate" | "domain" => matches!(self, Self::Peap | Self::Ttls | Self::Tls),
			"client-certificate" | "client-key" | "client-key-passphrase" => self == Self::Tls,
			_ => false,
		}
	}
}

/// The members of an enterprise candidate, each looked up by name and faulted at its own path.
struct Members<'a> {
	members: &'a Map<String, Json>,
	rank: usize,
}

impl<'a> Members<'a> {
	fn invalid(&self, member: &str, reason: impl Into<String>) -> Invalid {
		invalid_in(
			self.rank,
			&[Segment::Name("security"), Segment::Name(member)],
			reason,
		)
	}

	fn optional(&self, member: &str) -> Result<Option<&'a str>, Invalid> {
		match self.members.get(member) {
			None | Some(Json::Null) => Ok(None),
			Some(Json::String(value)) if !value.is_empty() => Ok(Some(value)),
			Some(_) => Err(self.invalid(member, format!("`{member}` is a non-empty string"))),
		}
	}

	fn required(&self, member: &str, why: &str) -> Result<&'a str, Invalid> {
		self.optional(member)?
			.ok_or_else(|| self.invalid(member, format!("`{member}` is required {why}")))
	}

	fn pem(&self, member: &str, why: &str) -> Result<String, Invalid> {
		let value = self.required(member, why)?.replace('\r', "");
		let value = value.trim();
		let well_formed = value.starts_with("-----BEGIN ")
			&& value.ends_with("-----")
			&& value.contains("-----END ")
			&& !value.lines().any(|line| line.trim_start().starts_with('['));
		if well_formed {
			Ok(format!("{value}\n"))
		} else {
			Err(self.invalid(member, format!("`{member}` is a PEM block")))
		}
	}
}

/// The `[Security]` group of an enterprise candidate's `.8021x`, with any certificate or key
/// embedded after it.
pub(super) fn security(members: &Map<String, Json>, rank: usize) -> Result<String, Invalid> {
	let members = Members { members, rank };
	let method = match members.required("eap", "to choose the EAP method")? {
		"peap" => Method::Peap,
		"ttls" => Method::Ttls,
		"tls" => Method::Tls,
		"pwd" => Method::Pwd,
		other => {
			return Err(members.invalid(
				"eap",
				format!("{other:?} is not one of `peap`, `ttls`, `tls` or `pwd`"),
			));
		}
	};
	if let Some(member) = members.members.keys().find(|member| !method.uses(member)) {
		return Err(members.invalid(
			member,
			format!(
				"`{member}` is not used by `{}`",
				method.iwd().to_lowercase()
			),
		));
	}

	let name = method.iwd();
	let identity = members.required("identity", "to authenticate")?;
	let mut out = format!("[Security]\nEAP-Method={name}\n");
	let mut embedded = String::new();

	if method == Method::Pwd {
		let password = members.required("password", "by `pwd`")?;
		let _ = write!(
			out,
			"EAP-Identity={}\nEAP-Password={}\n",
			escape(identity),
			escape(password)
		);
		return Ok(out);
	}

	let why = "to authenticate the access point";
	let ca = members.pem("ca-certificate", why)?;
	let domain = members.required("domain", why)?;
	if !domain
		.bytes()
		.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'*'))
	{
		return Err(members.invalid("domain", format!("{domain:?} is not a server name")));
	}

	let outer = match method {
		Method::Tls => identity,
		_ => members.optional("anonymous-identity")?.unwrap_or(identity),
	};
	let _ = write!(
		out,
		"EAP-Identity={}\nEAP-{name}-CACert=embed:ca\nEAP-{name}-ServerDomainMask={domain}\n",
		escape(outer)
	);
	let _ = write!(embedded, "\n[@pem@ca]\n{ca}");

	if method == Method::Tls {
		let why = "by `tls`";
		let certificate = members.pem("client-certificate", why)?;
		let key = members.pem("client-key", why)?;
		let _ = write!(
			out,
			"EAP-TLS-ClientCert=embed:client-certificate\nEAP-TLS-ClientKey=embed:client-key\n"
		);
		if let Some(passphrase) = members.optional("client-key-passphrase")? {
			let _ = writeln!(out, "EAP-TLS-ClientKeyPassphrase={}", escape(passphrase));
		}
		let _ = write!(
			embedded,
			"\n[@pem@client-certificate]\n{certificate}\n[@pem@client-key]\n{key}"
		);
	} else {
		let phase2 = members.required("phase2", "to choose the inner method")?;
		let inner = match (method, phase2) {
			(Method::Peap, "mschapv2") => "MSCHAPV2",
			(Method::Peap, "gtc") => "GTC",
			(Method::Peap, "md5") => "MD5",
			(Method::Ttls, "mschapv2") => "Tunneled-MSCHAPv2",
			(Method::Ttls, "mschap") => "Tunneled-MSCHAP",
			(Method::Ttls, "chap") => "Tunneled-CHAP",
			(Method::Ttls, "pap") => "Tunneled-PAP",
			_ => {
				return Err(members.invalid(
					"phase2",
					format!(
						"{phase2:?} is not an inner method of `{}`",
						name.to_lowercase()
					),
				));
			}
		};
		let password = members.required("password", "by the inner method")?;
		let _ = write!(
			out,
			"EAP-{name}-Phase2-Method={inner}\nEAP-{name}-Phase2-Identity={}\n\
			 EAP-{name}-Phase2-Password={}\n",
			escape(identity),
			escape(password)
		);
	}

	out.push_str(&embedded);
	Ok(out)
}
