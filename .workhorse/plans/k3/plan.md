# One-step network apply

The device saves a proposal before applying it and keeps it whether or not it verifies, and the configuration session becomes a stream reporting what the device holds and how it is doing.
Several sessions may be open, the longest-open one holding the configuration and the rest watching it read-only.
WPS goes, and unmet requests on every stream are answered with `cannot-act` or `cannot-configure` of MSG.

## Design notes

### Verification outlives the session

Today the held configuration lives behind a mutex each session takes with `try_lock_owned` (that is how `busy` works), and the proposal is verified inside the session task, so ending the session drops the verification and reverts.
Under CFG a session ending changes nothing, so the configuration, its outcome and any running verification belong to the `Configurator`, not to a session.

- The `Configurator` becomes one task owning the backend, the store and the held configuration: `{raw, document, outcome, failure}`.
  Sessions send it commands over an mpsc (propose with a oneshot reply of saved or `cannot-configure`, scan, survey) and read a `watch` of the held configuration.
- The task selects over its commands and the in-flight apply future, so a scan or a newer proposal is served while a verification runs.
  A newer save drops the in-flight future, which abandons that verification.
- Sessions register on opening and deregister on drop, in open order; a `watch` of the holder's id lets each session work out `busy` and send `state` when it changes.
- Each session sends `configuration` when the held document changes, and `state` after it and whenever the outcome, `busy`, capabilities or the backend's states change.
  The backend's `states()` watch already supports several readers.

### Store

`network.json` becomes `{document, outcome, failure?, replaced?}`, written atomically as now.
`replaced` is the configuration the held one replaced and is kept only while verifying, so a restart that interrupts a verification judges it again against the same configuration.
A settled configuration is written once it is saved and again when its outcome is reached.

Pre-release, the old bare-document file is not read: a device that has one starts on the default wired configuration.
The prototype's saved network is lost once on deploy.

### Driver

- `Command::Apply` carries the document it replaces, so `changed()` judges against that rather than the driver's own `running`, which a restart has lost.
- The driver sets `running` to the proposal whatever the verdict, and keeps selecting on it after a failure rather than waiting for a restore.
- `Restore` is only used at start, for a held configuration whose outcome is settled.
- An apply error after saving (render or `at_fault`) becomes the outcome `failed`, with the failure at the part at fault.
  `Backend::check` already dry-runs the render, so this is the rare case.

### Wire

- Gone: `applied`, `invalid`, `confirm`, `discard`, `busy`, `wps`, `pin`, `refused`.
- New: `cannot-act {act, at, reason}` and `cannot-configure {at, reason}`.
- Changed: `state` gains `busy`, `outcome` and `failure`; `configuration` from a device may carry `capabilities` on any send.
- No critical member changes, so `wire-breaks.toml` is untouched.
  The oracle's baseline snapshot carries no configuration-session messages, so removing them does not trip it.

### Client

- The reducer tells a saved proposal from a refused one by the next `configuration`.
  It takes the stage from `state.outcome` on opening, after the echo, and when the outcome changes.
  The `awaiting` counter and the `applied` stage go.
- Failures come from `state.failure` (verification) or `cannot-configure` (refusal), in the shape `Failure` already renders.
- `busy` becomes a watching stage inside the screen instead of replacing it.
- The kept session goes entirely, so leaving the screen always closes the session.

## Steps

- [x] Specs: CFG, NSCR, NET, WLAN, HOT, NFO, VIEW, MSG, CTL, CRV and the Control screen

### Core (`bliti-core`)

- [ ] `channel/messages.rs`: remove `Applied`, `Invalid`, `Confirm`, `Discard`, `Busy`, `Wps`, `Pin` and `Refused`. Add `CannotAct` and `CannotConfigure`. Give `State` `busy`, `outcome` and `failure`. Update `to_map`, the visitor and `known_types`.
- [ ] `channel/config.rs`: reshape `Invalid` into the failure carried on `state` (`at`, `reason`, `reached`) and a refusal (`at`, `reason`), with an `Outcome` enum for `verifying`, `applied` and `failed`.
- [ ] `channel/generate.rs`: update the strategy and `the_configuration_optional_members_are_reached` (drop `Invalid.reached`, `Applied.capabilities`, `Wps.*`; add `failure` and `failure.reached`).
- [ ] `messages/tests.rs`: update the round-trip list, drop `wps_carries_its_ssid_where_it_names_one`, and add round trips for `cannot-act`, `cannot-configure` and the new `state`.

