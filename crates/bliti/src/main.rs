//! bliti: QR-anchored BLE device provisioning.
//!
//! Two things live in this binary: the daemon a device runs, and the QR code generator. They share
//! the board-ID precedence and the key schedule in `bliti-core`, which is what makes the QR code a
//! generator prints match the handle the device advertises.
//!
//! Behaviour is specified under `.workhorse/specs/`.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod facts;
mod gatt;
mod identity;
mod network;
mod qr;
mod sampler;
mod session;

#[cfg(target_os = "linux")]
mod client;
#[cfg(target_os = "linux")]
mod device;

/// How often the rotation salt changes (ADV, "Rotation"). A client recomputes against whatever
/// salt it observes, so nothing a client does depends on this period.
pub const SALT_ROTATION: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Parser)]
#[command(name = "bliti", version = env!("BLITI_VERSION"), about = "QR-anchored BLE device provisioning")]
struct Cli {
	#[command(subcommand)]
	command: Command,

	/// Where the derived presence token is cached.
	#[arg(long, global = true, default_value_os_t = identity::default_cache_path())]
	cache: PathBuf,
}

/// What configures the network beneath a configuration session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum NetworkBackend {
	/// Configure nothing.
	Inert,
	/// iwd, hostapd and systemd-networkd.
	Stack,
}

#[derive(Debug, Subcommand)]
enum Command {
	/// Advertise over BLE and serve provisioning sessions.
	Daemon {
		/// Bluetooth adapter to use. Defaults to the system's first.
		#[arg(long)]
		adapter: Option<String>,

		/// Where the recorded network configuration is kept.
		#[arg(long, default_value_os_t = network::session::default_path())]
		network: PathBuf,

		/// What configures the network: `inert` leaves it as the image brought it up and refuses every
		/// proposal, `stack` drives iwd, hostapd and systemd-networkd.
		#[arg(long, value_enum, default_value_t = NetworkBackend::Inert)]
		network_backend: NetworkBackend,
	},

	/// Print the QR code for the board this runs on.
	Qr {
		/// Write the QR code as SVG rather than drawing it in the terminal.
		#[arg(long)]
		svg: bool,
	},

	/// Report which board ID source this board offers and which one wins, without deriving anything.
	BoardId,

	/// Report what this device's radios can do, and the network capabilities stated from them, as
	/// JSON. Only asks: nothing about the radios or the network is changed.
	Probe,

	/// Render a network configuration document and put it in force, for trying the stack on a
	/// device by hand. Verifies nothing and records nothing, and the daemon replaces it on its next
	/// apply.
	NetworkApply {
		/// The configuration document, as JSON.
		document: PathBuf,

		/// The candidates to bring up, by position in `attachments`.
		#[arg(long, value_delimiter = ',')]
		active: Vec<usize>,

		/// The channel the wireless client is on, as `2ghz:6`, for a hotspot that has to follow it.
		#[arg(long)]
		station_channel: Option<String>,
	},

	/// Scan for the device a QR code belongs to. The client half of discovery, without a browser.
	Scan {
		/// The QR code payload: a QR code URL, its fragment, or the rendering printed beneath the code.
		code: String,

		/// How long to listen for.
		#[arg(long, default_value_t = 10)]
		seconds: u64,

		/// Bluetooth adapter to use. Defaults to the system's first.
		#[arg(long)]
		adapter: Option<String>,
	},

	/// Open a configuration session on a device: each line of standard input is sent as one message,
	/// and each message the device answers with is printed as it arrived. The session ends with
	/// standard input.
	Configure {
		/// The QR code payload: a QR code URL, its fragment, or the rendering printed beneath the code.
		code: String,

		/// The device's address, as reported by `scan`. Found by matching the QR code when absent.
		#[arg(long)]
		address: Option<String>,

		/// Bluetooth adapter to use. Defaults to the system's first.
		#[arg(long)]
		adapter: Option<String>,
	},

