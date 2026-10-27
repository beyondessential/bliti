use std::time::Duration;

use bliti_core::channel::{
	envelope::{Reading, read},
	stream::{read_message, write_message},
};
use futures::io::{AsyncReadExt, ReadHalf, WriteHalf};
use tokio_util::compat::{Compat, TokioAsyncReadCompatExt};

use super::*;
use fake::{Fake, GOOD_KEY, JOINED};

type End = Compat<tokio::io::DuplexStream>;

/// Whatever the client end of a Tailscale stream has been sent.
struct Client {
	reader: ReadHalf<End>,
	writer: WriteHalf<End>,
	serving: tokio::task::JoinHandle<()>,
}

impl Client {
	async fn send(&mut self, message: Message) {
		write_message(&mut self.writer, &message.to_json())
			.await
			.unwrap();
	}

	async fn act(&mut self, act: &str, method: Option<&str>, key: Option<&str>) {
		self.send(Message::TailscaleAct {
			act: act.to_owned(),
			method: method.map(ToOwned::to_owned),
			key: key.map(Secret::new),
			name: None,
			peer: None,
		})
		.await;
	}

	async fn next(&mut self) -> Message {
		let raw = tokio::time::timeout(Duration::from_secs(600), read_message(&mut self.reader))
			.await
			.expect("the device answers")
			.unwrap()
			.expect("the stream is open");
		match read::<Message>(&raw).unwrap() {
			Reading::Message(message) => message,
			other => panic!("{other:?}"),
		}
	}

	/// The next message that is not a change in the acts listed, which a state change pushes whenever
	/// it happens.
	async fn next_result(&mut self) -> Message {
		loop {
			match self.next().await {
				Message::TailscaleActs { .. } => {}
				other => return other,
			}
		}
	}

	async fn next_acts(&mut self) -> Vec<String> {
		match self.next().await {
			Message::TailscaleActs { acts } => acts,
			other => panic!("{other:?}"),
		}
	}
}

/// What the fake `tailscale netcheck` prints, spaced as Tailscale's own output is.
const NETCHECK_REPORT: &str = "{\n  \"UDP\": true,\n  \"IPv4\": true,\n  \"GlobalV4\": \"203.0.113.7:41641\",\n  \"RegionLatency\": {\"5\": 38400000}\n}\n";

fn tailscale_with(
	fake: &Arc<Fake>,
	options: Options,
) -> impl std::future::Future<Output = Tailscale> {
	struct Netcheck;
	#[async_trait::async_trait]
	impl Runner for Netcheck {
		fn available(&self) -> bool {
			true
		}
		async fn run(&self) -> Result<String, String> {
			Ok(NETCHECK_REPORT.to_owned())
		}
	}
	Tailscale::start(fake.clone(), Arc::new(Netcheck), options)
}

/// Open a Tailscale stream against `tailscale` and read its opening list of acts.
async fn open(tailscale: &Tailscale) -> (Client, Vec<String>) {
	let (device, client) = tokio::io::duplex(64 * 1024);
	let tailscale = tailscale.clone();
	let serving = tokio::spawn(async move {
		let mut device = device.compat();
		let _ = serve(&mut device, &tailscale, &Peer::default()).await;
	});
	let (reader, writer) = client.compat().split();
	let mut client = Client {
		reader,
		writer,
		serving,
	};
	client.send(Message::Tailscale).await;
	let acts = client.next_acts().await;
	(client, acts)
}

fn strings(acts: &[&str]) -> Vec<String> {
	acts.iter().map(|act| (*act).to_owned()).collect()
}

#[tokio::test(start_paused = true)]
async fn the_opening_list_follows_the_state() {
	let fake = Fake::in_state("Running");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (_client, acts) = open(&tailscale).await;
	assert_eq!(acts, strings(&["turn-off", "log-out", "netcheck", "ping"]));
}

