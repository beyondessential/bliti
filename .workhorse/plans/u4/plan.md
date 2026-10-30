# Sign the Iti in to Tailscale during provisioning

Builds the `tailscale` step of [PTS](../../specs/provisioning/tailscale.md) and the pieces of PRS, RUN, PCON and PWEB it leans on.
Device tests happen against epic, not on this card, so everything here is built against fakes and checked with unit and browser tests.

## Design notes

**The engine can ask ahead of a step's turn already, but nothing tells it when.**
`Model::ask_later` and `Context::ask_later` (K4) open a non-blocking prompt, answers are held by key for the whole run in memory, and `Model::need` turns an open prompt into a blocking one when the step reaches it.
What is missing is the trigger: `Context` only exists while a step is running, and the `tailscale` step is not running when PTS says its prompts open (once every chosen step up to and including `eeprom` has passed).
So `Step` gets a hook, `asks_after() -> Option<&str>` (the id of the step) and `ask_ahead(&self, context)`, and `Inner::drive` calls it at the top of each loop iteration, for each pending step whose `asks_after` step and everything chosen before it has passed, with a `Context` built for that step.
`ask_later` skips a prompt that is already open or answered, so calling it every iteration is safe, and it is what reopens the prompts after a reboot (the run resumes, and the store holds no answers).
A retry reaches the same code: `Model::retry` makes the step pending again, and the key, discarded when the step ended, is asked for again.

**A prompt gains a default and a check.**
`ask_later` takes a `Question` (key, kind, text, optional default, optional check `fn(&str) -> Result<(), String>`) in place of its loose arguments.
`PromptReport` gains `default: Option<String>`, skipped when absent, which is the one wire change (`wire-breaks.toml` records nothing before the first release).
`Model::answer` runs the check and refuses at `$['value']` with the check's reason.

**The step holds the key by key, not by value.**
The step asks for `auth-key` and `name` under those keys, takes them with `Context::secret`/`text` at its turn (`Need::Held` where answered, a blocking prompt otherwise), and calls a new `Context::forget(key)` when it ends, pass or fail, which removes the held answer.
For `name`, an unanswered prompt at the turn takes the default instead of waiting, through a new `Context::take_text(key, default)` that closes the prompt.
Where the suffix is `FAULT` the prompt has no default, and `take_text` has nothing to fall back on, so the step waits for a name like any other answer.

**Signing in runs a private tailscaled against the disk's state.**
The medium is the `pi` image, which has Tailscale, but its own tailscaled must stay out of this: a sign-in in the medium's state would be taken for the next board's.
The step starts a `tailscaled` of its own (userspace networking, so the provisioning system's network is untouched; its own socket under `/run/bliti-provision`; its state in `/var/lib/tailscale` of the mounted root filesystem), drives it through the existing `tailscale::client::LocalApi`, and stops it before unmounting.
The sign-in itself (prefs with `WantRunning` and `Hostname`, `start` with the key, wait for `Running` or `NeedsApproval`, scrub the key from any message, hold to the accepted tailnet) is lifted out of `tailscale/run.rs` into a function both the TSC act and this step call, rather than copied.
The accepted tailnet is the `--tailnet` option of `BLITI_DAEMON_ARGS` in `/etc/bliti/daemon.env` on the mounted root filesystem, read with the clap definition `main.rs` uses for `daemon`, so the two cannot drift.

**Opening the root volume belongs to S4.**
PENC and this step both need "open the root volume through its TPM keyslot and mount its root filesystem", and S4's code is not on `main` yet (only its specs are).
The step takes it as a trait, `Root`, whose `open(read_only)` returns a mount that unmounts and closes on drop, with a fake for tests.
Before writing the real implementation, check whether S4's code has landed and use its helper; if not, write one with `cryptsetup open --token-only` and settle it with S4 so the two do not diverge.

**The done-state reads the state file.**
`tailscaled.state` is JSON.
The parser must be written against a real file, not a guessed schema: copy the prototype's `/var/lib/tailscale/tailscaled.state` (ask the user first, since it is the prototype's identity), strip every key and identifier from it, and keep it as a fixture.
The step reports `done` where the current profile records a sign-in that has not been logged out.

**The browser holds the key in local storage.**
A small module, `tailscale.js` in `app/src/provision`, owns the stored key: `{ key, saved }` under one storage name, read as nothing where unreadable or a week old (and removed), written when an `auth-key` answer is sent, removed on forget.
The hook auto-answers an open `auth-key` prompt from it once per prompt, marked `secret` so Activity records no value (`actSaid` already does that), and remembers that it did so to show "Using the key saved <date>" with a Forget button beneath the Tailscale row.
It discards the key where the device refuses the answer or the `tailscale` step fails.

## Specs

- [x] PTS, and the amendments to RUN, PRS, PWEB, PCON and PRV (done, on this branch)

## Engine and wire (`bliti-core`, `bliti`)

