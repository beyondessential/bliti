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
#[command(name = "bliti", version, about = "QR-anchored BLE device provisioning")]
struct Cli {
	#[command(subcommand)]
	command: Command,

	/// Where the derived presence token is cached.
	#[arg(long, global = true, default_value_os_t = identity::default_cache_path())]
	cache: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Command {
	/// Advertise over BLE and serve provisioning sessions.
	Daemon {
		/// Bluetooth adapter to use. Defaults to the system's first.
		#[arg(long)]
		adapter: Option<String>,
	},

	/// Print the QR code for the board this runs on.
	Qr {
		/// Write the QR code as SVG rather than drawing it in the terminal.
		#[arg(long)]
		svg: bool,
	},

	/// Report which board ID source this board offers and which one wins, without deriving anything.
	BoardId,

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
		Command::Qr { svg } => make_qr(&cli.cache, svg),
		Command::Daemon { adapter } => daemon(&cli.cache, adapter.as_deref()).await,
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
	}
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
async fn daemon(cache: &std::path::Path, adapter: Option<&str>) -> Result<()> {
	device::run(cache, adapter).await
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

#[cfg(not(target_os = "linux"))]
async fn connect(_s: &str, _a: Option<&str>, _t: &str, _ad: Option<&str>) -> Result<()> {
	anyhow::bail!("connecting runs on Linux, against BlueZ")
}

#[cfg(not(target_os = "linux"))]
async fn scan(_code: &str, _seconds: u64, _adapter: Option<&str>) -> Result<()> {
	anyhow::bail!("scanning runs on Linux, against BlueZ")
}

#[cfg(not(target_os = "linux"))]
async fn daemon(_cache: &std::path::Path, _adapter: Option<&str>) -> Result<()> {
	anyhow::bail!("the bliti daemon runs on Linux, against BlueZ")
}
