//! The window in which a device in provisioning mode is on the air (PRV, "Going on the air").

use std::time::Duration;

use async_trait::async_trait;
use tokio::{
	sync::mpsc,
	time::{Instant, sleep_until},
};

use super::button::Button;

/// How long a press of the USER button puts the device on the air for.
pub const WINDOW: Duration = Duration::from_secs(5 * 60);

/// How long to wait before trying a failed registration again, and the ceiling that wait backs off
/// to.
const RETRY: Duration = Duration::from_secs(1);
const RETRY_MAX: Duration = Duration::from_secs(30);

/// What moves the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
	/// The USER button was pressed.
	Press,
	/// Time has passed. The window ends if its time is up, and ignores the event if it is not.
	Timer,
	/// A client connected.
	Connect,
	/// The client that connected has gone.
	Disconnect,
}

/// What the device is to do about the air, after an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
	/// Be advertising.
	Advertise,
	/// Not be advertising.
	Withdraw,
	/// Nothing has changed.
	Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
	/// Not advertising, with no client connected.
	Silent,
	/// Advertising, until the instant.
	Open(Instant),
	/// A client is connected, and nothing is advertised.
	Connected,
}

/// Where the device is in the cycle of going on the air, a press at a time.
#[derive(Debug)]
pub struct Window {
	state: State,
}

impl Window {
	/// A window as the device starts: silent, whatever a run it resumes is doing (PRV).
	pub fn new() -> Self {
		Self {
			state: State::Silent,
		}
	}

	/// Whether the device is to be advertising.
	#[cfg(test)]
	fn is_open(&self) -> bool {
		matches!(self.state, State::Open(_))
	}

	/// Move on an event, as of now.
	pub fn handle(&mut self, event: Event) -> Action {
		match (self.state, event) {
			// spec: PRV a press opens the window, or restarts it when it is open
			(State::Silent | State::Open(_), Event::Press) => {
				self.state = State::Open(Instant::now() + WINDOW);
				Action::Advertise
			}
			// spec: PRV a press while a client is connected is ignored
			(State::Connected, Event::Press) => Action::Nothing,

			(State::Open(until), Event::Timer) if Instant::now() >= until => {
				self.state = State::Silent;
				Action::Withdraw
			}

			// spec: PRV a connection ends the window
			(State::Open(_), Event::Connect) => {
				self.state = State::Connected;
				Action::Withdraw
			}
			(State::Silent, Event::Connect) => {
				self.state = State::Connected;
				Action::Nothing
			}

			// spec: PRV nothing resumes advertising until the next press
			(State::Connected, Event::Disconnect) => {
				self.state = State::Silent;
				Action::Nothing
			}

			(State::Open(_), Event::Timer)
			| (State::Silent | State::Connected, Event::Timer)
			| (State::Connected, Event::Connect)
			| (State::Silent | State::Open(_), Event::Disconnect) => Action::Nothing,
		}
	}

	/// Resolve when the window's time is up, never where it is not open.
	pub async fn timer(&self) {
		match self.state {
			State::Open(until) => sleep_until(until).await,
			State::Silent | State::Connected => std::future::pending().await,
		}
	}
}

/// What the device puts itself on the air with.
#[async_trait]
pub trait Air: Send {
	/// Start advertising.
	async fn on(&mut self) -> Result<(), String>;

	/// Stop advertising.
	async fn off(&mut self);
}

