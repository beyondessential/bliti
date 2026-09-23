//! Whether a configuration document stays within what a device said it supports.
//!
//! Behaviour is specified in NET. A device's `capabilities.document` mirrors the document: at each
//! member it carries nothing (not supported), `true` (any value), an array (exactly these values), or
//! an object (constrained member by member). A member whose value decides what its siblings may
//! carry is a selector, mirrored as an object keyed by its values, each key holding the constraints
//! its siblings are held to under that value. The selectors are the ones NET names: `kind`,
//! `interface` and `band`.
//!
//! One checker serves both ends. The device rejects with it, and the client checks what it is about
//! to propose with the same code through wasm, so the two cannot disagree about a document.
//!
//! Members the document requires of a kind are supported wherever the kind is, without being listed
//! (NET). Which those are is the document's own shape, so it is read from [`required`] rather than
//! from capabilities. What is structurally wrong with a document (a member of the wrong type, one
//! missing) is [`super::config::Document::parse`]'s to find; this answers only whether it asks for
//! more than the device offers.

use serde_json::{Map, Value as Json};

use super::config::{Invalid, Segment, path};

/// The members whose value decides what their siblings may carry (NET).
const SELECTORS: &[&str] = &["kind", "interface", "band"];

/// Check `document` against a device's `capabilities.document`, naming in `at` the first member
/// capabilities do not cover.
pub fn check(
	document: &Map<String, Json>,
	capabilities: &Map<String, Json>,
) -> Result<(), Invalid> {
	let mut at = Vec::new();
	object(document, capabilities, Context::Document, &mut at).map_err(|fault| fault.invalid)
}

/// A document asking for more than the device offers, and how far the walk got before finding it.
///
/// Where a selector is left unset every way of resolving it is tried, and the fault worth reporting
/// is the one closest to being admitted: the deepest, and at equal depth one found among an object's
/// members, which means its selectors were admitted, over one found at a selector.
struct Fault {
	invalid: Invalid,
	progress: usize,
}

/// Where in the document an object sits, which decides what it requires of itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Context {
	Document,
	Attachment,
	Security,
	Hotspot,
	Other,
}

impl Context {
	/// The context of the object a member of this one holds.
	fn child(self, member: &str) -> Self {
		match (self, member) {
			(Self::Document, "attachments") => Self::Attachment,
			(Self::Document, "hotspot") => Self::Hotspot,
			(Self::Attachment, "security") => Self::Security,
			_ => Self::Other,
		}
	}
}

/// The members a document requires of an object, supported wherever the object is (NET).
///
/// `kind` names what the object is, which is what `kind` under capabilities is keyed by. An
/// enterprise security object carries the members its method uses, which WLAN gives and
/// capabilities do not list, so everything but `eap` passes here.
fn required(context: Context, kind: Option<&str>, member: &str) -> bool {
	match (context, kind) {
		(Context::Document, _) => member == "attachments",
		(Context::Attachment, Some("wireless")) => matches!(member, "label" | "ssid" | "security"),
		(Context::Attachment, Some("wired-dynamic")) => matches!(member, "label" | "interface"),
		(Context::Attachment, Some("wired-static")) => {
			matches!(member, "label" | "interface" | "addresses" | "gateway")
		}
		(Context::Security, Some("psk" | "sae" | "psk-sae")) => member == "passphrase",
		(Context::Security, Some("enterprise")) => member != "eap",
		(Context::Hotspot, _) => matches!(member, "ssid" | "passphrase"),
		_ => false,
	}
}

/// Check one object against the constraints on it, resolving its selectors first.
fn object(
	document: &Map<String, Json>,
	capabilities: &Map<String, Json>,
	context: Context,
	at: &mut Vec<Owned>,
) -> Result<(), Fault> {
	let kind = document.get("kind").and_then(Json::as_str);
	let resolutions = resolve(document, capabilities.clone(), Vec::new(), at)?;

	// A selector left unset is within capabilities where any of its keys admits the rest (NET), so
	// the object passes if any resolution does. Otherwise the fault reported is the one the walk got
	// furthest before meeting, which is the resolution closest to being admitted.
	let mut deepest: Option<Fault> = None;
	for resolution in resolutions {
		match members(document, &resolution, context, kind, at) {
			Ok(()) => return Ok(()),
			Err(fault) => {
				if deepest
					.as_ref()
					.is_none_or(|best| fault.progress > best.progress)
				{
					deepest = Some(fault);
				}
			}
		}
	}
	Err(deepest.expect("resolution yields at least one set of constraints or fails"))
}

/// The constraints an object is held to once one way of resolving its selectors is taken, and which
/// selectors that consumed.
struct Resolution {
	constraints: Map<String, Json>,
	consumed: Vec<&'static str>,
}

