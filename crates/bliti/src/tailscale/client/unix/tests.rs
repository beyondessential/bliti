use tokio::{
	io::{AsyncReadExt, AsyncWriteExt},
	net::UnixListener,
};

use super::*;

/// A socket answering every request with `answer`, or never where there is none.
fn serve(listener: UnixListener, answer: Option<&'static str>) {
	tokio::spawn(async move {
		let mut held = Vec::new();
		while let Ok((mut stream, _)) = listener.accept().await {
			if let Some(answer) = answer {
				let mut request = [0_u8; 1024];
				let _ = stream.read(&mut request).await;
				let _ = stream.write_all(answer.as_bytes()).await;
			} else {
				held.push(stream);
			}
		}
	});
}

fn socket(directory: &tempfile::TempDir) -> (UnixListener, Unix) {
	let path = directory.path().join("tailscaled.sock");
	(UnixListener::bind(&path).unwrap(), Unix::new(path))
}

#[tokio::test(start_paused = true)]
async fn a_probe_that_gets_no_answer_is_an_unanswered_probe() {
	let directory = tempfile::tempdir().unwrap();
	let (listener, api) = socket(&directory);
	serve(listener, None);
	let pong = api.ping("100.64.0.2".parse().unwrap()).await.unwrap();
	assert!(!pong.err.is_empty());
	assert!(pong.endpoint.is_empty());
}

#[tokio::test(start_paused = true)]
async fn another_call_that_gets_no_answer_is_a_timeout() {
	let directory = tempfile::tempdir().unwrap();
	let (listener, api) = socket(&directory);
	serve(listener, None);
	assert!(matches!(api.prefs().await, Err(ApiError::Timeout)));
}

#[tokio::test]
async fn an_answered_probe_is_read() {
	let directory = tempfile::tempdir().unwrap();
	let (listener, api) = socket(&directory);
	let body = r#"{"LatencySeconds":0.002,"Endpoint":"192.0.2.1:41641"}"#;
	let answer: &'static str = Box::leak(
		format!(
			"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
			body.len()
		)
		.into_boxed_str(),
	);
	serve(listener, Some(answer));
	let pong = api.ping("100.64.0.2".parse().unwrap()).await.unwrap();
	assert_eq!(pong.endpoint, "192.0.2.1:41641");
	assert!(pong.err.is_empty());
}
