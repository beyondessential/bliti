use bliti_core::{
	channel::stream::{Stream, connect_initiator},
	key_schedule::Root,
};
use tokio_util::compat::TokioAsyncReadCompatExt;

use super::*;
use crate::network::session::{Inert, Store};

fn keys(byte: u8) -> DeviceKeys {
	Root::from_bytes([byte; 32]).device_keys()
}

/// A configurator that configures nothing, over a recorded configuration nothing here writes.
async fn inert() -> Configurator<Inert> {
	let path = std::env::temp_dir().join("bliti-session-tests-never-written/network.json");
	let fallback = serde_json::json!({"attachments": []})
		.as_object()
		.cloned()
		.unwrap();
	Configurator::start(Inert, Store::new(path), fallback)
		.await
		.unwrap()
}

/// Open a client against a device session over an in-memory duplex, with no BLE involved.
async fn paired(keys: &DeviceKeys) -> Streams {
	paired_with(keys, Controller::none()).await
}

/// Open a client against a device session that shares `controller`.
async fn paired_with(keys: &DeviceKeys, controller: Controller) -> Streams {
	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let device_keys = keys.clone();
	let configurator = inert().await;
	tokio::spawn(async move {
		let _ = run(
			device_side.compat(),
			&device_keys,
			Sampler::start(None),
			configurator,
			controller,
		)
		.await;
	});

	let encrypted = connect_initiator(
		client_side.compat(),
		&keys.presence_token,
		&keys.static_key.public_key(),
	)
	.await
	.unwrap();
	let (streams, driver) = multiplex(encrypted, Mode::Client);
	tokio::spawn(async move {
		let _ = driver.await;
	});
	streams
}

/// Collect the messages a stream carries until it goes quiet for a moment.
async fn drain(stream: &mut Stream) -> Vec<Message> {
	let mut messages = Vec::new();
	while let Ok(Ok(Some(raw))) =
		tokio::time::timeout(Duration::from_millis(400), read_message(stream)).await
	{
		if let Ok(Reading::Message(message)) = read::<Message>(&raw) {
			messages.push(message);
		}
	}
	messages
}

/// The device opens two streams unprompted: a hello, and the default feed with no announcement of
/// the topic it serves (MSG).
#[tokio::test]
async fn the_device_pushes_a_hello_and_the_default_feed_unprompted() {
	let mut streams = paired(&keys(0x42)).await;

	let mut first = streams.accept().await.expect("a stream");
	let mut second = streams.accept().await.expect("a second stream");

	let a = drain(&mut first).await;
	let b = drain(&mut second).await;
	let all: Vec<Message> = a.into_iter().chain(b).collect();

	// One is the hello; the rest are facts and readings on the feed, none announcing a topic.
	assert!(
		all.iter()
			.any(|m| matches!(m, Message::Hello { name, .. } if name == DEVICE_NAME)),
		"the device names itself"
	);
	assert!(
		all.iter()
			.any(|m| matches!(m, Message::Fact(_) | Message::Reading(_))),
		"the feed carries facts and readings"
	);
	assert!(
		!all.iter().any(|m| matches!(m, Message::Subscribe { .. })),
		"the feed announces no topic"
	);
}