- [ ] `bliti-core/src/provisioning/report.rs`: `PromptReport.default`, skipped when absent; update the proptest strategy in `channel/generate.rs`, the message round-trip tests, and any place that builds a `PromptReport`
- [ ] `provision/model.rs`: `Question` struct; `Open` carries `default` and `check`; `ask_later` takes a `Question`; `snapshot` reports the default; `answer` runs the check; add `forget(key)` and `take(key)` (closes the prompt, returns the held answer)
- [ ] `provision/context.rs`: `ask_later(Question)`, `forget(key)`, `take_text(key, default)`; remove the `expect(dead_code)` markers the new callers make unnecessary
- [ ] `provision/step.rs` and `run.rs`: `Step::asks_after` and `Step::ask_ahead` with no-op defaults; `Inner::drive` runs the hook at the top of each iteration, for steps still `Pending` whose gate has passed; gate = every chosen step up to and including the named one is `Passed`, or none is chosen
- [ ] Tests in `model/tests.rs` and `run/tests.rs`: default reported; check refuses at `$['value']`; prompts open after the gating step passes and not before; open at the start where the gating step is unchosen; reopen after `adopt` on resume; an unanswered secret becomes blocking at the step's turn and the run is `waiting`; `forget` drops the held answer; a retry asks again

## The step (`bliti/src/provision/steps/tailscale.rs`)

- [ ] Extract the auth-key sign-in and the accepted-tailnet hold from `tailscale/run.rs` into functions both callers use; the TSC tests keep passing unchanged
- [ ] `Root` trait and its real implementation (see the design note); `Tailscaled` trait for starting and stopping the private daemon and reaching its `LocalApi`, with fakes in `tailscale/fake.rs`
- [ ] Parse `tailscaled.state` into "records a sign-in", with the scrubbed fixture and tests for signed in, logged out, empty and missing state
- [ ] Read the accepted tailnet from `daemon.env` on the mounted root filesystem; none where the file or option is absent
- [ ] `Tailscale` step: `id` `tailscale`, title "Sign in to Tailscale"; `asks_after` is `eeprom`; `ask_ahead` opens `auth-key` (secret, text saying the key is to be reusable) and `name` (text, default `tamanu-iti-<suffix lower case>`, check for 1 to 63 lower case letters, digits and hyphens that begin and end with a letter or digit)
- [ ] `done()`: open the root volume, mount read-only, read the state, close; `done`, `not-done` or `unknown` with a reason, writing nothing
- [ ] `run()`: take the name (default if unanswered), take the key (waits if unanswered), open and mount read-write, start the private tailscaled, sign in, report `needs-approval` as the result, stop tailscaled, unmount, close, and `forget("auth-key")` on every path out, including failure and panic (a drop guard, since the engine catches panics)
- [ ] Failure reasons for each case PTS lists (root volume, cannot reach Tailscale, key rejected or neither state reached, tailnet not accepted); none contains the key
- [ ] Register it in `steps::all` between `eeprom` and `bootloader` with the suffix, which `daemon.rs` passes through `registry` from the name it already derives (none where it is `FAULT`); S4's `disk`, `overlay` and `encryption` slot in ahead of it when they land
- [ ] Step tests with fakes: prompts and their texts; the name default and its check; a full sign-in passes and leaves no state on the medium; each failure; the key is gone after pass and after failure; `needs-approval` result; the done-states

## Console (`provision/console`)

- [ ] Show a prompt's default and take an empty answer as it (`question.rs`, `console.rs`); a secret stays unechoed
- [ ] Console tests: default shown and accepted, a refused name asks again, the key is not echoed

## Browser client (`bliti-web/app`)

- [ ] `provision/tailscale.js`: save, load (discarding at a week), forget; unit-tested through the Playwright specs with the page clock controlled
- [ ] `useProvision.js`: auto-answer `auth-key` from storage once per prompt as a `secret`; save the key on a sent `auth-key` answer; discard on a refusal or on the `tailscale` step failing; expose "used the saved key, saved <when>" and a forget action
- [ ] `Prompt.jsx`: a `text` field starts at the prompt's default
- [ ] `Run.jsx`: beneath the Tailscale row, the saved-key note with Forget; where the step passed with `needs-approval` true, the sentence PWEB requires
- [ ] `tests/fake-provision-client.js`: prompts with a default, a step result, and the extra rows
- [ ] `tests/provision-tailscale.spec.js`: both prompts show under the Tailscale row while it is pending; the field starts at the default; a saved key is written with its time; a second board is answered from it with the note; a key a week old is dropped and asked for; Forget removes it; a refusal and a failed step drop it; neither the key nor its prefix is in Activity; the approval sentence

## Closing out

- [ ] `provision/README.md`: say the medium's bench needs a network connection for the Tailscale step (PTS fails where it cannot reach Tailscale); PMED leaves the bench network unspecified, so raise it on the F4 umbrella rather than inventing a requirement here
- [ ] `cargo fmt`, `cargo clippy`, `cargo test` for `bliti-core` and `bliti`, `just test-web`
- [ ] Remove this plan's directory before merge, as the other cards do