	/// Open a channel to a device and exchange the milestone's two messages.
	Connect {
		/// The QR code payload: a QR code URL, its fragment, or the rendering printed beneath the code.
		code: String,

		/// The device's address, as reported by `scan`. Found by matching the QR code when absent.
		#[arg(long)]
		address: Option<String>,

		/// Bluetooth adapter to use. Defaults to the system's first.
		#[arg(long)]
		adapter: Option<String>,
	},
}

fn main() -> Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(
			tracing_subscriber::EnvFilter::try_from_env("BLITI_LOG")
				.unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
		)
		.with_writer(std::io::stderr)
		.init();

	let cli = Cli::parse();
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(run(cli))
}

async fn run(cli: Cli) -> Result<()> {
	match cli.command {
		Command::BoardId => board_id(),
		Command::Probe => probe().await,
		Command::NetworkApply {
			document,
			active,
			station_channel,
		} => network_apply(&document, active, station_channel.as_deref()).await,
		Command::Qr { svg } => make_qr(&cli.cache, svg),
		Command::Daemon {
			adapter,
			network,
			network_backend,
		} => daemon(&cli.cache, &network, network_backend, adapter.as_deref()).await,
		Command::Scan {
			code,
			seconds,
			adapter,
		} => scan(&code, seconds, adapter.as_deref()).await,
		Command::Connect {
			code,
			address,
			adapter,
		} => connect(&code, address.as_deref(), adapter.as_deref()).await,
		Command::Configure {
			code,
			address,
			adapter,
		} => configure(&code, address.as_deref(), adapter.as_deref()).await,
	}
}

/// Report what the radios can do, and the capabilities a configuration session would state (NET).
#[cfg(target_os = "linux")]
async fn probe() -> Result<()> {
	use network::{probe, wired};

	let radios = probe::Nl80211::connect()?
		.radios(&Default::default())
		.await?;
	let wired = wired::interfaces(std::path::Path::new(wired::SYS_CLASS_NET));
	let capabilities = probe::capabilities(&radios, &wired, &probe::Backend::stack());
	for radio in &radios {
		eprintln!("{radio:#?}");
	}
	println!("{}", serde_json::to_string_pretty(&capabilities)?);
	Ok(())
}

/// Render `document` for the candidates named, and apply it with the running system's own stack.
#[cfg(target_os = "linux")]
async fn network_apply(
	document: &std::path::Path,
	active: Vec<usize>,
	station_channel: Option<&str>,
) -> Result<()> {
	use bliti_core::channel::config::Document;
	use network::{apply, probe, render, wired};

	let text = std::fs::read_to_string(document).context("reading the document")?;
	let raw: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&text)?;
	let document =
		Document::parse(&raw).map_err(|e| anyhow::anyhow!("{} (at {})", e.reason, e.at))?;

	let radios = probe::Nl80211::connect()?
		.radios(&Default::default())
		.await?;
	let wired = wired::interfaces(std::path::Path::new(wired::SYS_CLASS_NET));
	let hardware = probe::render_hardware(radios.first(), &wired, "ap0", render::Paths::system());
	let station_channel = station_channel
		.map(|text| {
			let (band, number) = text.split_once(':').context("a channel is `band:number`")?;
			let band = match band {
				"2ghz" => render::Band::TwoPointFour,
				"5ghz" => render::Band::Five,
				other => anyhow::bail!("{other:?} is not a band"),
			};
			Ok(render::Channel {
				band,
				number: number.parse()?,
			})
		})
		.transpose()?;
	let selection = render::Selection {
		active,
		station_channel,
		hotspot_waits: false,
	};

	let rendered = render::render(&document, &hardware, &selection)?;
	let changes = tokio::task::spawn_blocking(move || {
		let mut system = apply::Linux::new()?;
		apply::apply(
			&rendered,
			&hardware,
			std::path::Path::new(apply::STATE),
			&mut system,
		)
		.map_err(anyhow::Error::from)
	})
	.await??;
	println!("{changes:#?}");
	Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn network_apply(
	_document: &std::path::Path,
	_active: Vec<usize>,
	_station_channel: Option<&str>,
) -> Result<()> {
	anyhow::bail!("applying a network configuration needs Linux")
}

