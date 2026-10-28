//! Which steps the operator has ticked to run, and how they change the ticks (PCON, RUN).

use std::collections::BTreeSet;

use bliti_core::provisioning::report::{DoneState, StepReport};

/// What the operator typed at the step list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
	/// Flip these entries, by index.
	Toggle(Vec<usize>),
	All,
	Nothing,
	/// Start a run of the steps ticked.
	Start,
	/// Leave without starting.
	Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
	id: String,
	label: String,
	done: DoneState,
	why: Option<String>,
	ticked: bool,
}

fn ticked_by_default(done: DoneState) -> bool {
	// spec: RUN
	done != DoneState::Done
}

/// The steps in device order, each ticked or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
	entries: Vec<Entry>,
}

/// A step by its title and its id, which is what the operator types.
pub fn label(step: &StepReport) -> String {
	if step.title == step.id {
		step.id.clone()
	} else {
		format!("{} ({})", step.title, step.id)
	}
}

/// A done-state with its reason, as the list shows it.
pub fn done_text(done: DoneState, why: Option<&str>) -> String {
	let state = match done {
		DoneState::Done => "done",
		DoneState::NotDone => "not done",
		DoneState::Unknown => "unknown",
	};
	match why {
		Some(why) if !why.is_empty() => format!("{state}: {why}"),
		_ => state.to_owned(),
	}
}

impl Selection {
	/// Every step ticked but those reported done.
	pub fn of(steps: &[StepReport]) -> Self {
		Self {
			entries: steps
				.iter()
				.map(|step| Entry {
					id: step.id.clone(),
					label: label(step),
					done: step.done,
					why: step.why.clone(),
					ticked: ticked_by_default(step.done),
				})
				.collect(),
		}
	}

	/// Take the done-states the device has determined since, ticking afresh each step whose done-state
	/// changed and leaving the rest as the operator set them. True where anything changed.
	pub fn refresh(&mut self, steps: &[StepReport]) -> bool {
		let mut changed = false;
		for (entry, step) in self.entries.iter_mut().zip(steps) {
			if entry.done != step.done || entry.why != step.why {
				entry.done = step.done;
				entry.why.clone_from(&step.why);
				entry.ticked = ticked_by_default(step.done);
				changed = true;
			}
		}
		changed
	}

	/// The list as lines, numbered from 1.
	pub fn lines(&self) -> Vec<String> {
		let width = self.entries.len().to_string().len();
		self.entries
			.iter()
			.enumerate()
			.map(|(index, entry)| {
				format!(
					"  {:>width$}. [{}] {}: {}",
					index + 1,
					if entry.ticked { 'x' } else { ' ' },
					entry.label,
					done_text(entry.done, entry.why.as_deref()),
				)
			})
			.collect()
	}

	/// Read what the operator typed: step numbers or ids to flip, `all`, `none`, nothing to start, or
	/// `q` to leave.
	pub fn parse(&self, line: &str) -> Result<Pick, String> {
		let line = line.trim();
		match line.to_ascii_lowercase().as_str() {
			"" => return Ok(Pick::Start),
			"q" | "quit" => return Ok(Pick::Quit),
			"all" => return Ok(Pick::All),
			"none" => return Ok(Pick::Nothing),
			_ => {}
		}
		let mut flips = BTreeSet::new();
		for token in line.split(|c: char| c.is_whitespace() || c == ',') {
			if token.is_empty() {
				continue;
			}
			let by_id = self.entries.iter().position(|entry| entry.id == token);
			let by_number = token
				.parse::<usize>()
				.ok()
				.filter(|number| (1..=self.entries.len()).contains(number))
				.map(|number| number - 1);
			match by_id.or(by_number) {
				Some(index) => {
					flips.insert(index);
				}
				None => {
					return Err(format!(
						"{token:?} is neither a step number from 1 to {} nor a step name",
						self.entries.len()
					));
				}
			}
		}
		Ok(Pick::Toggle(flips.into_iter().collect()))
	}

	/// Carry out a pick that changes the ticks.
	pub fn apply(&mut self, pick: &Pick) {
		match pick {
			Pick::Toggle(indices) => {
				for &index in indices {
					if let Some(entry) = self.entries.get_mut(index) {
						entry.ticked = !entry.ticked;
					}
				}
			}
			Pick::All => self
				.entries
				.iter_mut()
				.for_each(|entry| entry.ticked = true),
			Pick::Nothing => self
				.entries
				.iter_mut()
				.for_each(|entry| entry.ticked = false),
			Pick::Start | Pick::Quit => {}
		}
	}

