//! The step `tailscale`: sign the Iti in to the operator's tailnet from the provisioning system,
//! against the Tailscale state of the system on the disk (PTS).

use std::{fs, io, panic::AssertUnwindSafe, sync::Arc};

use async_trait::async_trait;
use bliti_core::{
	channel::messages::Secret,
	provisioning::{Suffix, report::DoneState, report::PromptKind},
};
use futures::FutureExt as _;
use serde_json::{Map, Value as Json};

use self::process::{Node, Tailscaled};
use super::blocking;
use crate::provision::{
	context::Context,
	luks::Luks,
	model::Question,
	name::Known,
	step::{Done, Step},
	storage::{Storage, single_disk},
	system::{self, open_root},
};

mod process;
mod state;
mod tailnet;

/// The step whose end the prompts open after (PTS, "Asking").
const AFTER: &str = "eeprom";

/// What the answers are held under, which is also the id of each prompt.
const KEY: &str = "auth-key";
const NAME: &str = "name";

const KEY_TEXT: &str =
	"Tailscale auth key for this Iti. Use a reusable key; one that expires in a week is enough.";
const NAME_TEXT: &str = "Name for this Iti in the tailnet.";

/// The step.
pub struct Tailscale {
	storage: Arc<dyn Storage>,
	luks: Arc<dyn Luks>,
	node: Arc<dyn Node>,
	name: Known,
}

impl Tailscale {
	/// A step signing in through the real Tailscale, to the system on the disk `storage` finds,
	/// naming the Iti after the suffix of `name` by default.
	pub fn new(storage: Arc<dyn Storage>, luks: Arc<dyn Luks>, name: Known) -> Self {
		Self::with(storage, luks, Arc::new(Tailscaled), name)
	}

	fn with(
		storage: Arc<dyn Storage>,
		luks: Arc<dyn Luks>,
		node: Arc<dyn Node>,
		name: Known,
	) -> Self {
		Self {
			storage,
			luks,
			node,
			name,
		}
	}

	/// Open the root volume of the system on the board's disk and mount its root filesystem.
	async fn open(&self, writable: bool) -> Result<system::Root, String> {
		let (storage, luks) = (self.storage.clone(), self.luks.clone());
		blocking(move || open_root(&*storage, &luks, &single_disk(&*storage)?, writable)).await
	}

	/// The name offered for the Iti, once the board's ID has been established: none where it could
	/// not be.
	// spec: PTS
	async fn default_name(&self) -> Option<String> {
		let mut name = self.name.clone();
		let known = name.wait_for(Option::is_some).await.ok()?;
		match &known.as_ref()?.name.suffix {
			Suffix::Board(suffix) => Some(format!("tamanu-iti-{}", suffix.to_lowercase())),
			Suffix::Fault => None,
		}
	}

	/// Sign in, with the root filesystem mounted for as long as it takes.
	// spec: PTS
	async fn join(&self, name: &str, key: Secret) -> Result<bool, String> {
		let mount = self.open(true).await?;
		let at = mount.path().to_owned();
		let signed_in = async {
			let env = match fs::read_to_string(at.join(tailnet::PATH)) {
				Ok(env) => env,
				Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
				Err(err) => {
					return Err(format!(
						"cannot read the daemon options of the system: {err}"
					));
				}
			};
			let accepted = tailnet::accepted(&env)?;
			let directory = at.join(
				state::PATH
					.rsplit_once('/')
					.map_or(state::PATH, |(directory, _)| directory),
			);
			fs::create_dir_all(&directory)
				.map_err(|err| format!("cannot make Tailscale's state directory: {err}"))?;
			self.node.sign_in(&directory, name, key, accepted).await
		}
		.await;
		blocking(move || drop(mount)).await;
		signed_in
	}
}

/// Whether `name` is one Tailscale registers a machine under: 1 to 63 lower case letters, digits
/// and hyphens, beginning and ending with a letter or digit.
fn valid_name(name: &str) -> Result<(), String> {
	let edge = |c: Option<char>| c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
	let body = name
		.chars()
		.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
	if (1..=63).contains(&name.len())
		&& body
		&& edge(name.chars().next())
		&& edge(name.chars().last())
	{
		Ok(())
	} else {
		Err("a name is 1 to 63 lower case letters, digits and hyphens, beginning and ending with a letter or digit".to_owned())
	}
}

#[async_trait]
impl Step for Tailscale {
	fn id(&self) -> &str {
		"tailscale"
	}

	fn title(&self) -> &str {
		"Sign in to Tailscale"
	}

	fn asks_after(&self) -> Option<&str> {
		Some(AFTER)
	}

	// spec: PTS
	async fn ask_ahead(&self, context: &mut Context) {
		context.ask_later(Question::new(KEY, PromptKind::Secret, KEY_TEXT));
		let mut name = Question::new(NAME, PromptKind::Text, NAME_TEXT).check(valid_name);
		if let Some(default) = self.default_name().await {
			name = name.default(default);
		}
		context.ask_later(name);
	}

	// spec: PTS
	async fn done(&self) -> Done {
		let read = match self.open(false).await {
			Ok(mount) => {
				let at = mount.path().to_owned();
				blocking(move || {
					let _mount = mount;
					match fs::read(at.join(state::PATH)) {
						Ok(file) => state::records_sign_in(&file),
						Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(false),
						Err(err) => Err(format!("cannot read the Tailscale state: {err}")),
					}
				})
				.await
			}
			Err(why) => Err(why),
		};
		match read {
			Ok(true) => Done {
				state: DoneState::Done,
				why: Some("signed in".to_owned()),
			},
			Ok(false) => Done {
				state: DoneState::NotDone,
				why: Some("not signed in".to_owned()),
			},
			Err(why) => Done {
				state: DoneState::Unknown,
				why: Some(why),
			},
		}
	}

	// spec: PTS
	async fn run(&self, context: &mut Context) -> Result<(), String> {
		let default = self.default_name().await;
		let name = match context.take_text(NAME, default.as_deref()) {
			Some(name) => name,
			None => context.text(NAME, NAME_TEXT).await,
		};
		let key = Secret::new(context.secret(KEY, KEY_TEXT).await);

		let outcome = AssertUnwindSafe(self.join(&name, key)).catch_unwind().await;
		context.forget(KEY);
		let needs_approval = match outcome {
			Ok(joined) => joined?,
			Err(_) => return Err("the sign-in panicked".to_owned()),
		};
		let mut result = Map::new();
		result.insert("needs-approval".to_owned(), Json::Bool(needs_approval));
		context.result(result);
		Ok(())
	}
}

#[cfg(test)]
mod tests;