#[cfg(not(target_os = "linux"))]
async fn probe() -> Result<()> {
	anyhow::bail!("probing radios needs nl80211, which only Linux has")
}

/// Report what the board offers. Probes only: no source value is read, so this is safe and instant
/// even where the winning source is a TPM.
fn board_id() -> Result<()> {
	use bliti_core::board_id::{BoardIdSource, strongest_present};

	identity::guard_unreadable_sources()?;
	let sources = identity::sources();
	for source in &sources {
		let presence = source.probe()?;
		println!("{:>28}  {presence:?}", source.kind().to_string());
	}

	let refs: Vec<&dyn BoardIdSource> = sources.iter().map(AsRef::as_ref).collect();
	match strongest_present(&refs)? {
		Some(kind) => println!("\nwinning source: {kind}"),
		None => println!("\nno usable source: this board cannot derive a QR code"),
	}
	Ok(())
}

/// Print the QR code for this board, deriving its secret if the cache does not already hold it.
fn make_qr(cache: &std::path::Path, svg: bool) -> Result<()> {
	let identity = identity::establish(cache).context("establishing this board's identity")?;
	if identity.derived {
		tracing::info!(source = %identity.kind, "derived this board's presence token");
	}

	let payload = bliti_core::qr::QrPayload::new(identity.secret);
	let code = qr::Printable::new(&payload)?;

	if svg {
		println!("{}", code.to_svg());
	} else {
		println!("{}", code.to_terminal());
		println!("{}", code.url);
	}
	// The human-readable rendering is printed beneath the code, so a scuffed QR code stays usable.
	println!("\n{}", code.human);
	Ok(())
}

#[cfg(target_os = "linux")]
async fn daemon(
	cache: &std::path::Path,
	network: &std::path::Path,
	backend: NetworkBackend,
	adapter: Option<&str>,
) -> Result<()> {
	device::run(cache, network, backend, adapter).await
}

/// Read a QR code however it was given: the URL a code encodes, its fragment alone, or the
/// human-readable rendering printed beneath the code. All three carry the same payload.
fn read_qr(given: &str) -> Result<bliti_core::qr::QrPayload> {
	bliti_core::qr::QrPayload::read(given).context("reading the QR code")
}

#[cfg(target_os = "linux")]
async fn scan(code: &str, seconds: u64, adapter: Option<&str>) -> Result<()> {
	device::scan(&read_qr(code)?, seconds, adapter).await
}

#[cfg(target_os = "linux")]
async fn connect(code: &str, address: Option<&str>, adapter: Option<&str>) -> Result<()> {
	let payload = read_qr(code)?;
	let address = address
		.map(str::parse::<bluer::Address>)
		.transpose()
		.context("reading the device address")?;
	client::connect(address, payload.secret(), adapter).await
}

#[cfg(target_os = "linux")]
async fn configure(code: &str, address: Option<&str>, adapter: Option<&str>) -> Result<()> {
	let payload = read_qr(code)?;
	let address = address
		.map(str::parse::<bluer::Address>)
		.transpose()
		.context("reading the device address")?;
	client::configure(address, payload.secret(), adapter).await
}

#[cfg(not(target_os = "linux"))]
async fn configure(_code: &str, _address: Option<&str>, _adapter: Option<&str>) -> Result<()> {
	anyhow::bail!("configuring runs on Linux, against BlueZ")
}

#[cfg(not(target_os = "linux"))]
async fn connect(_s: &str, _a: Option<&str>, _t: &str, _ad: Option<&str>) -> Result<()> {
	anyhow::bail!("connecting runs on Linux, against BlueZ")
}

#[cfg(not(target_os = "linux"))]
async fn scan(_code: &str, _seconds: u64, _adapter: Option<&str>) -> Result<()> {
	anyhow::bail!("scanning runs on Linux, against BlueZ")
}

#[cfg(not(target_os = "linux"))]
async fn daemon(
	_cache: &std::path::Path,
	_network: &std::path::Path,
	_backend: NetworkBackend,
	_adapter: Option<&str>,
) -> Result<()> {
	anyhow::bail!("the bliti daemon runs on Linux, against BlueZ")
}