#[tokio::test(start_paused = true)]
async fn turning_off_is_accepted_then_the_list_changes() {
	let fake = Fake::in_state("Running");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("turn-off", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert_eq!(
		client.next_acts().await,
		strings(&["turn-on", "log-out", "netcheck"])
	);
	client.act("turn-on", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert_eq!(
		client.next_acts().await,
		strings(&["turn-off", "log-out", "netcheck", "ping"])
	);
}

#[tokio::test(start_paused = true)]
async fn an_act_not_listed_is_refused_at_act() {
	let fake = Fake::in_state("Running");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("turn-on", None, None).await;
	match client.next().await {
		Message::CannotAct { act, at, .. } => {
			assert_eq!((act.as_str(), at.as_str()), ("turn-on", "$['act']"))
		}
		other => panic!("{other:?}"),
	}
	assert!(!fake.calls().contains(&"turn-on".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_sign_in_without_its_key_is_refused_at_key_and_takes_no_slot() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("sign-in", Some("auth-key"), None).await;
	assert!(matches!(client.next().await, Message::CannotAct { at, .. } if at == "$['key']"));
	client
		.act("sign-in", Some("auth-key"), Some(GOOD_KEY))
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
}

#[tokio::test(start_paused = true)]
async fn an_act_is_refused_while_another_is_under_way_on_any_stream() {
	let fake = Fake::in_state("Running");
	fake.stick();
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut first, _) = open(&tailscale).await;
	let (mut second, _) = open(&tailscale).await;
	first.act("turn-off", None, None).await;
	assert_eq!(first.next().await, Message::TailscaleAccepted);
	second.act("log-out", None, None).await;
	match second.next().await {
		Message::CannotAct { at, reason, .. } => {
			assert_eq!(at, "$");
			assert!(reason.contains("turning Tailscale off"), "{reason}");
		}
		other => panic!("{other:?}"),
	}
}

#[tokio::test(start_paused = true)]
async fn a_transition_that_never_arrives_fails_on_its_deadline() {
	let fake = Fake::in_state("Running");
	fake.stick();
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("turn-off", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert!(
		matches!(client.next().await, Message::TailscaleFailed { reason } if reason.contains("did not turn off"))
	);
	// The slot is free again.
	client.act("turn-off", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
}

#[tokio::test(start_paused = true)]
async fn a_call_tailscaled_refuses_fails_with_its_words() {
	let fake = Fake::in_state("Running");
	fake.refuse();
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("turn-off", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert!(
		matches!(client.next().await, Message::TailscaleFailed { reason } if reason.contains("prefs write access denied"))
	);
}

#[tokio::test(start_paused = true)]
async fn an_auth_key_signs_in_and_registers_the_name() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, acts) = open(&tailscale).await;
	assert_eq!(acts, strings(&["sign-in", "netcheck"]));
	client
		.send(Message::TailscaleAct {
			act: "sign-in".into(),
			method: Some("auth-key".into()),
			key: Some(Secret::new(GOOD_KEY)),
			name: Some("iti-7".into()),
			peer: None,
		})
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert_eq!(
		client.next_acts().await,
		strings(&["turn-off", "log-out", "netcheck", "ping"])
	);
	assert!(
		fake.calls()
			.iter()
			.any(|call| call == "start key=true hostname=iti-7")
	);
}

#[tokio::test(start_paused = true)]
async fn a_rejected_key_fails_with_the_reason_and_not_the_key() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client
		.act("sign-in", Some("auth-key"), Some("tskey-auth-bad"))
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	match client.next().await {
		Message::TailscaleFailed { reason } => {
			assert!(reason.contains("invalid key"), "{reason}");
			assert!(!reason.contains("tskey"), "{reason}");
		}
		other => panic!("{other:?}"),
	}
}

#[tokio::test(start_paused = true)]
async fn a_login_link_is_forwarded_and_approving_it_signs_in() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("sign-in", Some("login-link"), None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	match client.next().await {
		Message::TailscaleLoginLink { url } => {
			assert_eq!(url.expose(), "https://login.example/a/abc123")
		}
		other => panic!("{other:?}"),
	}
	fake.approve();
	assert_eq!(
		client.next_acts().await,
		strings(&["turn-off", "log-out", "netcheck", "ping"])
	);
}

#[tokio::test(start_paused = true)]
async fn a_login_link_nobody_opens_fails_on_the_deadline() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("sign-in", Some("login-link"), None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert!(matches!(
		client.next().await,
		Message::TailscaleLoginLink { .. }
	));
	assert!(
		matches!(client.next().await, Message::TailscaleFailed { reason } if reason.contains("not approved"))
	);
}

#[tokio::test(start_paused = true)]
async fn a_stream_ending_mid_login_link_leaves_tailscale_as_it_stood() {
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("sign-in", Some("login-link"), None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	let _ = client.next().await;
	let serving = client.serving;
	drop((client.reader, client.writer));
	serving.await.unwrap();
	tokio::time::sleep(Duration::from_secs(1)).await;
	assert_eq!(
		tailscale.observed().and_then(|observed| observed.state()),
		Some(state::State::NeedsLogin)
	);
	// The slot was released with the stream, so another act is accepted.
	let (mut again, _) = open(&tailscale).await;
	again.act("sign-in", Some("login-link"), None).await;
	assert_eq!(again.next().await, Message::TailscaleAccepted);
}

#[tokio::test(start_paused = true)]
async fn a_stream_ending_mid_turn_on_leaves_it_to_finish() {
	let fake = Fake::in_state("Stopped");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("turn-on", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	let serving = client.serving;
	drop((client.reader, client.writer));
	serving.await.unwrap();
	tokio::time::sleep(Duration::from_secs(1)).await;
	assert_eq!(
		tailscale.observed().and_then(|observed| observed.state()),
		Some(state::State::Running)
	);
}

#[tokio::test(start_paused = true)]
async fn a_foreign_sign_in_is_logged_out_and_failed() {
	let fake = Fake::in_state("NeedsLogin");
	fake.joins("other.example");
	let options = Options {
		tailnet: Some(JOINED.to_owned()),
		..Options::default()
	};
	let tailscale = tailscale_with(&fake, options).await;
	let (mut client, _) = open(&tailscale).await;
	client
		.act("sign-in", Some("auth-key"), Some(GOOD_KEY))
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	match client.next_result().await {
		Message::TailscaleFailed { reason } => assert!(reason.contains(JOINED), "{reason}"),
		other => panic!("{other:?}"),
	}
	assert!(fake.calls().contains(&"logout".to_owned()));
	assert_eq!(
		tailscale.observed().and_then(|observed| observed.state()),
		Some(state::State::NeedsLogin)
	);
}

#[tokio::test(start_paused = true)]
async fn an_unrestricted_device_accepts_any_tailnet() {
	let fake = Fake::in_state("NeedsLogin");
	fake.joins("other.example");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client
		.act("sign-in", Some("auth-key"), Some(GOOD_KEY))
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	let _ = client.next_acts().await;
	assert!(!fake.calls().contains(&"logout".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn netcheck_runs_signed_out_and_names_relays_it_has_learned() {
	let fake = Fake::in_state("NeedsLogin");
	fake.relays(&[(5, "Sydney")]);
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("netcheck", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	match client.next().await {
		Message::TailscaleResult { result } => {
			assert_eq!(result["udp"], serde_json::json!(true));
			assert_eq!(
				result["relays"],
				serde_json::json!([{"id": 5, "name": "Sydney", "latency": 0.0384}])
			);
			assert_eq!(result["report"], serde_json::json!(NETCHECK_REPORT));
		}
		other => panic!("{other:?}"),
	}
}

#[tokio::test(start_paused = true)]
async fn a_stream_ending_mid_netcheck_ends_it_and_frees_the_slot() {
	struct Slow;
	#[async_trait::async_trait]
	impl Runner for Slow {
		fn available(&self) -> bool {
			true
		}
		async fn run(&self) -> Result<String, String> {
			std::future::pending().await
		}
	}
	let fake = Fake::in_state("Running");
	let tailscale = Tailscale::start(fake.clone(), Arc::new(Slow), Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("netcheck", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	let serving = client.serving;
	drop((client.reader, client.writer));
	serving.await.unwrap();
	tokio::time::sleep(Duration::from_secs(1)).await;
	let (mut again, _) = open(&tailscale).await;
	again.act("netcheck", None, None).await;
	assert_eq!(again.next().await, Message::TailscaleAccepted);
}

#[tokio::test(start_paused = true)]
async fn ping_uses_the_default_peer_and_names_it() {
	let fake = Fake::in_state("Running");
	fake.peer("canopy");
	fake.pong(client::Pong {
		latency_seconds: 0.002,
		endpoint: "192.0.2.1:41641".into(),
		..client::Pong::default()
	});
	let options = Options {
		ping_default: Some("canopy".to_owned()),
		..Options::default()
	};
	let tailscale = tailscale_with(&fake, options).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("ping", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	match client.next().await {
		Message::TailscaleResult { result } => {
			assert_eq!(result["peer"], serde_json::json!("canopy.fleet.example"));
			assert_eq!(result["path"], serde_json::json!("direct"));
		}
		other => panic!("{other:?}"),
	}
}

#[tokio::test(start_paused = true)]
async fn ping_with_no_peer_and_no_default_fails() {
	let fake = Fake::in_state("Running");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client.act("ping", None, None).await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	assert!(matches!(
		client.next().await,
		Message::TailscaleFailed { .. }
	));
}

#[tokio::test(start_paused = true)]
async fn a_device_without_tailscaled_offers_only_netcheck() {
	let tailscale = Tailscale::none();
	assert!(tailscale.listed().is_empty());
	assert!(tailscale.observed().is_none());
}

/// Everything logged by any test in this process. A scoped subscriber would miss the events of a call
/// site another test thread settled first, so the one subscriber is global and the test looks only for
/// what is its own.
fn logged() -> Arc<Mutex<Vec<u8>>> {
	use std::{io::Write, sync::OnceLock};
	#[derive(Clone)]
	struct Captured(Arc<Mutex<Vec<u8>>>);
	impl Write for Captured {
		fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
			self.0.lock().unwrap().extend_from_slice(buf);
			Ok(buf.len())
		}
		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}
	impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
		type Writer = Self;
		fn make_writer(&'a self) -> Self {
			self.clone()
		}
	}
	static LOG: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
	LOG.get_or_init(|| {
		let log = Arc::new(Mutex::new(Vec::new()));
		let subscriber = tracing_subscriber::fmt()
			.with_writer(Captured(log.clone()))
			.with_max_level(tracing::Level::TRACE)
			.finish();
		let _ = tracing::subscriber::set_global_default(subscriber);
		log
	})
	.clone()
}

#[tokio::test(start_paused = true)]
async fn the_log_never_holds_the_key() {
	let log = logged();
	let fake = Fake::in_state("NeedsLogin");
	let tailscale = tailscale_with(&fake, Options::default()).await;
	let (mut client, _) = open(&tailscale).await;
	client
		.act("sign-in", Some("auth-key"), Some("tskey-auth-hunter2"))
		.await;
	assert_eq!(client.next().await, Message::TailscaleAccepted);
	let failure = client.next().await;
	assert!(!format!("{failure:?}").contains("hunter2"));
	let log = String::from_utf8(log.lock().unwrap().clone()).unwrap();
	assert!(log.contains("act asked for and accepted"), "{log}");
	assert!(!log.contains("hunter2"), "{log}");
}
