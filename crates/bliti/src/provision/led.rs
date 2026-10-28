//! The status LED of a device in provisioning mode, showing the state of its run (PLED).
//!
//! Driven only from provisioning mode: nothing outside it starts a [`drive`], which is how a device
//! that is not in provisioning mode leaves the LED alone (PLED).

use std::{
	io,
	path::{Path, PathBuf},
	time::Duration,
};

use async_trait::async_trait;
use bliti_core::provisioning::report::RunState;
use tokio::{
	sync::watch,
	time::{Instant, sleep_until},
};

use super::run::Snapshot;

/// Where the kernel lists the board's LEDs.
const LEDS: &str = "/sys/class/leds";

/// The names the kernel has given the green LED, newest first: `ACT` is the activity LED.
const GREEN: &[&str] = &["ACT", "led0"];
/// The names the kernel has given the red LED, newest first: `PWR` is the power LED.
const RED: &[&str] = &["PWR", "led1"];

/// The activity LED is lit by driving its line low.
const GREEN_ACTIVE_LOW: bool = true;
const RED_ACTIVE_LOW: bool = false;

/// The two LEDs of the board, as the colours they show.
#[async_trait]
pub trait Led: Send {
	/// Light the red LED and the green one, or not, each as given. Both lit is amber.
	async fn show(&mut self, red: bool, green: bool) -> io::Result<()>;
}

/// The board's LEDs through sysfs.
pub struct Sysfs {
	red: Channel,
	green: Channel,
}

#[derive(Clone)]
struct Channel {
	brightness: PathBuf,
	active_low: bool,
}

impl Sysfs {
	/// The board's LEDs, taken off whatever the kernel has been making them do.
	pub fn open() -> io::Result<Self> {
		Self::at(Path::new(LEDS))
	}

	fn at(root: &Path) -> io::Result<Self> {
		Ok(Self {
			red: Channel::take(root, RED, RED_ACTIVE_LOW)?,
			green: Channel::take(root, GREEN, GREEN_ACTIVE_LOW)?,
		})
	}
}

impl Channel {
	/// The first of `names` the kernel has an LED by, with its trigger set to `none`.
	///
	/// No hardware trigger is ever used to blink: a daemon that has hung must stop blinking, which a
	/// kernel timer would not (PLED).
	fn take(root: &Path, names: &[&str], active_low: bool) -> io::Result<Self> {
		let dir = names
			.iter()
			.map(|name| root.join(name))
			.find(|dir| dir.is_dir())
			.ok_or_else(|| {
				io::Error::new(
					io::ErrorKind::NotFound,
					format!("no LED named {} in {}", names.join(" or "), root.display()),
				)
			})?;
		std::fs::write(dir.join("trigger"), "none")?;
		Ok(Self {
			brightness: dir.join("brightness"),
			active_low,
		})
	}

	fn set(&self, lit: bool) -> io::Result<()> {
		std::fs::write(
			&self.brightness,
			if lit != self.active_low { "1" } else { "0" },
		)
	}
}

#[async_trait]
impl Led for Sysfs {
	async fn show(&mut self, red: bool, green: bool) -> io::Result<()> {
		let (red_channel, green_channel) = (self.red.clone(), self.green.clone());
		tokio::task::spawn_blocking(move || {
			let red = red_channel.set(red);
			green_channel.set(green).and(red)
		})
		.await
		.map_err(io::Error::other)?
	}
}

