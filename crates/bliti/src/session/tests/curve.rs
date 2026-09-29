//! The curve stream over the channel (CRV).

use bliti_core::channel::messages::Span;
use serde_json::{Value as Json, json};

use super::*;
use crate::facts::curve::{Document, store::Store as CurveStore};

/// A valid document with a charging curve, unlike the shipped one.
fn learnt() -> Json {
	json!({
		"discharging": {
			"points": [[2.6, 0.0], [2.8, 0.2], [4.2, 1.0]],
			"learnt-from": 2,
			"error": 0.1,
			"duration": 20000.0,
		},
		"charging": {
			"points": [[3.6, 0.2], [4.2, 1.0]],
			"learnt-from": 1,
			"error": 0.15,
			"duration": 9000.0,
		},
	})
}

/// `curves` as the device sends it for `document`.
fn curves_of(document: &Document) -> Message {
	let span = |span: crate::facts::curve::Span| Span {
		duration: (span.duration * 1e4).round() / 1e4,
		margin: (span.margin * 1e4).round() / 1e4,
	};
	Message::Curves {
		document: Some(document.to_json()),
		lasts: Some(span(document.lasts())),
		recharge: document.recharge().map(span),
	}
}

/// A supply managing a backup supply, its curve file in `dir`.
fn managed(dir: &tempfile::TempDir) -> Supply {
	Supply::managed_for_test(CurveStore::new(dir.path().join("battery-curve.json")))
}

/// Open a curve stream on a session, returning it with the `curves` it was answered with.
async fn curve_stream(streams: &mut Streams) -> (Stream, Message) {
	let mut stream = streams.open().await.unwrap();
	write_message(
		&mut stream,
		&Message::Hello {
			name: "curve-test".to_owned(),
			version: "0.0.0".to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	write_message(&mut stream, &Message::Curve.to_json())
		.await
		.unwrap();
	let answer = next(&mut stream).await;
	(stream, answer)
}

async fn ask(stream: &mut Stream, message: Message) {
	write_message(stream, &message.to_json()).await.unwrap();
}

/// `curve` is answered with the document in force and how long a full charge lasts; a load is
/// accepted and followed by the `curves` it brought about, recharge included once a charging curve
/// is held; and a reset returns to the shipped curve (CRV).
#[tokio::test]
async fn a_curve_stream_reads_loads_and_resets_the_curves() {
	let dir = tempfile::tempdir().unwrap();
	let supply = managed(&dir);
	let mut streams = paired_on(&keys(0x42), Controller::none(), supply.clone()).await;

	let (mut stream, answer) = curve_stream(&mut streams).await;
	let shipped = Document::shipped();
	assert_eq!(answer, curves_of(&shipped));
	assert!(
		matches!(
			&answer,
			Message::Curves {
				lasts: Some(_),
				recharge: None,
				..
			}
		),
		"the shipped curve holds no charging curve, so no recharge"
	);

	ask(&mut stream, Message::Load { document: learnt() }).await;
	assert_eq!(next(&mut stream).await, Message::Accepted);
	let loaded = Document::from_json(&learnt()).unwrap();
	assert_eq!(next(&mut stream).await, curves_of(&loaded));
	assert_eq!(supply.document(), Some(loaded));

	ask(&mut stream, Message::Reset).await;
	assert_eq!(next(&mut stream).await, Message::Accepted);
	assert_eq!(next(&mut stream).await, curves_of(&shipped));
	assert_eq!(supply.document(), Some(shipped));
}

/// A document breaking CRV is refused with the reason it breaks it, and changes nothing (CRV).
#[tokio::test]
async fn an_invalid_document_is_refused_with_its_reason() {
	let dir = tempfile::tempdir().unwrap();
	let supply = managed(&dir);
	let mut streams = paired_on(&keys(0x42), Controller::none(), supply.clone()).await;
	let (mut stream, _) = curve_stream(&mut streams).await;

	let mut invalid = learnt();
	invalid["discharging"]["points"] = json!([[2.6, 0.0]]);
	let reason = Document::from_json(&invalid).unwrap_err().to_string();
	ask(&mut stream, Message::Load { document: invalid }).await;
	assert_eq!(next(&mut stream).await, Message::Refused { reason });
	assert_eq!(supply.document(), Some(Document::shipped()));

	let quiet = tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
	assert!(quiet.is_err(), "nothing changed, so no curves are sent");
}

/// A load on one channel reaches the curve streams open on another, and the others on the same one
/// (CRV).
#[tokio::test]
async fn a_load_is_sent_to_every_open_curve_stream() {
	let dir = tempfile::tempdir().unwrap();
	let supply = managed(&dir);
	let keys = keys(0x42);
	let mut asking = paired_on(&keys, Controller::none(), supply.clone()).await;
	let mut watching = paired_on(&keys, Controller::none(), supply.clone()).await;

	let (mut asker, _) = curve_stream(&mut asking).await;
	let (mut beside, _) = curve_stream(&mut asking).await;
	let (mut other, _) = curve_stream(&mut watching).await;

	ask(&mut asker, Message::Load { document: learnt() }).await;
	assert_eq!(next(&mut asker).await, Message::Accepted);
	let loaded = curves_of(&Document::from_json(&learnt()).unwrap());
	for stream in [&mut asker, &mut beside, &mut other] {
		assert_eq!(next(stream).await, loaded);
	}
}

/// A device managing no backup supply sends no document, and refuses every load and reset (CRV).
#[tokio::test]
async fn with_no_backup_supply_every_change_is_refused() {
	let mut streams = paired_on(&keys(0x42), Controller::none(), Supply::default()).await;
	let (mut stream, answer) = curve_stream(&mut streams).await;
	assert_eq!(
		answer,
		Message::Curves {
			document: None,
			lasts: None,
			recharge: None,
		}
	);

	let refused = Message::Refused {
		reason: "this device manages no backup supply".to_owned(),
	};
	ask(&mut stream, Message::Load { document: learnt() }).await;
	assert_eq!(next(&mut stream).await, refused);
	// Nothing to load into comes before anything wrong with what was sent.
	ask(
		&mut stream,
		Message::Load {
			document: serde_json::json!({"points": []}),
		},
	)
	.await;
	assert_eq!(next(&mut stream).await, refused);
	ask(&mut stream, Message::Reset).await;
	assert_eq!(next(&mut stream).await, refused);
}