/// Resolve an object's selectors against its own values: one resolution where every selector is set,
/// one per key where a selector is left unset.
fn resolve(
	document: &Map<String, Json>,
	mut constraints: Map<String, Json>,
	consumed: Vec<&'static str>,
	at: &mut Vec<Owned>,
) -> Result<Vec<Resolution>, Fault> {
	let selector = SELECTORS.iter().copied().find(|name| {
		!consumed.contains(name) && matches!(constraints.get(*name), Some(Json::Object(_)))
	});
	let Some(selector) = selector else {
		return Ok(vec![Resolution {
			constraints,
			consumed,
		}]);
	};
	let Some(Json::Object(keyed)) = constraints.remove(selector) else {
		unreachable!("the selector was found holding an object")
	};
	let mut consumed = consumed;
	consumed.push(selector);

	match document.get(selector) {
		None | Some(Json::Null) => {
			let mut all = Vec::new();
			for under in keyed.values() {
				if let Json::Object(under) = under {
					all.extend(resolve(
						document,
						merged(&constraints, under),
						consumed.clone(),
						at,
					)?);
				}
			}
			if all.is_empty() {
				return Err(outside_at(
					at,
					selector,
					format!("no `{selector}` is supported"),
				));
			}
			Ok(all)
		}
		Some(value) => match keyed.get(&key_of(value)) {
			Some(Json::Object(under)) => {
				resolve(document, merged(&constraints, under), consumed, at)
			}
			_ => Err(outside_at(
				at,
				selector,
				format!("{value} is not a supported `{selector}`"),
			)),
		},
	}
}

/// The constraints of an object with a selected key's constraints laid over them.
fn merged(base: &Map<String, Json>, under: &Map<String, Json>) -> Map<String, Json> {
	let mut out = base.clone();
	for (name, value) in under {
		out.insert(name.clone(), value.clone());
	}
	out
}

/// A document value as the key capabilities index it by.
fn key_of(value: &Json) -> String {
	match value {
		Json::String(text) => text.clone(),
		other => other.to_string(),
	}
}

/// Check each member an object carries against what one resolution of its constraints says of it.
fn members(
	document: &Map<String, Json>,
	resolution: &Resolution,
	context: Context,
	kind: Option<&str>,
	at: &mut Vec<Owned>,
) -> Result<(), Fault> {
	for (name, value) in document {
		if value.is_null() || resolution.consumed.contains(&name.as_str()) {
			continue;
		}
		at.push(Owned::Name(name.clone()));
		let outcome = match resolution.constraints.get(name) {
			// A required member holding objects still has those objects' own members checked.
			None if required(context, kind, name) => {
				if holds_objects(value) {
					value_against(value, &Json::Object(Map::new()), context.child(name), at)
				} else {
					Ok(())
				}
			}
			None => Err(outside(at, format!("`{name}` is not supported"))),
			Some(constraint) => value_against(value, constraint, context.child(name), at),
		};
		at.pop();
		outcome?;
	}
	Ok(())
}

/// Whether a value is an object, or an array holding one.
fn holds_objects(value: &Json) -> bool {
	match value {
		Json::Object(_) => true,
		Json::Array(items) => items.iter().any(Json::is_object),
		_ => false,
	}
}

/// Check one value against the constraint capabilities carry for it.
fn value_against(
	value: &Json,
	constraint: &Json,
	context: Context,
	at: &mut Vec<Owned>,
) -> Result<(), Fault> {
	match (value, constraint) {
		(_, Json::Bool(true)) => Ok(()),
		(Json::Array(items), _) => {
			for (index, item) in items.iter().enumerate() {
				at.push(Owned::Index(index));
				let outcome = value_against(item, constraint, context, at);
				at.pop();
				outcome?;
			}
			Ok(())
		}
		(Json::Object(inner), Json::Object(constraints)) => object(inner, constraints, context, at),
		(_, Json::Array(allowed)) if allowed.contains(value) => Ok(()),
		_ => Err(outside(at, format!("{value} is not supported"))),
	}
}

/// A path segment the checker owns, so it can grow and shrink while it walks.
#[derive(Debug, Clone)]
enum Owned {
	Name(String),
	Index(usize),
}

/// A selector whose value capabilities do not key, found at a member of the node the walk reached.
fn outside_at(at: &mut Vec<Owned>, selector: &str, reason: impl Into<String>) -> Fault {
	at.push(Owned::Name(selector.to_owned()));
	let fault = Fault {
		invalid: invalid(at, reason),
		progress: at.len() * 2,
	};
	at.pop();
	fault
}

/// A member or value capabilities do not cover, at the node the walk has reached.
fn outside(at: &[Owned], reason: impl Into<String>) -> Fault {
	Fault {
		invalid: invalid(at, reason),
		progress: at.len() * 2 + 1,
	}
}

/// The `invalid` naming the node the walk has reached.
fn invalid(at: &[Owned], reason: impl Into<String>) -> Invalid {
	let segments: Vec<Segment<'_>> = at
		.iter()
		.map(|segment| match segment {
			Owned::Name(name) => Segment::Name(name),
			Owned::Index(index) => Segment::Index(*index),
		})
		.collect();
	Invalid {
		at: path(&segments),
		reason: reason.into(),
		reached: None,
	}
}

#[cfg(test)]
mod tests;
