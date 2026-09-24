//! The device's own observers and platform: rtnetlink, iwd, nl80211 and raw sockets.

use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::mpsc;

use super::{Observation, gateway, iwd, nl80211, rtnl};
use crate::network::{render, stack::Platform};

/// Start watching the running system, and what the backend asks of it.
///
/// iwd not running is not fatal: its stations are observed from when it appears.
pub async fn linux(
	paths: render::Paths,
) -> anyhow::Result<(Platform, mpsc::UnboundedReceiver<Observation>)> {
	let (observations, observed) = mpsc::unbounded_channel();
	rtnl::watch(observations.clone())
		.await
		.context("watching links over rtnetlink")?;
	let iwd = iwd::Iwd::start(paths, observations)
		.await
		.context("watching iwd over the system bus")?;
	let air = nl80211::Air::connect().context("connecting to nl80211")?;
	let platform = Platform {
		iwd: Arc::new(iwd),
		air: Arc::new(air),
		gateway: Arc::new(gateway::Probe),
	};
	Ok((platform, observed))
}