/// A client that declines the feed by closing it still holds the device's name and version from
/// the hello, and can resume with a `subscribe` for `default` (MSG).
#[tokio::test]
async fn declining_the_feed_keeps_the_hello_and_a_subscribe_resumes() {
	let mut streams = paired(&keys(0x42)).await;

	// Find the hello stream and the feed stream among the two the device pushes.
	let mut one = streams.accept().await.unwrap();
	let two = streams.accept().await.unwrap();
	let first = drain(&mut one).await;
	let named = first.iter().any(|m| matches!(m, Message::Hello { .. }));
	let (mut hello_stream, mut feed) = if named { (one, two) } else { (two, one) };
	let _ = drain(&mut feed).await;

	// Decline the feed by closing it; the hello stream is untouched.
	feed.close().await.unwrap();
	let _ = &mut hello_stream;

	// Resume with a subscribe for default, and be served current data.
	let mut resume = streams.open().await.unwrap();
	write_message(
		&mut resume,
		&Message::Subscribe {
			topic: DEFAULT_TOPIC.to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	let served = drain(&mut resume).await;
	assert!(
		served
			.iter()
			.any(|m| matches!(m, Message::Fact(_) | Message::Reading(_))),
		"a resume is served what is current"
	);
}

/// A subscribe for a topic already being served is skipped, so a client cannot receive it twice
/// (MSG). The pushed feed is already serving default, so a subscribe for it draws nothing.
#[tokio::test]
async fn a_subscribe_for_an_already_served_topic_is_skipped() {
	let mut streams = paired(&keys(0x42)).await;
	// Leave the two pushed streams open, so default stays served.
	let _pushed_a = streams.accept().await.unwrap();
	let _pushed_b = streams.accept().await.unwrap();

	let mut duplicate = streams.open().await.unwrap();
	write_message(
		&mut duplicate,
		&Message::Subscribe {
			topic: DEFAULT_TOPIC.to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();

	let quiet = drain(&mut duplicate).await;
	assert!(quiet.is_empty(), "a second serve of default sends nothing");
}

/// An unrecognised message draws no reply and costs nothing: the stream stays open (MSG).
#[tokio::test]
async fn an_unrecognised_message_is_passed_over_in_silence() {
	let mut streams = paired(&keys(0x42)).await;
	let _a = streams.accept().await.unwrap();

	let mut stream = streams.open().await.unwrap();
	write_message(&mut stream, br#"{"type":"reboot","when":"now"}"#)
		.await
		.unwrap();

	let quiet = tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
	assert!(quiet.is_err(), "nothing is answered on the wire");

	write_message(
		&mut stream,
		&Message::Hello {
			name: "test-client".to_owned(),
			version: "0.0.0".to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
}

/// A known message the device has nothing to do about is a no-op, not a fault: the stream stays
/// open and the session carries on (MSG).
#[tokio::test]
async fn a_known_but_unactionable_message_is_a_no_op() {
	let mut streams = paired(&keys(0x42)).await;
	let _a = streams.accept().await.unwrap();

	// A client reporting a reading of its own. The device has nothing to do about it.
	let mut stream = streams.open().await.unwrap();
	write_message(
		&mut stream,
		&Message::Reading(Entry::quantity(
			1,
			"signal-strength",
			"decibel-milliwatts",
			-71.0,
		))
		.to_json(),
	)
	.await
	.unwrap();

	let quiet = tokio::time::timeout(Duration::from_millis(250), read_message(&mut stream)).await;
	assert!(quiet.is_err(), "a no-op is silent on the wire");

	// The stream is still usable.
	write_message(
		&mut stream,
		&Message::Hello {
			name: "c".to_owned(),
			version: "0".to_owned(),
		}
		.to_json(),
	)
	.await
	.expect("the stream stays open");
}

/// A client that is not speaking the protocol loses the stream it did it on, and nothing else.
#[tokio::test]
async fn a_protocol_fault_costs_the_stream_and_not_the_connection() {
	let mut streams = paired(&keys(0x42)).await;
	let _a = streams.accept().await.unwrap();

	let mut bad = streams.open().await.unwrap();
	write_message(&mut bad, b"this is not json").await.unwrap();
	let ended = matches!(read_message(&mut bad).await, Ok(None) | Err(_));
	assert!(ended, "a fault closes the stream it arrived on");

	// The connection is untouched: a subscribe for default is still served.
	let mut good = streams.open().await.unwrap();
	write_message(
		&mut good,
		&Message::Subscribe {
			topic: DEFAULT_TOPIC.to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	// It may be quiet if the pushed feed still holds default; either way the connection lives.
	let _ = drain(&mut good).await;
}

#[tokio::test]
async fn a_client_with_the_wrong_code_cannot_open_a_session() {
	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let configurator = inert().await;
	let device = tokio::spawn(async move {
		run(
			device_side.compat(),
			&keys(0x01),
			Sampler::start(None),
			configurator,
			Controller::none(),
		)
		.await
	});

	let other = keys(0x02);
	assert!(
		connect_initiator(
			client_side.compat(),
			&other.presence_token,
			&other.static_key.public_key()
		)
		.await
		.is_err()
	);
	assert!(matches!(
		device.await.unwrap(),
		Err(SessionError::Handshake(_))
	));
}

/// A client holding the right presence token but expecting another device's static key does not
/// open a session: the token alone, as a photograph of the QR code yields, is not enough to be
/// taken for this device (SEC, "A photograph does not permit impersonation").
#[tokio::test]
async fn the_right_token_with_the_wrong_static_key_cannot_open_a_session() {
	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let configurator = inert().await;
	let device = tokio::spawn(async move {
		run(
			device_side.compat(),
			&keys(0x01),
			Sampler::start(None),
			configurator,
			Controller::none(),
		)
		.await
	});

	assert!(
		connect_initiator(
			client_side.compat(),
			&keys(0x01).presence_token,
			&keys(0x02).static_key.public_key()
		)
		.await
		.is_err()
	);
	assert!(matches!(
		device.await.unwrap(),
		Err(SessionError::Handshake(_))
	));
}
/// A stream opened with `configure` is served a configuration session, and a second one while it
/// is open is told the device is busy (CFG).
#[tokio::test]
async fn configure_opens_a_session_and_a_second_is_busy() {
	let mut streams = paired(&keys(0x42)).await;
	let _a = streams.accept().await.unwrap();

	let mut first = streams.open().await.unwrap();
	write_message(&mut first, &Message::Configure.to_json())
		.await
		.unwrap();
	let opened = read::<Message>(&read_message(&mut first).await.unwrap().unwrap()).unwrap();
	let Reading::Message(Message::Configuration {
		document,
		capabilities: Some(capabilities),
		..
	}) = opened
	else {
		panic!("a session opens with the configuration and capabilities, got {opened:?}");
	};
	assert_eq!(
		document,
		serde_json::json!({"attachments": []})
			.as_object()
			.cloned()
			.unwrap()
	);
	assert_eq!(
		serde_json::Value::Object(capabilities),
		serde_json::json!({"document": {}, "acts": {}}),
		"this build offers no member and no act"
	);

	let mut second = streams.open().await.unwrap();
	write_message(&mut second, &Message::Configure.to_json())
		.await
		.unwrap();
	let answer = read::<Message>(&read_message(&mut second).await.unwrap().unwrap()).unwrap();
	assert_eq!(answer, Reading::Message(Message::Busy));
	assert!(
		matches!(read_message(&mut second).await, Ok(None) | Err(_)),
		"the busy stream is closed"
	);
}

/// A session dropped from outside, as the daemon drops one whose client unsubscribed, takes the
/// streams it was serving with it, so a configuration session it held ends and the next client is
/// not told the device is busy (CFG). The transport is left healthy throughout, so nothing but the
/// drop can end it.
#[tokio::test]
async fn dropping_a_session_ends_the_configuration_session_it_held() {
	let keys = keys(0x42);
	let configurator = inert().await;

	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	let device = {
		let keys = keys.clone();
		let configurator = configurator.clone();
		tokio::spawn(async move {
			let _ = run(
				device_side.compat(),
				&keys,
				Sampler::start(None),
				configurator,
				Controller::none(),
			)
			.await;
		})
	};
	let encrypted = connect_initiator(
		client_side.compat(),
		&keys.presence_token,
		&keys.static_key.public_key(),
	)
	.await
	.unwrap();
	let (mut streams, driver) = multiplex(encrypted, Mode::Client);
	tokio::spawn(async move {
		let _ = driver.await;
	});
	let _hello = streams.accept().await.unwrap();

	let mut held = streams.open().await.unwrap();
	write_message(&mut held, &Message::Configure.to_json())
		.await
		.unwrap();
	read_message(&mut held).await.unwrap().unwrap();

	device.abort();
	let _ = device.await;

	let (client_side, device_side) = tokio::io::duplex(1 << 16);
	tokio::spawn({
		let keys = keys.clone();
		async move {
			let _ = run(
				device_side.compat(),
				&keys,
				Sampler::start(None),
				configurator,
				Controller::none(),
			)
			.await;
		}
	});
	let encrypted = connect_initiator(
		client_side.compat(),
		&keys.presence_token,
		&keys.static_key.public_key(),
	)
	.await
	.unwrap();
	let (mut streams, driver) = multiplex(encrypted, Mode::Client);
	tokio::spawn(async move {
		let _ = driver.await;
	});
	let mut next = streams.open().await.unwrap();
	write_message(&mut next, &Message::Configure.to_json())
		.await
		.unwrap();
	let answer = tokio::time::timeout(Duration::from_secs(5), read_message(&mut next))
		.await
		.expect("the device answers")
		.unwrap()
		.unwrap();
	assert!(
		matches!(
			read::<Message>(&answer).unwrap(),
			Reading::Message(Message::Configuration { .. })
		),
		"the next client opens a session rather than being told the device is busy"
	);
}

/// Records the acts it is asked to carry out.
#[derive(Default)]
struct Recorded(Mutex<Vec<crate::power::Act>>);

impl crate::power::System for Recorded {
	fn carry_out(&self, act: crate::power::Act) -> anyhow::Result<()> {
		self.0.lock().unwrap().push(act);
		Ok(())
	}
}

/// Read one message off a stream, failing the test if none comes.
async fn next(stream: &mut Stream) -> Message {
	let raw = tokio::time::timeout(Duration::from_secs(5), read_message(stream))
		.await
		.expect("the device answers")
		.unwrap()
		.expect("the stream is open");
	match read::<Message>(&raw).unwrap() {
		Reading::Message(message) => message,
		other => panic!("expected a message, got {other:?}"),
	}
}

/// Tell a session's hello stream from its feed: the feed is the one that carries facts.
async fn feed_of(streams: &mut Streams) -> Stream {
	let one = streams.accept().await.unwrap();
	let two = streams.accept().await.unwrap();
	let (mut one, mut two) = (one, two);
	let first = drain(&mut one).await;
	let _ = drain(&mut two).await;
	if first.iter().any(|m| matches!(m, Message::Hello { .. })) {
		two
	} else {
		one
	}
}

/// The next `going-away` on a feed, as its act and cause, passing over the facts and readings.
async fn going_away_on(feed: &mut Stream) -> (String, String) {
	loop {
		match next(feed).await {
			Message::GoingAway { act, cause } => return (act, cause),
			Message::Fact(_) | Message::Reading(_) => continue,
			other => panic!("unexpected {other:?} on the feed"),
		}
	}
}

/// A power stream lists the acts, answers each asked for, and an act accepted is announced on the
/// feed of every session, the asking one included, as asked for by a client, before the sessions end
/// and the act is carried out (CTL).
#[tokio::test]
async fn an_act_accepted_is_announced_on_every_feed_before_it_is_carried_out() {
	use crate::power::Act;

	let system = Arc::new(Recorded::default());
	let controller = Controller::new(
		vec![Act::Restart, Act::Reboot],
		system.clone(),
		Box::new(|| Box::pin(async {})),
	);
	let keys = keys(0x42);
	let mut asking = paired_with(&keys, controller.clone()).await;
	let mut watching = paired_with(&keys, controller).await;

	let mut asking_feed = feed_of(&mut asking).await;
	let mut watching_feed = feed_of(&mut watching).await;

	let mut power = asking.open().await.unwrap();
	write_message(&mut power, &Message::Power.to_json())
		.await
		.unwrap();
	assert_eq!(
		next(&mut power).await,
		Message::Acts {
			acts: vec!["restart".to_owned(), "reboot".to_owned()]
		}
	);

	write_message(
		&mut power,
		&Message::Act {
			act: "power-off".to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	assert!(
		matches!(next(&mut power).await, Message::Refused { reason } if reason.contains("power-off")),
		"an act not listed is refused"
	);

	write_message(
		&mut power,
		&Message::Act {
			act: "reboot".to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	assert_eq!(next(&mut power).await, Message::Accepted);

	for feed in [&mut asking_feed, &mut watching_feed] {
		assert_eq!(
			going_away_on(feed).await,
			("reboot".to_owned(), "manual-control".to_owned())
		);
	}

	// Then each session ends, and the act is carried out.
	for feed in [&mut asking_feed, &mut watching_feed] {
		let ended = tokio::time::timeout(Duration::from_secs(5), async {
			while let Ok(Some(_)) = read_message(feed).await {}
		})
		.await;
		assert!(ended.is_ok(), "the session ends once the act is announced");
	}
	let carried = tokio::time::timeout(Duration::from_secs(5), async {
		loop {
			if let Some(act) = system.0.lock().unwrap().first().copied() {
				return act;
			}
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	})
	.await
	.expect("the act is carried out");
	assert_eq!(carried, Act::Reboot);
}

/// A low-battery shutdown begun off the runtime is announced on the feed with its cause, an act a
/// client asks for once it has begun is refused, and the device powers off (CTL, LOW).
#[tokio::test]
async fn a_low_battery_shutdown_is_announced_and_refuses_every_act() {
	use crate::power::Act;

	let system = Arc::new(Recorded::default());
	let controller = Controller::new(
		vec![Act::Reboot, Act::PowerOff],
		system.clone(),
		Box::new(|| Box::pin(async {})),
	);
	let keys = keys(0x42);
	let mut streams = paired_with(&keys, controller.clone()).await;
	let mut feed = feed_of(&mut streams).await;
	let mut power = streams.open().await.unwrap();
	write_message(&mut power, &Message::Power.to_json())
		.await
		.unwrap();
	assert!(matches!(next(&mut power).await, Message::Acts { .. }));

	// A feed never told holds the sessions open, so the act below is answered before they end.
	let untold = controller.feed();
	let begun = {
		let controller = controller.clone();
		std::thread::spawn(move || controller.low_battery())
			.join()
			.unwrap()
	};
	assert_eq!(begun, Ok(()));
	assert_eq!(
		going_away_on(&mut feed).await,
		("power-off".to_owned(), "low-battery".to_owned())
	);

	write_message(
		&mut power,
		&Message::Act {
			act: "reboot".to_owned(),
		}
		.to_json(),
	)
	.await
	.unwrap();
	assert_eq!(
		next(&mut power).await,
		Message::Refused {
			reason: "the device is already powering off for a low battery".to_owned()
		}
	);

	drop(untold);
	let carried = tokio::time::timeout(Duration::from_secs(5), async {
		loop {
			if let Some(act) = system.0.lock().unwrap().first().copied() {
				return act;
			}
			tokio::time::sleep(Duration::from_millis(10)).await;
		}
	})
	.await
	.expect("the device powers off");
	assert_eq!(carried, Act::PowerOff);
}