### Device (`bliti`): configuration

- [ ] `network/session/store.rs`: the `{document, outcome, failure?, replaced?}` shape, a record of the outcome, and store tests for each shape and for a missing file.
- [ ] `network/stack/driver.rs` and `driver/verdict.rs`: `Command::Apply` carries the replaced document; `running` becomes the proposal whatever the verdict; the post-save apply error becomes a verdict.
- [ ] `network/session/backend.rs` and `stack.rs`: `apply(document, replacing)`. Drop `wps`, the `Wps` struct and `Inert`'s WPS refusal.
- [ ] `network/session.rs`: the `Configurator` task per the design notes.
  - On start: load, then `restore` a settled configuration, or `apply` an unsettled one against `replaced`.
  - Propose: `parse`, `within` and `check`, then either `cannot-configure` or record, publish, then apply. On the verdict, record the outcome and publish.
  - Drop `confirm`, `discard`, `revert`, the `Drop` restore task and the `provisional` watch.
- [ ] `network/session.rs`: the session side.
  - Register in open order and send the opening `configuration` and `state`.
  - Send `configuration` and `state` on every change of the held configuration, holder, capabilities or backend states.
  - Send `cannot-configure` then `configuration` on a refusal.
  - Answer a busy session's proposal or act at `$`.
  - Answer scan and survey through the task, with `cannot-act` naming the act.
- [ ] Split `network/session.rs` into submodules (task, session, holder) to keep each under 1000 lines.
- [ ] `session.rs` (dispatcher): drop the `network-configuration` fact from `facts_now` and the 2s provisional poll in `serve_default`; drop the removed types' no-op arms.
- [ ] WPS removal on the device:
  - `network/stack/acts.rs`: `wps`, `Hold`, `joined_document`, PIN generation.
  - Driver `Command::Hold` and the held-station skips.
  - `observe.rs` and `observe/iwd.rs`: the `Iwd` WPS methods, `WpsFailed`, `WALK`, `NO_CREDENTIALS`; keep `passphrase` and `forget` only if anything else uses them.
  - `probe/capabilities.rs`: the `wps` act in `acts()`.
  - `render.rs` and `render/iwd.rs`: the `known_psk` / `known_passphrase` path a WPS join leaves, if nothing else needs it.
  - Keep `wps_state=0` in `render/hostapd.rs`: the hotspot still never offers WPS.
- [ ] `client.rs` (CLI): the new message set; print `cannot-act` and `cannot-configure`.

### Device (`bliti`): power and curve

- [ ] `power.rs`: answer `act` with `cannot-act`, at `$['act']` for an act not listed and at `$` otherwise.
- [ ] `battery.rs`: answer `load` with `cannot-configure` (at the first member at fault, else `$`) and `reset` with `cannot-act`; keep the command line printing the reason on standard error.

### Device tests

- [ ] `network/session/tests/fake.rs`: drop `Call::Wps`, `pins` and `joins`; `Applying::Hang` stays for verifications that outlive a session.
- [ ] Delete `network/session/tests/wps.rs` and the WPS tests in `tests/capabilities.rs`, `stack/tests.rs` and `stack/tests/fake.rs`.
- [ ] Rewrite `network/session/tests.rs` for the new exchange:
  - opening sends `configuration` then `state`
  - a saved proposal is echoed, then `state` with `verifying`, then the outcome
  - a refused one gets `cannot-configure` then the unchanged `configuration`
  - a failed one stays held, and the next session opens on it with the failure
  - a session closed mid-verification leaves it verifying, and the next session sees it finish
  - a newer proposal abandons the previous verification
  - a second session is busy, gets every change, and has its proposals and acts refused at `$`
  - the second session holds once the first closes
- [ ] Store and start: a restart with `replaced` present verifies again against it; with the outcome settled, it restores without verifying.
- [ ] `stack/tests.rs`: a failed verdict leaves the proposal running, and selection continues on it.
- [ ] `session/tests.rs` (dispatcher): replace `configure_opens_a_session_and_a_second_is_busy` with two sessions open at once; drop the `network-configuration` fact assertions.
- [ ] Power and curve tests for `cannot-act` and `cannot-configure`.

### Web client: bridge

