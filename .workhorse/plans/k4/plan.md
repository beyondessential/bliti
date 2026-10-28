# Run provisioning mode on the device

Build the device side of provisioning mode from the provisioning specs (PRV, RUN, PRS, PCON, PLED) and their amendments to ADV, CHN, KEY, VER and SEC.
No real steps ship with this card: the registry is empty in production, and the engine is exercised with fake steps.
Later cards add steps against the `Step` trait built here.

## Where it goes

- Protocol types and the provisioning token, name and unauthenticated initiator go in `bliti-core`, because the browser client compiles that crate and must agree with the device.
- Everything that touches the board goes in a new `crates/bliti/src/provision/` module, one file per concern, in the style of `tamanu.rs`: a cheaply cloned `Provisioner` handle with a trait behind each outside dependency (steps, LED, button, clock) so tests use fakes.
- The GATT server and the advertising loop in `crates/bliti/src/device.rs` are shared rather than copied.
  `device.rs` is 864 lines, so the shared server setup moves into `device/` submodules before provisioning leans on it.

## Decisions taken in this plan

- Mode entry is its own subcommand, run by a unit that only the provisioning image installs, so the installed system's `bliti daemon` has no path into it.
  The subcommand also refuses to start unless `--medium` names a mount point, and takes `--board-id-source` to say which kind of source the board ID must come from (PRV).
  How the image wires these is the image-build card's.
- The USER button is read by polling GPIO27 through the existing by-name line reader, with a debounce, so no new dependency is added.
  That reader moves out of `facts/power/` to a shared module.
- The console is a client of the daemon over a local socket speaking the provisioning stream one message a line, as the battery socket does, so it shares one run with clients by construction.
- The console's hidden entry of a `secret` prompt wants a terminal without echo.
  `rpassword` is the small obvious crate for it; confirm before adding.
- The LED blinks from a software timer, not a kernel trigger, so a hung daemon stops blinking instead of blinking forever.
  Both LEDs get their kernel trigger set to `none` first.

## Core: protocol and shared pieces

- [ ] `bliti-core/src/channel/messages.rs`: add `Provision`, `ProvisionState { run, steps, prompts }`, `ProvisionAct`, `ProvisionAccepted`, with the typed step and prompt reports of PRS
- [ ] Wire names `provision`, `provision-state`, `provision-act`, `provision-accepted` in the type list, and pin `provision-act`'s `act`, `steps`, `prompt` and `value` critical in `critical_members`
- [ ] `messages/tests.rs`: round-trip and critical-member tests for each, including a `confirm` boolean and a `text` string `VALUE`
- [ ] `channel/generate.rs`: generators for the four new types, so the property tests and the wire-compat oracle cover them
- [ ] `cargo test -p bliti-wire-compat`: regenerate its corpus if it asks, and confirm `wire-breaks.toml` stays untouched
- [ ] New `bliti-core/src/provisioning.rs`: the 16-byte token `ITI PROVISIONING`, a pre-shared key from it through `PresenceToken`, and the name renderer and parser for `ITI_PROVISION_<marker>_<suffix>` with `FAULT`
- [ ] Compile-time assertion that the longest name element fits the advertising budget, beside the one in `advertisement.rs`
- [ ] `channel/noise.rs` and `stream.rs`: an initiator that skips the fingerprint comparison (PRV "The provisioning token"), and a `connect_initiator` counterpart, with a test that it completes against a device holding random keys
- [ ] `DeviceKeys` constructor for provisioning mode: random clamped static key, random 32-byte handshake digest, the provisioning token and its pre-shared key

## Device: identity and the name

- [ ] `identity.rs`: a variant of `establish` that never reads or writes the cache, probes, checks the strongest present kind is the expected one, checks memory as DEV requires, reads and derives, returning the QR payload
- [ ] `provision/name.rs`: the suffix from `QrPayload::suffix()`, and `FAULT` with the cause logged on stderr where the expected source is absent, the memory is short, or derivation fails
- [ ] The derivation runs once at start on a blocking task, before the USER press can need it, and never again while the process lives
- [ ] Tests: a board whose expected source is missing gives `FAULT`, and a weaker present source is never used

## Device: the button, the window and the GATT server

- [ ] Move `facts/power/gpio.rs` to a shared `gpio.rs`, and update the power code's imports
- [ ] `provision/button.rs`: a `Button` trait with a GPIO27 implementation that polls the line, debounces, and yields one event per press (low while held)
- [ ] Split `device.rs` so the GATT application, the write and subscription serving and `Slots` are reusable; `Slots` takes the number of slots it gives out
- [ ] `provision/window.rs`: the five-minute window as a state machine over press, timer, connect and disconnect events
  - silent from start; a press opens it; a press while open restarts it; a press while connected is ignored
  - a connection ends the window; nothing resumes advertising until the next press
