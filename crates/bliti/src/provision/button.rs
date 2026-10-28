//! The USER button: one event for each press (PRV, "Going on the air").

use std::{io, time::Duration};

use async_trait::async_trait;

use crate::gpio;

/// The line the USER button is on, by the name the kernel gives it on the pin header.
const USER_LINE: &str = "GPIO27";

/// How often the line is read.
const POLL: Duration = Duration::from_millis(20);

/// How many reads in a row must agree before the line is taken to have changed, so contact bounce
/// is not a press and a held button is not many.
const STABLE: u32 = 3;

/// An input that is pressed.
#[async_trait]
pub trait Button: Send {
	/// Resolve when the button is next pressed. A button held down is one press, however long.
	async fn pressed(&mut self);
}

/// One line the button's level can be read from.
#[async_trait]
pub trait Line: Send {
	/// Whether the line is high.
	async fn is_high(&mut self) -> io::Result<bool>;
}

/// A line read by the name the kernel gives it, off the async runtime.
pub struct Named(&'static str);

#[async_trait]
impl Line for Named {
	async fn is_high(&mut self) -> io::Result<bool> {
		let name = self.0;
		tokio::task::spawn_blocking(move || gpio::read_by_name(name))
			.await
			.map_err(io::Error::other)?
	}
}

/// The USER button of the board.
pub fn user() -> Polled<Named> {
	Polled::new(Named(USER_LINE))
}

/// A button found by reading its line at intervals, which reads low while the button is held.
pub struct Polled<L> {
	line: L,
	debounce: Debounce,
	failing: bool,
}

impl<L: Line> Polled<L> {
	pub fn new(line: L) -> Self {
		Self {
			line,
			debounce: Debounce::new(),
			failing: false,
		}
	}
}

#[async_trait]
impl<L: Line> Button for Polled<L> {
	async fn pressed(&mut self) {
		loop {
			tokio::time::sleep(POLL).await;
			match self.line.is_high().await {
				Ok(high) => {
					if self.failing {
						tracing::info!("reading the USER button again");
						self.failing = false;
					}
					if self.debounce.sample(!high) {
						return;
					}
				}
				// A line that cannot be read is retried for as long as the daemon runs, since a device
				// that cannot be put on the air is unreachable. Said once per failure, not on every read.
				Err(err) => {
					if !self.failing {
						tracing::warn!(%err, "cannot read the USER button; retrying");
						self.failing = true;
					}
				}
			}
		}
	}
}

/// Settles a stream of reads into presses.
struct Debounce {
	/// What the line has settled at: held, or not.
	held: bool,
	/// How many reads in a row have disagreed with that.
	disagreeing: u32,
}

impl Debounce {
	/// Starts out taking the button as held, so one that is down as the daemon starts is not a press:
	/// it counts once it has been let go and pressed again.
	fn new() -> Self {
		Self {
			held: true,
			disagreeing: 0,
		}
	}

	/// Take a read, `true` if the button was found held, and say whether it makes a new press.
	fn sample(&mut self, held: bool) -> bool {
		if held == self.held {
			self.disagreeing = 0;
			return false;
		}
		self.disagreeing += 1;
		if self.disagreeing < STABLE {
			return false;
		}
		self.held = held;
		self.disagreeing = 0;
		held
	}
}

/// A button pressed from a test.
#[cfg(test)]
pub struct Fake(tokio::sync::mpsc::UnboundedReceiver<()>);

#[cfg(test)]
impl Fake {
	/// A button, and what presses it.
	pub fn new() -> (Self, Presser) {
		let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
		(Self(rx), Presser(tx))
	}
}

/// Presses a [`Fake`].
#[cfg(test)]
#[derive(Clone)]
pub struct Presser(tokio::sync::mpsc::UnboundedSender<()>);

#[cfg(test)]
impl Presser {
	pub fn press(&self) {
		let _ = self.0.send(());
	}
}

#[cfg(test)]
#[async_trait]
impl Button for Fake {
	async fn pressed(&mut self) {
		match self.0.recv().await {
			Some(()) => {}
			None => std::future::pending().await,
		}
	}
}

#[cfg(test)]
mod tests {
	use std::{
		collections::VecDeque,
		sync::{Arc, Mutex},
	};

	use super::*;

	/// A line that reads what it is given, and then stays high.
	struct Script(Arc<Mutex<VecDeque<io::Result<bool>>>>);

	impl Script {
		fn new(reads: impl IntoIterator<Item = io::Result<bool>>) -> Self {
			Self(Arc::new(Mutex::new(reads.into_iter().collect())))
		}
	}

	#[async_trait]
	impl Line for Script {
		async fn is_high(&mut self) -> io::Result<bool> {
			self.0.lock().unwrap().pop_front().unwrap_or(Ok(true))
		}
	}

	fn low(count: usize) -> impl Iterator<Item = io::Result<bool>> {
		std::iter::repeat_with(|| Ok(false)).take(count)
	}

	fn high(count: usize) -> impl Iterator<Item = io::Result<bool>> {
		std::iter::repeat_with(|| Ok(true)).take(count)
	}

	/// How long a read takes to be found pending, which the paused clock reaches by auto-advance.
	async fn is_pending(button: &mut Polled<Script>) -> bool {
		tokio::time::timeout(Duration::from_secs(1), button.pressed())
			.await
			.is_err()
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_is_found_once_the_line_has_stayed_low() {
		let line = Script::new(high(5).chain(low(STABLE as usize)));
		let mut button = Polled::new(line);
		tokio::time::timeout(Duration::from_secs(1), button.pressed())
			.await
			.expect("pressed");
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_held_down_is_one_press() {
		let line = Script::new(high(5).chain(low(200)));
		let mut button = Polled::new(line);
		button.pressed().await;
		assert!(is_pending(&mut button).await, "a held button pressed again");
	}

	#[tokio::test(start_paused = true)]
	async fn a_press_after_a_release_is_another() {
		let line = Script::new(high(5).chain(low(10)).chain(high(10)).chain(low(10)));
		let mut button = Polled::new(line);
		button.pressed().await;
		button.pressed().await;
	}

	#[tokio::test(start_paused = true)]
	async fn bounce_is_not_a_press() {
		let bounce = (0..20).flat_map(|_| low(STABLE as usize - 1).chain(high(1)));
		let line = Script::new(high(5).chain(bounce));
		let mut button = Polled::new(line);
		assert!(is_pending(&mut button).await, "contact bounce was a press");
	}

	#[tokio::test(start_paused = true)]
	async fn bounce_on_release_is_not_a_second_press() {
		let bounce = (0..20).flat_map(|_| high(STABLE as usize - 1).chain(low(1)));
		let line = Script::new(high(5).chain(low(10)).chain(bounce));
		let mut button = Polled::new(line);
		button.pressed().await;
		assert!(
			is_pending(&mut button).await,
			"bounce on release was a press"
		);
	}

	#[tokio::test(start_paused = true)]
	async fn a_button_held_as_the_daemon_starts_is_not_a_press() {
		let line = Script::new(low(50).chain(high(10)));
		let mut button = Polled::new(line);
		assert!(is_pending(&mut button).await);
	}

	#[tokio::test(start_paused = true)]
	async fn a_line_that_cannot_be_read_is_retried() {
		let failure = || Err(io::Error::other("no such line"));
		let line = Script::new(
			high(5)
				.chain([failure(), failure(), failure()])
				.chain(low(STABLE as usize)),
		);
		let mut button = Polled::new(line);
		tokio::time::timeout(Duration::from_secs(1), button.pressed())
			.await
			.expect("pressed once the line could be read again");
	}

	#[tokio::test(start_paused = true)]
	async fn a_failed_read_does_not_break_a_run_of_agreeing_reads() {
		let line = Script::new(
			high(5)
				.chain(low(STABLE as usize - 1))
				.chain([Err(io::Error::other("busy"))])
				.chain(low(1)),
		);
		let mut button = Polled::new(line);
		tokio::time::timeout(Duration::from_secs(1), button.pressed())
			.await
			.expect("pressed");
	}

	#[tokio::test]
	async fn the_fake_presses_when_told() {
		let (mut button, presser) = Fake::new();
		presser.press();
		button.pressed().await;
	}
}
