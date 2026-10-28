//! What a device in provisioning mode reports of its run: the states, each step's report, and each
//! open prompt (RUN, PRS).
//!
//! These are the members `provision-state` carries, defined here because the device builds them and
//! the browser client reads them.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use serde_json::{Map, Value as Json};

macro_rules! names {
	($(#[$meta:meta])* $name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
		$(#[$meta])*
		#[derive(Debug, Clone, Copy, PartialEq, Eq)]
		pub enum $name {
			$(
				#[doc = concat!("`", $wire, "`.")]
				$variant,
			)+
		}

		impl $name {
			/// Every state, in the order the spec lists them.
			pub const ALL: &'static [Self] = &[$(Self::$variant),+];

			/// The name this has on the wire.
			pub fn as_str(self) -> &'static str {
				match self {
					$(Self::$variant => $wire,)+
				}
			}

			/// Read the name a state has on the wire.
			pub fn read(name: &str) -> Option<Self> {
				match name {
					$($wire => Some(Self::$variant),)+
					_ => None,
				}
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				f.write_str(self.as_str())
			}
		}

		impl Serialize for $name {
			fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
				serializer.serialize_str(self.as_str())
			}
		}

		impl<'de> Deserialize<'de> for $name {
			fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
				let name = String::deserialize(deserializer)?;
				Self::read(&name).ok_or_else(|| {
					de::Error::custom(format!(
						concat!("{:?} is not a ", stringify!($name)),
						name
					))
				})
			}
		}
	};
}

names! {
	/// The state of the device as a whole (RUN, "Run states").
	RunState {
		Idle => "idle",
		Running => "running",
		Waiting => "waiting",
		Failed => "failed",
		Finished => "finished",
	}
}

names! {
	/// What a step says of whether the board already holds what it would produce (RUN, "Done-state").
	DoneState {
		Done => "done",
		NotDone => "not-done",
		Unknown => "unknown",
	}
}

names! {
	/// The state of one step (RUN, "Run states").
	StepState {
		Unchosen => "unchosen",
		Pending => "pending",
		Running => "running",
		Waiting => "waiting",
		Passed => "passed",
		Failed => "failed",
	}
}

/// What a prompt asks for (RUN, "Prompts").
///
/// A kind this build does not know is kept by name, because a client shows the text of such a
/// prompt and does not answer it (PRS, "The state").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
	/// Asks an operator to do something, and closes when the step sees it done.
	Notice,
	/// Asks an operator to confirm, answered with true or false.
	Confirm,
	/// Asks an operator for text.
	Text,
	/// Asks an operator for text that is not to be shown or kept.
	Secret,
	/// A kind a newer device defines.
	Other(String),
}

impl PromptKind {
	/// The name this has on the wire.
	pub fn as_str(&self) -> &str {
		match self {
			Self::Notice => "notice",
			Self::Confirm => "confirm",
			Self::Text => "text",
			Self::Secret => "secret",
			Self::Other(name) => name,
		}
	}

	/// Read the name a kind has on the wire.
	pub fn read(name: &str) -> Self {
		match name {
			"notice" => Self::Notice,
			"confirm" => Self::Confirm,
			"text" => Self::Text,
			"secret" => Self::Secret,
			other => Self::Other(other.to_owned()),
		}
	}
}

impl Serialize for PromptKind {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(self.as_str())
	}
}

impl<'de> Deserialize<'de> for PromptKind {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		Ok(Self::read(&String::deserialize(deserializer)?))
	}
}

/// One step as `provision-state` reports it (PRS, "The state").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepReport {
	/// Names the step, in lower case letters, digits and hyphens.
	pub id: String,
	/// The step's name for an operator.
	pub title: String,
	/// Whether the board already holds what the step would produce.
	pub done: DoneState,
	/// The reason for the done-state.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub why: Option<String>,
	/// The step's state.
	pub state: StepState,
	/// The fraction of the step's work done, from 0 to 1.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub progress: Option<f64>,
	/// Why the step failed, where its state is `failed`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub reason: Option<String>,
	/// What the step found or produced, whose members the spec defining the step gives.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub result: Option<Map<String, Json>>,
}

/// One open prompt as `provision-state` reports it (PRS, "The state").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptReport {
	/// Names the prompt among those open.
	pub id: String,
	/// The id of the step that asked.
	pub step: String,
	/// What the prompt asks for.
	pub kind: PromptKind,
	/// What to tell the operator.
	pub text: String,
	/// Whether the step waits for the prompt.
	pub blocking: bool,
}

/// The answer a client gives to a prompt: a boolean for `confirm`, a string for `text` and `secret`.
///
/// Kept as the JSON it arrived as, so a device can refuse one of the wrong type at the member that
/// carries it rather than faulting the stream (PRS, "The acts"). Its [`fmt::Debug`] says nothing of
/// what it holds, because the answer to a `secret` prompt must not reach a log.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Answer(Json);

impl Answer {
	/// The answer to a `confirm` prompt.
	pub fn boolean(value: bool) -> Self {
		Self(Json::Bool(value))
	}

	/// The answer to a `text` or `secret` prompt.
	pub fn text(value: impl Into<String>) -> Self {
		Self(Json::String(value.into()))
	}

	/// Hold whatever JSON arrived.
	pub fn from_json(value: Json) -> Self {
		Self(value)
	}

	/// The answer as a boolean, where it is one.
	pub fn as_bool(&self) -> Option<bool> {
		self.0.as_bool()
	}

	/// The answer as text, where it is some.
	pub fn as_text(&self) -> Option<&str> {
		self.0.as_str()
	}
}

impl fmt::Debug for Answer {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("Answer(..)")
	}
}