- [ ] The advertising task registers the provisioning name and the service UUID, sets the adapter alias to the same name, and drops the registration the moment a client connects instead of re-registering as `device::run` does
- [ ] One slot only, so a second client reads the empty allocation
- [ ] Tests: the window over a paused clock covers each transition in PRV "Going on the air"

## Device: runs

- [ ] `provision/step.rs`: the `Step` trait (id, title, `done` giving a state and optional reason, `run` taking a context for progress, results and prompts), and the fixed-order registry, empty in production
- [ ] `provision/prompt.rs`: the four kinds, blocking and non-blocking prompts, a step's need of an earlier answer opening a blocking prompt when none is held, and answers held in memory only
- [ ] `provision/store.rs`: the run's state on the medium (chosen, passed, failed step and reason), keyed by platform serial, written before each step begins and after it passes, by temporary file and rename
- [ ] `provision/run.rs`: the `Provisioner` handle and its task, implementing the five states, the six step states and the acts with the refusal paths of PRS
  - `start` from idle or finished, `retry` from failed, `stop` from waiting or failed, `acknowledge` from finished, `answer` while a prompt is open
  - a finished run clears its stored state and lives in memory until acknowledged, replaced, or the process ends
  - done-states are determined on entering idle or finished and on each attach while in either, and reported `unknown` until then
- [ ] On start, resume a stored run that was running or waiting without any client, restarting the interrupted step, and enter `failed` for a stored failed run without retrying it
- [ ] Tests with fake steps: ordering, a failing step stopping the run, retry skipping passed steps, resume after a simulated restart mid-step, a second answer refused, a `secret` never reaching the store, a non-blocking prompt answered late by the step that needs it, the run continuing with no front end attached

## Device: the stream

- [ ] `provision/stream.rs`: `serve(stream, provisioner, peer)` in the shape of `tamanu::serve`, sending `provision-state` on open and on every change, answering each `provision-act` once with `provision-accepted` or `cannot-act` at the paths PRS gives
- [ ] `provision/session.rs`: a session that accepts the handshake with the provisioning keys, opens the hello stream, serves only `provision`, and passes over every other stream role and topic, as MSG requires
- [ ] The log line for each act names the client's `hello`, as `power::serve` does
- [ ] `session/tests`-style tests over an in-memory duplex: handshake, state on open, every act, every refusal, two streams at once

## Device: the LED

- [ ] `provision/led.rs`: an `Led` trait, a sysfs implementation for `ACT` (green, active low) and `PWR` (red), and a driver that watches the run state and shows the PLED table
- [ ] Amber is both lit; blink at 1 Hz and 3 Hz from a timer; set both triggers to `none` on start
- [ ] The driver exists only inside provisioning mode
- [ ] Tests with a fake LED over a paused clock: every state, and the change from one pattern to the next mid-blink

## Console

- [ ] `provision/socket.rs`: the daemon's local socket, root-only, serving the provisioning stream one message a line, in the shape of `battery/socket.rs`
- [ ] `provision/console.rs`: the command, with the step list and defaults, live state lines, prompts answered by typed lines, retry, stop and acknowledge, results printed, and `secret` input without echo
- [ ] Subcommands in `main.rs` for the daemon and the console, with `--medium`, `--board-id-source` and `--socket`
- [ ] Tests: the console against a daemon over a socket in a temp directory, and alongside a stream client with the first answer winning

## Wiring and docs

- [ ] `services/bliti-provision.service` for the provisioning image only, and its entry in `services/README.md`
- [ ] README: a short section on provisioning mode and the console command
- [ ] `// spec: PRV`, `RUN`, `PRS`, `PCON` and `PLED` comments where each rule is implemented
- [ ] `cargo check -p bliti-web --target wasm32-unknown-unknown`, fixing any exhaustive match the new messages break; the page itself is another card
- [ ] `cargo fmt`, `cargo clippy --workspace`, `cargo test --workspace`

## Check on hardware

- [ ] On the prototype: silent until USER is pressed, the name and suffix match the label, advertising stops on connect and stays off after disconnect, and a second press restarts it
- [ ] On the prototype: each LED state reads as the table says, amber included
- [ ] On the prototype: a fake long step survives a power cut and resumes, and a failed step stays failed across a reboot