/// Put the device on the air as the USER button and the clients' comings and goings say, until
/// `events` is closed.
///
/// Registering with the radio can fail, and a device that cannot be put on the air cannot be reached
/// at all, so a failure is retried for as long as the window is open.
pub async fn keep<B, A>(mut button: B, mut air: A, mut events: mpsc::UnboundedReceiver<Event>)
where
	B: Button,
	A: Air,
{
	let mut window = Window::new();
	let mut registered = false;
	let mut retry: Option<Instant> = None;
	let mut backoff = RETRY;

	loop {
		let action = tokio::select! {
			_ = button.pressed() => window.handle(Event::Press),
			event = events.recv() => match event {
				Some(event) => window.handle(event),
				None => break,
			},
			_ = window.timer() => window.handle(Event::Timer),
			_ = async {
				match retry {
					Some(at) => sleep_until(at).await,
					None => std::future::pending().await,
				}
			} => Action::Advertise,
		};

		match action {
			// Registered afresh on every press: a client that connected and left without opening a
			// session stopped a controller that does not resume by itself, and nothing else tells the
			// device (ADV).
			Action::Advertise => {
				if registered {
					air.off().await;
					registered = false;
				}
				match air.on().await {
					Ok(()) => {
						registered = true;
						retry = None;
						backoff = RETRY;
					}
					Err(err) => {
						tracing::error!(%err, ?backoff, "could not register the advertisement; retrying");
						retry = Some(Instant::now() + backoff);
						backoff = (backoff * 2).min(RETRY_MAX);
					}
				}
			}
			Action::Nothing => {}
			Action::Withdraw => {
				retry = None;
				backoff = RETRY;
				if registered {
					air.off().await;
					registered = false;
				}
			}
		}
	}
	if registered {
		air.off().await;
	}
}

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use super::*;
	use crate::provision::button::{Fake, Presser};

	const MINUTE: Duration = Duration::from_secs(60);

	async fn advance(by: Duration) {
		tokio::time::advance(by).await;
	}

	#[tokio::test(start_paused = true)]
	async fn the_device_starts_silent() {
		let mut window = Window::new();
		assert!(!window.is_open());
		advance(10 * MINUTE).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		assert!(!window.is_open());
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_opens_a_window_of_five_minutes() {
		let mut window = Window::new();
		assert_eq!(window.handle(Event::Press), Action::Advertise);
		assert!(window.is_open());

		advance(WINDOW - Duration::from_secs(1)).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		assert!(window.is_open(), "closed with a second to go");

		advance(Duration::from_secs(1)).await;
		assert_eq!(window.handle(Event::Timer), Action::Withdraw);
		assert!(!window.is_open());
	}

	#[tokio::test(start_paused = true)]
	async fn the_timer_resolves_when_the_window_is_up() {
		let mut window = Window::new();
		window.handle(Event::Press);
		let started = Instant::now();
		window.timer().await;
		assert_eq!(started.elapsed(), WINDOW);
	}

	#[tokio::test(start_paused = true)]
	async fn the_timer_never_resolves_while_not_open() {
		let mut window = Window::new();
		let waited = tokio::time::timeout(10 * MINUTE, window.timer()).await;
		assert!(waited.is_err());

		window.handle(Event::Press);
		window.handle(Event::Connect);
		let waited = tokio::time::timeout(10 * MINUTE, window.timer()).await;
		assert!(waited.is_err());
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_while_open_restarts_the_window() {
		let mut window = Window::new();
		window.handle(Event::Press);
		advance(3 * MINUTE).await;
		assert_eq!(window.handle(Event::Press), Action::Advertise);

		// Five minutes after the first press would have been the end.
		advance(2 * MINUTE).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		assert!(window.is_open());

		advance(3 * MINUTE).await;
		assert_eq!(window.handle(Event::Timer), Action::Withdraw);
	}

	#[tokio::test(start_paused = true)]
	async fn a_connection_ends_the_window() {
		let mut window = Window::new();
		window.handle(Event::Press);
		assert_eq!(window.handle(Event::Connect), Action::Withdraw);
		assert!(!window.is_open());

		advance(10 * MINUTE).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		assert!(!window.is_open());
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_while_a_client_is_connected_is_ignored() {
		let mut window = Window::new();
		window.handle(Event::Press);
		window.handle(Event::Connect);
		assert_eq!(window.handle(Event::Press), Action::Nothing);
		assert!(!window.is_open());
	}

	#[tokio::test(start_paused = true)]
	async fn nothing_resumes_advertising_after_the_client_has_gone() {
		let mut window = Window::new();
		window.handle(Event::Press);
		window.handle(Event::Connect);
		assert_eq!(window.handle(Event::Disconnect), Action::Nothing);
		assert!(!window.is_open());

		advance(10 * MINUTE).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		assert!(!window.is_open());

		assert_eq!(window.handle(Event::Press), Action::Advertise);
		assert!(window.is_open());
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_after_a_client_has_gone_opens_a_whole_window() {
		let mut window = Window::new();
		window.handle(Event::Press);
		advance(4 * MINUTE).await;
		window.handle(Event::Connect);
		window.handle(Event::Disconnect);
		advance(10 * MINUTE).await;

		window.handle(Event::Press);
		advance(WINDOW - Duration::from_secs(1)).await;
		assert_eq!(window.handle(Event::Timer), Action::Nothing);
		advance(Duration::from_secs(1)).await;
		assert_eq!(window.handle(Event::Timer), Action::Withdraw);
	}

	#[tokio::test(start_paused = true)]
	async fn a_client_connected_while_silent_keeps_presses_out() {
		let mut window = Window::new();
		assert_eq!(window.handle(Event::Connect), Action::Nothing);
		assert_eq!(window.handle(Event::Press), Action::Nothing);
		assert!(!window.is_open());
		window.handle(Event::Disconnect);
		assert_eq!(window.handle(Event::Press), Action::Advertise);
	}

	#[tokio::test(start_paused = true)]
	async fn a_disconnect_with_no_client_connected_changes_nothing() {
		let mut window = Window::new();
		assert_eq!(window.handle(Event::Disconnect), Action::Nothing);
		window.handle(Event::Press);
		assert_eq!(window.handle(Event::Disconnect), Action::Nothing);
		assert!(window.is_open(), "a stray disconnect closed the window");
	}

	#[derive(Debug, Clone, PartialEq, Eq)]
	enum Call {
		On,
		Off,
	}

	/// An air that records what it is asked, and fails as many registrations as it is told to.
	/// Let time pass, the driver waking at each timer it has set.
	async fn wait(by: Duration) {
		tokio::time::sleep(by).await;
	}

	struct FakeAir {
		calls: Arc<Mutex<Vec<(Call, Instant)>>>,
		failures: usize,
	}

	#[async_trait]
	impl Air for FakeAir {
		async fn on(&mut self) -> Result<(), String> {
			self.calls.lock().unwrap().push((Call::On, Instant::now()));
			match self.failures.checked_sub(1) {
				Some(left) => {
					self.failures = left;
					Err("busy".into())
				}
				None => Ok(()),
			}
		}

		async fn off(&mut self) {
			self.calls.lock().unwrap().push((Call::Off, Instant::now()));
		}
	}

	struct Rig {
		presser: Presser,
		events: mpsc::UnboundedSender<Event>,
		calls: Arc<Mutex<Vec<(Call, Instant)>>>,
		started: Instant,
	}

	impl Rig {
		fn new(failures: usize) -> Self {
			let (button, presser) = Fake::new();
			let (events, rx) = mpsc::unbounded_channel();
			let calls = Arc::new(Mutex::new(Vec::new()));
			tokio::spawn(keep(
				button,
				FakeAir {
					calls: calls.clone(),
					failures,
				},
				rx,
			));
			Self {
				presser,
				events,
				calls,
				started: Instant::now(),
			}
		}

		/// Let the driver take what it has been sent.
		async fn settle(&self) {
			tokio::time::sleep(Duration::from_millis(1)).await;
		}

		fn calls(&self) -> Vec<Call> {
			self.calls
				.lock()
				.unwrap()
				.iter()
				.map(|(call, _)| call.clone())
				.collect()
		}
	}

	#[tokio::test(start_paused = true)]
	async fn the_device_is_not_on_the_air_until_pressed() {
		let rig = Rig::new(0);
		wait(10 * MINUTE).await;
		rig.settle().await;
		assert_eq!(rig.calls(), []);
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_puts_the_device_on_the_air_for_a_window() {
		let rig = Rig::new(0);
		rig.presser.press();
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On]);

		wait(WINDOW).await;
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On, Call::Off]);
		let at = rig.calls.lock().unwrap()[1].1;
		assert_eq!(at - rig.started, WINDOW);
	}

	#[tokio::test(start_paused = true)]
	async fn a_second_press_restarts_the_window_and_registers_again() {
		let rig = Rig::new(0);
		rig.presser.press();
		rig.settle().await;
		wait(3 * MINUTE).await;
		rig.presser.press();
		rig.settle().await;
		wait(3 * MINUTE).await;
		rig.settle().await;
		assert_eq!(
			rig.calls(),
			[Call::On, Call::Off, Call::On],
			"went off the air at the first window's end"
		);

		wait(2 * MINUTE).await;
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On, Call::Off, Call::On, Call::Off]);
	}

	#[tokio::test(start_paused = true)]
	async fn a_connection_takes_the_device_off_the_air_and_it_stays_off() {
		let rig = Rig::new(0);
		rig.presser.press();
		rig.settle().await;
		rig.events.send(Event::Connect).unwrap();
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On, Call::Off]);

		rig.presser.press();
		rig.settle().await;
		assert_eq!(
			rig.calls(),
			[Call::On, Call::Off],
			"a press with a client connected"
		);

		rig.events.send(Event::Disconnect).unwrap();
		wait(10 * MINUTE).await;
		rig.settle().await;
		assert_eq!(
			rig.calls(),
			[Call::On, Call::Off],
			"resumed with nobody pressing"
		);

		rig.presser.press();
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On, Call::Off, Call::On]);
	}

	#[tokio::test(start_paused = true)]
	async fn a_failed_registration_is_retried_with_backoff_while_the_window_is_open() {
		let rig = Rig::new(2);
		rig.presser.press();
		rig.settle().await;
		wait(Duration::from_secs(10)).await;
		rig.settle().await;
		assert_eq!(rig.calls(), [Call::On, Call::On, Call::On]);

		let times: Vec<_> = rig
			.calls
			.lock()
			.unwrap()
			.iter()
			.map(|(_, at)| (*at - rig.started).as_secs())
			.collect();
		assert_eq!(times, [0, 1, 3], "retries at one second, then two");
	}

	#[tokio::test(start_paused = true)]
	async fn a_registration_that_never_succeeds_stops_being_tried_when_the_window_ends() {
		let rig = Rig::new(usize::MAX);
		rig.presser.press();
		rig.settle().await;
		wait(WINDOW).await;
		rig.settle().await;
		let tried = rig.calls().len();
		wait(10 * MINUTE).await;
		rig.settle().await;
		assert_eq!(rig.calls().len(), tried);
		assert!(
			!rig.calls().contains(&Call::Off),
			"withdrew what was never registered"
		);
	}

	#[tokio::test(start_paused = true)]
	async fn the_registration_is_dropped_when_the_driver_is_told_to_stop() {
		let rig = Rig::new(0);
		rig.presser.press();
		rig.settle().await;
		drop(rig.events);
		tokio::time::sleep(Duration::from_millis(1)).await;
		assert_eq!(
			*rig.calls
				.lock()
				.unwrap()
				.iter()
				.map(|(c, _)| c.clone())
				.collect::<Vec<_>>(),
			[Call::On, Call::Off]
		);
	}
}