- [ ] `bliti-web/src/lib.rs`: drop `ConfigurationHandle::{confirm, discard, wps}`.
- [ ] `app/src/client.js`: drop the same from `configure()`.
- [ ] `app/src/client.js`: power and curve streams read `cannot-act` and `cannot-configure` in place of `refused`.

### Web client: network screen

- [ ] `app/src/network.js`: rewrite `reduce`, `fromDevice`, `opening`, `editing` and `answer` for the stream.
  - Add the `watching` stage from `state.busy`.
  - Take the stage from `outcome` on opening, after the echo, and when it changes.
  - Tell saved from refused by comparing the next `configuration` with `proposed`.
  - Fill `failure` from `state.failure` or `cannot-configure`.
  - Drop `awaiting`, `confirming`, the `applied` stage, the WPS fields, `pin`, and the `invalid` handling of act failures (now `cannot-act`).
- [ ] `app/src/network.js`: `unapplied(state)`, true in editing or errored where the fields differ from the held configuration.
- [ ] `app/src/Network.jsx`: `SessionBar` drops applied, Confirm and Cancel.
  - Errored says the device is on these settings, or that it did not take them for a refusal.
  - Watching says someone else is configuring the device.
  - `busy` stops replacing the screen; the fields go read-only through `writable`.
- [ ] `app/src/Network.jsx`: `failureHeadline` wording no longer says "Reverted" or "Nothing was changed" for a saved failure.
- [ ] `app/src/Network.jsx`: Back asks to confirm while `unapplied`, with a dialog in the shape of Control's act confirmation.
- [ ] `app/src/Network.jsx`: a `beforeunload` listener while `unapplied`.
- [ ] `app/src/App.jsx`: drop `held`, `keep`, `HeldBar`, the hidden kept mount of the network screen, `leaveNetwork`'s keep logic, and the `provisional` and `unapplied` props to Control.
- [ ] `app/src/Control.jsx`: drop the provisional and unapplied warnings; render `cannot-act` and `cannot-configure` reasons.
- [ ] `app/src/Readings.jsx`, `readings.js`, `styles.css`: drop the `Provisional` context, `PROVISIONAL_TILES`, the notice, the `trial` label and its CSS, and `showProvisional`.
- [ ] WPS removal in the client:
  - `capabilities.js`: `WPS_METHODS`, `wpsRadios`, `wpsMethods`, `wpsMethodsFor`, `wps` in `acts()`, `RADIO_SETTINGS` and `offered()`.
  - `Network.jsx`: `joinByWps`, the Add-row buttons, the "WPS with" select, the failure notice, the PIN.
  - `Scan.jsx`: the per-network WPS link.
  - `styles.css`: `.network-row .wps` and `.bar-state .pin`.

### Web tests

- [ ] `tests/fake-client.js`: script `state` with `busy` and `outcome`; helpers for a saved echo, a refusal, and `busy` flipping mid-session; drop `wps`, `confirm` and `discard`.
- [ ] `tests/network-fixtures.js`: drop the WPS acts from `PI`, `INDEPENDENT` and `TWO_RADIOS`; give `STATES` the new members.
- [ ] `tests/network.spec.js`: rewrite "editing is the application's own" and "the session as the device speaks it".
  - apply then echo then verifying then applied
  - a refusal keeps the proposal and marks the field
  - a saved failure keeps the fields, and reopening shows the failure
  - opening while verifying shows applying, read-only
  - watching is read-only, follows changes and offers no scan, then becomes editable when `busy` clears
  - Back with unapplied edits asks first, and Back after a saved failure does not
  - `beforeunload` is armed only while unapplied
  - Drop the WPS cases.
- [ ] `tests/network-scan.spec.js` and `tests/capabilities.spec.js`: drop the WPS cases.
- [ ] `tests/readings.spec.js` and `tests/control.spec.js`: drop the provisional cases; add `cannot-act` rendering for power and curve.

### Finish

- [ ] `cargo fmt`, `cargo clippy --all-targets`, `cargo test` (with `-p bliti-core --features generate`), `cargo test -p bliti-wire-compat`.
- [ ] `just test-web`.
- [ ] On the prototype (bliti-prototype skill):
  - apply a working change and a failing one
  - leave mid-verification and come back
  - reboot mid-verification
  - a second client watching, then taking over
  - a refused power act shows its reason
