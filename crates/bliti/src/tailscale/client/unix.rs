//! The local API over tailscaled's unix socket: one HTTP/1 connection per call.

use std::{net::IpAddr, path::PathBuf, time::Duration};

use async_trait::async_trait;
use bliti_core::channel::messages::Secret;
use bytes::{Buf, Bytes, BytesMut};
use futures::stream::{BoxStream, StreamExt};
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, StatusCode, body::Incoming, client::conn::http1};
use hyper_util::rt::TokioIo;
use serde_json::{Value as Json, json};
use tokio::net::UnixStream;

use super::{
	ApiError, CALL_TIMEOUT, DerpMap, LocalApi, Notify, PROBE_TIMEOUT, Pong, Prefs, Status,
	read_json,
};

/// The notifications to ask the bus for: the state and the health as they stand now, then each change.
const WATCH_MASK: u32 = 2 | 128;

/// tailscaled's local API on a unix socket.
#[derive(Debug, Clone)]
pub struct Unix {
	socket: PathBuf,
}

impl Unix {
	/// A client of the daemon listening on `socket`.
	pub fn new(socket: impl Into<PathBuf>) -> Self {
		Self {
			socket: socket.into(),
		}
	}

	async fn send(
		&self,
		method: Method,
		path: &str,
		body: Option<Json>,
	) -> Result<hyper::Response<Incoming>, ApiError> {
		let stream = UnixStream::connect(&self.socket)
			.await
			.map_err(|err| ApiError::Unreachable(err.to_string()))?;
		let (mut sender, connection) = http1::handshake(TokioIo::new(stream))
			.await
			.map_err(|err| ApiError::Unreachable(err.to_string()))?;
		tokio::spawn(async move {
			let _ = connection.await;
		});
		let payload = body.map_or_else(Bytes::new, |body| Bytes::from(body.to_string()));
		let request = Request::builder()
			.method(method)
			.uri(format!("/localapi/v0/{path}"))
			.header("host", "local-tailscaled.sock")
			.header("content-type", "application/json")
			.body(Full::new(payload))
			.map_err(|err| ApiError::Unreadable(err.to_string()))?;
		sender
			.send_request(request)
			.await
			.map_err(|err| ApiError::Unreachable(err.to_string()))
	}

	/// A call that answers once, with its body whole.
	async fn call(
		&self,
		method: Method,
		path: &str,
		body: Option<Json>,
	) -> Result<Bytes, ApiError> {
		self.call_within(CALL_TIMEOUT, method, path, body).await
	}

	/// A call that answers once, with its body whole, given up on after `limit`.
	async fn call_within(
		&self,
		limit: Duration,
		method: Method,
		path: &str,
		body: Option<Json>,
	) -> Result<Bytes, ApiError> {
		let exchange = async {
			let response = self.send(method, path, body).await?;
			let status = response.status();
			let bytes = response
				.into_body()
				.collect()
				.await
				.map_err(|err| ApiError::Unreachable(err.to_string()))?
				.to_bytes();
			if status.is_success() {
				Ok(bytes)
			} else {
				Err(refused(status, &bytes))
			}
		};
		tokio::time::timeout(limit, exchange)
			.await
			.map_err(|_| ApiError::Timeout)?
	}
}

fn refused(status: StatusCode, body: &Bytes) -> ApiError {
	ApiError::Refused {
		status: status.as_u16(),
		message: String::from_utf8_lossy(body).trim().to_owned(),
	}
}

#[async_trait]
impl LocalApi for Unix {
	async fn status(&self) -> Result<Status, ApiError> {
		read_json(&self.call(Method::GET, "status?peers=true", None).await?)
	}

	async fn watch(&self) -> Result<BoxStream<'static, Result<Notify, ApiError>>, ApiError> {
		let response = tokio::time::timeout(
			CALL_TIMEOUT,
			self.send(
				Method::GET,
				&format!("watch-ipn-bus?mask={WATCH_MASK}"),
				None,
			),
		)
		.await
		.map_err(|_| ApiError::Timeout)??;
		let status = response.status();
		if !status.is_success() {
			let body = response
				.into_body()
				.collect()
				.await
				.map(|body| body.to_bytes())
				.unwrap_or_default();
			return Err(refused(status, &body));
		}
		let lines = futures::stream::unfold(
			(response.into_body(), BytesMut::new()),
			|(mut body, mut held)| async move {
				loop {
					if let Some(end) = held.iter().position(|byte| *byte == b'\n') {
						let line = held.split_to(end + 1);
						let line = &line[..end];
						if line.iter().all(u8::is_ascii_whitespace) {
							continue;
						}
						let notify = read_json::<Notify>(&Bytes::copy_from_slice(line));
						return Some((notify, (body, held)));
					}
					match body.frame().await {
						Some(Ok(frame)) => {
							if let Ok(data) = frame.into_data() {
								held.extend_from_slice(data.chunk());
							}
						}
						Some(Err(err)) => {
							return Some((
								Err(ApiError::Unreachable(err.to_string())),
								(body, held),
							));
						}
						None => return None,
					}
				}
			},
		);
		Ok(lines.boxed())
	}

	async fn prefs(&self) -> Result<Prefs, ApiError> {
		read_json(&self.call(Method::GET, "prefs", None).await?)
	}

	async fn set_running(&self, on: bool) -> Result<(), ApiError> {
		let body = json!({ "WantRunningSet": true, "WantRunning": on });
		self.call(Method::PATCH, "prefs", Some(body)).await?;
		Ok(())
	}

	async fn start(&self, prefs: Prefs, auth_key: Option<Secret>) -> Result<(), ApiError> {
		let mut options = serde_json::Map::new();
		options.insert("UpdatePrefs".to_owned(), Json::Object(prefs));
		if let Some(key) = auth_key {
			options.insert("AuthKey".to_owned(), key.expose().into());
		}
		self.call(Method::POST, "start", Some(Json::Object(options)))
			.await?;
		Ok(())
	}

	async fn login_interactive(&self) -> Result<(), ApiError> {
		self.call(Method::POST, "login-interactive", None).await?;
		Ok(())
	}

	async fn logout(&self) -> Result<(), ApiError> {
		self.call(Method::POST, "logout", None).await?;
		Ok(())
	}

	async fn ping(&self, address: IpAddr) -> Result<Pong, ApiError> {
		// tailscaled leaves the wait for a probe to its caller, so a peer that does not answer is a
		// probe that timed out here, not a failure of the call.
		let answer = self
			.call_within(
				PROBE_TIMEOUT,
				Method::POST,
				&format!("ping?ip={address}&type=disco&size=0"),
				None,
			)
			.await;
		match answer {
			Ok(body) => read_json(&body),
			Err(ApiError::Timeout) => Ok(Pong {
				err: "no answer in time".to_owned(),
				..Pong::default()
			}),
			Err(err) => Err(err),
		}
	}

	async fn derp_map(&self) -> Result<Option<DerpMap>, ApiError> {
		read_json(&self.call(Method::GET, "derpmap", None).await?)
	}
}

#[cfg(test)]
mod tests;