/// What the LED does for a state of the run (PLED, "The states").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pattern {
	Steady(Colour),
	/// Lit then dark, this many times a second.
	Blink(Colour, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Colour {
	Red,
	Green,
	Amber,
}

impl Colour {
	/// Which LEDs are lit to make this colour: red, then green.
	fn lit(self) -> (bool, bool) {
		match self {
			Self::Red => (true, false),
			Self::Green => (false, true),
			Self::Amber => (true, true),
		}
	}
}

impl Pattern {
	// spec: PLED
	fn of(run: RunState) -> Self {
		match run {
			RunState::Idle => Self::Steady(Colour::Amber),
			RunState::Running => Self::Blink(Colour::Green, 1),
			RunState::Waiting => Self::Blink(Colour::Amber, 1),
			RunState::Failed => Self::Blink(Colour::Red, 3),
			RunState::Finished => Self::Steady(Colour::Green),
		}
	}
}

/// Show the state of the run on `led`, from the moment this starts, for as long as the run's state
/// is watched.
///
/// A change of pattern takes effect at once, at the start of the new one, wherever the old one was.
/// A change in the snapshot that leaves the run's state alone does not disturb a blink.
pub async fn drive<L: Led>(mut led: L, mut state: watch::Receiver<Snapshot>) {
	let mut failing = false;
	let mut run = state.borrow_and_update().run;
	loop {
		let pattern = Pattern::of(run);
		let mut changed = std::pin::pin!(next_run(&mut state, run));
		let next = match pattern {
			Pattern::Steady(colour) => {
				let (red, green) = colour.lit();
				show(&mut led, red, green, &mut failing).await;
				changed.await
			}
			Pattern::Blink(colour, hz) => {
				// Half of each cycle lit, then half dark.
				let half = Duration::from_secs(1) / (2 * hz);
				let (red, green) = colour.lit();
				let mut lit = true;
				let mut at = Instant::now();
				loop {
					if lit {
						show(&mut led, red, green, &mut failing).await;
					} else {
						show(&mut led, false, false, &mut failing).await;
					}
					at += half;
					lit = !lit;
					tokio::select! {
						next = &mut changed => break next,
						_ = sleep_until(at) => {}
					}
				}
			}
		};
		match next {
			Some(next) => run = next,
			None => return,
		}
	}
}

/// Resolve with the run's state when it is no longer `current`, or `None` once nothing can change it.
async fn next_run(state: &mut watch::Receiver<Snapshot>, current: RunState) -> Option<RunState> {
	loop {
		state.changed().await.ok()?;
		let run = state.borrow_and_update().run;
		if run != current {
			return Some(run);
		}
	}
}

/// Show a colour, saying once if the LEDs cannot be written and once when they can again.
async fn show<L: Led>(led: &mut L, red: bool, green: bool, failing: &mut bool) {
	match led.show(red, green).await {
		Ok(()) if *failing => {
			tracing::info!("writing the status LED again");
			*failing = false;
		}
		Ok(()) => {}
		Err(err) if !*failing => {
			tracing::warn!(%err, "cannot write the status LED");
			*failing = true;
		}
		Err(_) => {}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use super::*;

	const OFF: (bool, bool) = (false, false);
	const AMBER: (bool, bool) = (true, true);
	const GREEN_ON: (bool, bool) = (false, true);
	const RED_ON: (bool, bool) = (true, false);

	type Log = Arc<Mutex<Vec<(Duration, (bool, bool))>>>;

	struct Fake {
		log: Log,
		started: Instant,
		fail: Arc<Mutex<bool>>,
	}

	#[async_trait]
	impl Led for Fake {
		async fn show(&mut self, red: bool, green: bool) -> io::Result<()> {
			if *self.fail.lock().unwrap() {
				return Err(io::Error::other("no such LED"));
			}
			self.log
				.lock()
				.unwrap()
				.push((self.started.elapsed(), (red, green)));
			Ok(())
		}
	}

	struct Rig {
		log: Log,
		fail: Arc<Mutex<bool>>,
		state: watch::Sender<Snapshot>,
		driver: tokio::task::JoinHandle<()>,
	}

	fn snapshot(run: RunState) -> Snapshot {
		Snapshot {
			revision: 1,
			built: jiff::civil::date(2026, 10, 29),
			run,
			steps: Vec::new(),
			prompts: Vec::new(),
		}
	}

	impl Rig {
		fn new(run: RunState) -> Self {
			let log = Log::default();
			let fail = Arc::new(Mutex::new(false));
			let (state, rx) = watch::channel(snapshot(run));
			let driver = tokio::spawn(drive(
				Fake {
					log: log.clone(),
					started: Instant::now(),
					fail: fail.clone(),
				},
				rx,
			));
			Self {
				log,
				fail,
				state,
				driver,
			}
		}

		/// Let the driver run what it has been told, without time passing.
		async fn settle(&self) {
			for _ in 0..5 {
				tokio::task::yield_now().await;
			}
		}

		async fn change(&self, run: RunState) {
			self.state.send_replace(snapshot(run));
			self.settle().await;
		}

		/// What was shown so far, with how long after the start.
		fn shown(&self) -> Vec<(u64, (bool, bool))> {
			self.log
				.lock()
				.unwrap()
				.iter()
				.map(|(at, lit)| (at.as_millis() as u64, *lit))
				.collect()
		}
	}

	/// Let `by` pass, the driver waking at each instant it has set a timer for.
	async fn pass(by: Duration) {
		tokio::time::sleep(by).await;
	}

	const MS: Duration = Duration::from_millis(1);

	#[tokio::test(start_paused = true)]
	async fn idle_is_amber_from_the_start_and_steady() {
		let rig = Rig::new(RunState::Idle);
		rig.settle().await;
		assert_eq!(rig.shown(), [(0, AMBER)], "shown before time passes");
		pass(Duration::from_secs(30)).await;
		assert_eq!(rig.shown(), [(0, AMBER)]);
	}

	#[tokio::test(start_paused = true)]
	async fn running_is_green_blinking_once_a_second() {
		let rig = Rig::new(RunState::Running);
		pass(Duration::from_millis(2500) + MS).await;
		assert_eq!(
			rig.shown(),
			[
				(0, GREEN_ON),
				(500, OFF),
				(1000, GREEN_ON),
				(1500, OFF),
				(2000, GREEN_ON),
				(2500, OFF),
			]
		);
	}

	#[tokio::test(start_paused = true)]
	async fn waiting_is_amber_blinking_once_a_second() {
		let rig = Rig::new(RunState::Waiting);
		pass(Duration::from_millis(1500) + MS).await;
		assert_eq!(
			rig.shown(),
			[(0, AMBER), (500, OFF), (1000, AMBER), (1500, OFF)]
		);
	}

	#[tokio::test(start_paused = true)]
	async fn failed_is_red_blinking_three_times_a_second() {
		let rig = Rig::new(RunState::Failed);
		pass(Duration::from_secs(2) - MS).await;
		let shown = rig.shown();
		assert!(shown.iter().all(|(_, lit)| *lit == RED_ON || *lit == OFF));
		let lit: Vec<_> = shown.iter().filter(|(_, lit)| *lit == RED_ON).collect();
		assert_eq!(
			lit.len(),
			6,
			"three cycles a second for two seconds: {shown:?}"
		);
		for (i, (at, _)) in lit.iter().enumerate() {
			let expected = (i as u64 * 1000) / 3;
			assert!(
				at.abs_diff(expected) <= 1,
				"cycle {i} began at {at}, not {expected}"
			);
		}
		assert!(
			shown.windows(2).all(|pair| pair[0].1 != pair[1].1),
			"lit and dark alternate: {shown:?}"
		);
	}

	#[tokio::test(start_paused = true)]
	async fn finished_is_green_and_steady() {
		let rig = Rig::new(RunState::Finished);
		rig.settle().await;
		assert_eq!(rig.shown(), [(0, GREEN_ON)]);
		pass(Duration::from_secs(30)).await;
		assert_eq!(rig.shown(), [(0, GREEN_ON)]);
	}

	#[tokio::test(start_paused = true)]
	async fn a_change_while_lit_shows_the_new_pattern_at_once() {
		let rig = Rig::new(RunState::Running);
		pass(Duration::from_millis(200)).await;
		rig.change(RunState::Failed).await;
		assert_eq!(rig.shown(), [(0, GREEN_ON), (200, RED_ON)]);
		pass(Duration::from_millis(1000)).await;
		assert!(
			rig.shown()
				.iter()
				.skip(2)
				.all(|(_, lit)| *lit == OFF || *lit == RED_ON),
			"green after the change: {:?}",
			rig.shown()
		);
	}

	#[tokio::test(start_paused = true)]
	async fn a_change_while_dark_lights_the_new_pattern_without_waiting_out_the_dark() {
		let rig = Rig::new(RunState::Running);
		pass(Duration::from_millis(700)).await;
		assert_eq!(rig.shown().last(), Some(&(500, OFF)));
		rig.change(RunState::Finished).await;
		assert_eq!(rig.shown().last(), Some(&(700, GREEN_ON)));
		pass(Duration::from_secs(10)).await;
		assert_eq!(
			rig.shown().last(),
			Some(&(700, GREEN_ON)),
			"steady after the switch"
		);
	}

	#[tokio::test(start_paused = true)]
	async fn the_new_blink_keeps_its_own_time_from_the_change() {
		let rig = Rig::new(RunState::Running);
		pass(Duration::from_millis(700)).await;
		rig.change(RunState::Waiting).await;
		pass(Duration::from_millis(1000) + MS).await;
		let shown: Vec<_> = rig
			.shown()
			.into_iter()
			.skip_while(|(at, _)| *at < 700)
			.collect();
		assert_eq!(shown[..3], [(700, AMBER), (1200, OFF), (1700, AMBER)]);
	}

	#[tokio::test(start_paused = true)]
	async fn a_steady_state_changes_to_a_blink_and_back() {
		let rig = Rig::new(RunState::Idle);
		rig.settle().await;
		rig.change(RunState::Running).await;
		pass(Duration::from_millis(600)).await;
		rig.change(RunState::Idle).await;
		pass(Duration::from_secs(5)).await;
		assert_eq!(
			rig.shown(),
			[(0, AMBER), (0, GREEN_ON), (500, OFF), (600, AMBER)]
		);
	}

	#[tokio::test(start_paused = true)]
	async fn a_snapshot_that_leaves_the_state_alone_does_not_disturb_the_blink() {
		let rig = Rig::new(RunState::Running);
		pass(Duration::from_millis(300)).await;
		rig.state.send_modify(|snapshot| snapshot.prompts.clear());
		rig.settle().await;
		pass(Duration::from_millis(1000)).await;
		assert_eq!(rig.shown(), [(0, GREEN_ON), (500, OFF), (1000, GREEN_ON)]);
	}

	#[tokio::test(start_paused = true)]
	async fn the_driver_ends_when_nothing_can_change_the_state_any_more() {
		let rig = Rig::new(RunState::Idle);
		rig.settle().await;
		drop(rig.state);
		rig.driver.await.expect("ended cleanly");
	}

	#[tokio::test(start_paused = true)]
	async fn an_led_that_cannot_be_written_does_not_stop_the_driver() {
		let rig = Rig::new(RunState::Running);
		*rig.fail.lock().unwrap() = true;
		pass(Duration::from_millis(1700)).await;
		*rig.fail.lock().unwrap() = false;
		pass(Duration::from_millis(500)).await;
		let shown = rig.shown();
		assert!(shown.iter().any(|(at, _)| *at >= 2000), "{shown:?}");
		assert!(!rig.driver.is_finished());
	}

	#[test]
	fn every_state_has_a_pattern_of_the_table() {
		assert_eq!(Pattern::of(RunState::Idle), Pattern::Steady(Colour::Amber));
		assert_eq!(
			Pattern::of(RunState::Running),
			Pattern::Blink(Colour::Green, 1)
		);
		assert_eq!(
			Pattern::of(RunState::Waiting),
			Pattern::Blink(Colour::Amber, 1)
		);
		assert_eq!(
			Pattern::of(RunState::Failed),
			Pattern::Blink(Colour::Red, 3)
		);
		assert_eq!(
			Pattern::of(RunState::Finished),
			Pattern::Steady(Colour::Green)
		);
	}

	mod sysfs {
		use super::*;

		fn leds(names: &[&str]) -> tempfile::TempDir {
			let root = tempfile::tempdir().unwrap();
			for name in names {
				let dir = root.path().join(name);
				std::fs::create_dir(&dir).unwrap();
				std::fs::write(dir.join("trigger"), "none [heartbeat] mmc0\n").unwrap();
				std::fs::write(dir.join("brightness"), "0").unwrap();
			}
			root
		}

		fn read(root: &Path, name: &str, file: &str) -> String {
			std::fs::read_to_string(root.join(name).join(file)).unwrap()
		}

		#[tokio::test]
		async fn the_triggers_are_set_to_none_and_the_green_led_is_active_low() {
			let root = leds(&["ACT", "PWR"]);
			let mut sysfs = Sysfs::at(root.path()).unwrap();
			assert_eq!(read(root.path(), "ACT", "trigger"), "none");
			assert_eq!(read(root.path(), "PWR", "trigger"), "none");

			sysfs.show(true, false).await.unwrap();
			assert_eq!(read(root.path(), "PWR", "brightness"), "1");
			assert_eq!(read(root.path(), "ACT", "brightness"), "1");

			sysfs.show(false, true).await.unwrap();
			assert_eq!(read(root.path(), "PWR", "brightness"), "0");
			assert_eq!(read(root.path(), "ACT", "brightness"), "0");
		}

		#[test]
		fn older_kernels_name_the_leds_by_number() {
			let root = leds(&["led0", "led1"]);
			Sysfs::at(root.path()).unwrap();
			assert_eq!(read(root.path(), "led0", "trigger"), "none");
			assert_eq!(read(root.path(), "led1", "trigger"), "none");
		}

		#[test]
		fn a_board_with_no_such_led_is_an_error() {
			let root = leds(&["ACT"]);
			let err = Sysfs::at(root.path()).err().expect("no red LED");
			assert_eq!(err.kind(), io::ErrorKind::NotFound);
		}
	}
}
