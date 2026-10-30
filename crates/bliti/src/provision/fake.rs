//! Steps that do what a test scripts, standing in for the real ones.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bliti_core::provisioning::report::{DoneState, PromptKind};
use serde_json::{Map, Value as Json};
use tokio::sync::Notify;

use super::{
	context::Context,
	model::Question,
	step::{Done, Step},
};

/// The date the image on a test's medium was built.
pub const BUILT: jiff::civil::Date = jiff::civil::date(2026, 10, 29);

/// One thing a fake step does, in order.
#[derive(Clone)]
pub enum Do {
	Progress(f64),
	Result(&'static str),
	Confirm(&'static str),
	Text(&'static str),
	Secret(&'static str),
	AskLater(&'static str, PromptKind),
	/// Take the answer under the key, or the default where there is none, logging what was taken.
	TakeText(&'static str, Option<&'static str>),
	/// Forget the answer held under the key.
	Forget(&'static str),
	/// Hold a notice open until the gate is opened.
	Notice(&'static str, Arc<Gate>),
	/// Wait until the gate is opened.
	Hold(Arc<Gate>),
	/// Fail for this reason, on the attempts before the one named (counting from 1), and pass after.
	FailUntil(u32, &'static str),
}

/// A point a test lets a step through.
#[derive(Default)]
pub struct Gate {
	open: Notify,
}

impl Gate {
	pub fn new() -> Arc<Self> {
		Arc::default()
	}

	pub fn open(&self) {
		self.open.notify_one();
	}
}

/// What the steps did, in the order they did it.
#[derive(Clone, Default)]
pub struct Log(Arc<Mutex<Vec<String>>>);

impl Log {
	pub fn push(&self, line: String) {
		self.0.lock().unwrap().push(line);
	}

	pub fn lines(&self) -> Vec<String> {
		self.0.lock().unwrap().clone()
	}
}

pub struct Fake {
	id: &'static str,
	script: Vec<Do>,
	done: Mutex<Done>,
	log: Log,
	attempts: Mutex<u32>,
	ahead: Mutex<Option<(&'static str, Question)>>,
}

impl Fake {
	pub fn new(id: &'static str, log: &Log, script: Vec<Do>) -> Arc<Self> {
		Arc::new(Self {
			id,
			script,
			done: Mutex::new(Done {
				state: DoneState::NotDone,
				why: None,
			}),
			log: log.clone(),
			attempts: Mutex::new(0),
			ahead: Mutex::new(None),
		})
	}

	/// Have this step ask `question` ahead of its turn, once the step `after` has passed.
	pub fn asks_ahead(&self, after: &'static str, question: Question) {
		*self.ahead.lock().unwrap() = Some((after, question));
	}

	pub fn done_is(&self, state: DoneState, why: Option<&str>) {
		*self.done.lock().unwrap() = Done {
			state,
			why: why.map(str::to_owned),
		};
	}
}

#[async_trait]
impl Step for Fake {
	fn id(&self) -> &str {
		self.id
	}

	fn title(&self) -> &str {
		self.id
	}

	async fn done(&self) -> Done {
		self.done.lock().unwrap().clone()
	}

	fn asks_after(&self) -> Option<&str> {
		self.ahead.lock().unwrap().as_ref().map(|(after, _)| *after)
	}

	async fn ask_ahead(&self, context: &mut Context) {
		let ahead = self.ahead.lock().unwrap().clone();
		if let Some((_, question)) = ahead {
			context.ask_later(question);
		}
	}

	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let attempt = {
			let mut attempts = self.attempts.lock().unwrap();
			*attempts += 1;
			*attempts
		};
		self.log.push(format!("{} begins", self.id));
		for action in &self.script {
			match action {
				Do::Progress(fraction) => context.progress(*fraction),
				Do::Result(text) => {
					let mut result = Map::new();
					result.insert("found".to_owned(), Json::String((*text).to_owned()));
					context.result(result);
				}
				Do::Confirm(key) => {
					let answer = context.confirm(key, "Sure?").await;
					self.log.push(format!("{} confirmed {answer}", self.id));
				}
				Do::Text(key) => {
					let answer = context.text(key, "Name?").await;
					self.log.push(format!("{} text {answer}", self.id));
				}
				Do::Secret(key) => {
					let answer = context.secret(key, "Password?").await;
					self.log.push(format!("{} secret {answer}", self.id));
				}
				Do::TakeText(key, default) => {
					let taken = context.take_text(key, *default);
					self.log.push(format!("{} took {taken:?}", self.id));
				}
				Do::Forget(key) => context.forget(key),
				Do::AskLater(key, kind) => {
					context.ask_later(Question::new(key, kind.clone(), "Later?"))
				}
				Do::Notice(text, gate) => {
					let notice = context.notice("notice", text);
					gate.open.notified().await;
					notice.close();
				}
				Do::Hold(gate) => gate.open.notified().await,
				Do::FailUntil(passes_on, reason) if attempt < *passes_on => {
					return Err((*reason).to_owned());
				}
				Do::FailUntil(..) => {}
			}
		}
		self.log.push(format!("{} ends", self.id));
		Ok(())
	}
}