	/// The ids of the steps ticked, in order.
	pub fn chosen(&self) -> Vec<String> {
		self.entries
			.iter()
			.filter(|entry| entry.ticked)
			.map(|entry| entry.id.clone())
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use bliti_core::provisioning::report::StepState;

	use super::*;

	fn step(id: &str, done: DoneState, why: Option<&str>) -> StepReport {
		StepReport {
			id: id.to_owned(),
			title: id.to_owned(),
			done,
			why: why.map(str::to_owned),
			state: StepState::Unchosen,
			progress: None,
			reason: None,
			result: None,
		}
	}

	fn steps() -> Vec<StepReport> {
		vec![
			step("flash", DoneState::NotDone, Some("older firmware")),
			step("name", DoneState::Done, Some("Up to date")),
			step("check", DoneState::Unknown, None),
		]
	}

	/// A step reported done is unticked, every other ticked (RUN, PCON).
	#[test]
	fn only_the_steps_reported_done_start_unticked() {
		let selection = Selection::of(&steps());
		assert_eq!(selection.chosen(), ["flash", "check"]);
		assert_eq!(
			selection.lines(),
			[
				"  1. [x] flash: not done: older firmware",
				"  2. [ ] name: done: Up to date",
				"  3. [x] check: unknown",
			]
		);
	}

	#[test]
	fn a_title_is_shown_with_its_id() {
		let mut flash = step("flash", DoneState::NotDone, None);
		flash.title = "Flash the firmware".to_owned();
		assert_eq!(
			Selection::of(&[flash]).lines(),
			["  1. [x] Flash the firmware (flash): not done"]
		);
	}

	#[test]
	fn numbers_and_ids_flip_ticks_and_once_each() {
		let mut selection = Selection::of(&steps());
		let pick = selection.parse("2, flash 2").unwrap();
		assert_eq!(pick, Pick::Toggle(vec![0, 1]));
		selection.apply(&pick);
		assert_eq!(selection.chosen(), ["name", "check"]);
	}

	#[test]
	fn keywords_start_quit_and_tick_everything_or_nothing() {
		let mut selection = Selection::of(&steps());
		assert_eq!(selection.parse("").unwrap(), Pick::Start);
		assert_eq!(selection.parse("  ").unwrap(), Pick::Start);
		assert_eq!(selection.parse("q").unwrap(), Pick::Quit);
		assert_eq!(selection.parse("Quit").unwrap(), Pick::Quit);
		selection.apply(&selection.parse("all").unwrap());
		assert_eq!(selection.chosen(), ["flash", "name", "check"]);
		selection.apply(&selection.parse("none").unwrap());
		assert!(selection.chosen().is_empty());
	}

	#[test]
	fn what_names_no_step_is_refused() {
		let selection = Selection::of(&steps());
		for bad in ["0", "4", "flsh", "1 9"] {
			let err = selection.parse(bad).unwrap_err();
			assert!(err.contains("neither a step number"), "{bad}: {err}");
		}
	}

	/// A step's id wins over a number where a step is named by digits.
	#[test]
	fn an_id_made_of_digits_is_the_step_it_names() {
		let selection = Selection::of(&[
			step("a", DoneState::NotDone, None),
			step("1", DoneState::NotDone, None),
		]);
		assert_eq!(selection.parse("1").unwrap(), Pick::Toggle(vec![1]));
	}

	/// Done-states determined after the list was first shown replace the defaults they change, and
	/// leave what the operator set elsewhere.
	#[test]
	fn a_later_done_state_reticks_only_what_it_changed() {
		let mut unknown = steps();
		unknown.iter_mut().for_each(|step| {
			step.done = DoneState::Unknown;
			step.why = None;
		});
		let mut selection = Selection::of(&unknown);
		selection.apply(&Pick::Toggle(vec![2]));
		assert_eq!(selection.chosen(), ["flash", "name"]);

		assert!(selection.refresh(&steps()));
		assert_eq!(selection.chosen(), ["flash"]);
		assert!(!selection.refresh(&steps()));
	}
}
