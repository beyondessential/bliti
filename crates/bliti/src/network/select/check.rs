//! The rules of LINK and HOT that turn on the hardware a document is selected on.

use bliti_core::channel::config::{AttachmentKind, Document, Invalid, Segment, path};

use super::Hardware;

fn invalid(at: &[Segment<'_>], reason: String) -> Invalid {
	Invalid {
		at: path(at),
		reason,
		reached: None,
	}
}

/// Refuse a document selection could not carry out on `hardware`: a candidate naming an interface
/// the device does not have, or a hotspot with no radio able to run it.
///
/// A hotspot and a wireless candidate that only a radio running one at a time could carry are the
/// capabilities' to refuse, as [`bliti_core::channel::capabilities::placement`] does.
pub fn check(document: &Document, hardware: &Hardware) -> Result<(), Invalid> {
	for (rank, attachment) in document.attachments.iter().enumerate() {
		let at = |member| [Segment::Name("attachments"), Segment::Index(rank), member];
		match &attachment.kind {
			AttachmentKind::WiredDynamic { interface }
			| AttachmentKind::WiredStatic { interface, .. } => {
				if !hardware.wired.contains(interface) {
					return Err(invalid(
						&at(Segment::Name("interface")),
						format!("{interface:?} is not a wired interface on this device"),
					));
				}
			}
			AttachmentKind::Wireless(wireless) => {
				if hardware.radios.is_empty() {
					return Err(invalid(
						&at(Segment::Name("kind")),
						"this device has no wireless interface".to_owned(),
					));
				}
				if let Some(interface) = &wireless.interface
					&& hardware.radio(interface).is_none()
				{
					return Err(invalid(
						&at(Segment::Name("interface")),
						format!("{interface:?} is not a wireless interface on this device"),
					));
				}
			}
		}
	}

	let Some(hotspot) = &document.hotspot else {
		return Ok(());
	};
	let at: &[Segment<'_>] = match hotspot.interface {
		Some(_) => &[Segment::Name("hotspot"), Segment::Name("interface")],
		None => &[Segment::Name("hotspot")],
	};
	if let Some(interface) = &hotspot.interface {
		match hardware.radio(interface) {
			None => {
				return Err(invalid(
					at,
					format!("{interface:?} is not a wireless interface on this device"),
				));
			}
			Some(radio) if radio.access_point.is_none() => {
				return Err(invalid(at, format!("{interface} cannot run a hotspot")));
			}
			Some(_) => {}
		}
	}
	if hardware.hotspot_radios(hotspot).next().is_none() {
		return Err(invalid(at, "this device cannot run a hotspot".to_owned()));
	}

	Ok(())
}
