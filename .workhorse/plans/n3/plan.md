# Tamanu status and restart (N3)

Specs: [TMU](../../specs/control/tamanu.md), and the Tamanu section of [NFO](../../specs/device-info.md). Shaped like Tailscale (M3): `crates/bliti/src/tailscale/` and `facts/tailscale.rs` are the template.

## What `bestool` gives

The device calls `bestool` as it is: `status --json` to read, `start` and `restart` to act. It does not reimplement any of its discovery, ordering, rolling or readiness logic, and the spec does not restate it.

- State and versions come from `bestool tamanu status --json`. It prints the JSON and then exits non-zero when anything is short, so parse stdout whatever the exit status.
- Per expectation the JSON has `name`, `expected_state`, `running`, `min_count`, `status` (`up`, `down`, `short`, `absent`, `forbidden`, `unknown`), `reason`, `expected_version`, and `instances[]` with `version_actual` (only while that instance runs) and `version_status` (`match`, `mismatch`, `unknown`). `any_short` is true for a short or forbidden expectation and for any version mismatch. That is `running` versus not.
- Web is the `tamanu-frontend` expectation, versioned by `TAMANU_FRONTEND_VERSION`. Facility server is `tamanu-facility-api`, versioned by `TAMANU_VERSION`. Both carry `expected_version` even with nothing running, which is the configured fallback the spec asks for.
- `unknown` expectations (a DB signal that could not be read) are not short, and do not count toward `degraded`.
- Absent: with no installation `status --json` exits 1 with only stderr ("not a Tamanu host: /etc/tamanu/env not found"), and `find` exits 0 printing nothing. Decide absent from `find` printing nothing, not from the stderr text; `find` listing a root while `status` fails, `bestool` not found on the path (not expected, since the image carries it), unparseable JSON, or a timeout are `broken`.
- `stopped` is every `up`-expected expectation at `down`; any other shortfall is `degraded`.
- Acts are `bestool tamanu start` and `bestool tamanu restart`. Both are slow (`restart` rolls instances behind readiness probes), so the device must not block its other work on them; success is judged by the state reaching `running`.
- The daemon runs as root, so `bestool`'s sudo re-exec for reading container versions does not trigger.

## Where it goes

The shape is Tailscale's (M3), minus the daemon watcher and the act parameters:

- `crates/bliti/src/tamanu.rs` with submodules, in place of `tailscale.rs`: a shared `Tamanu` handle, `serve` for the stream, the one-act `Slot`.
- `crates/bliti/src/facts/tamanu.rs` for the three entries, as `facts/tailscale.rs`.
- A `Runner` trait over the `bestool` calls (read status, find, run an act), with a fake for tests, as `tailscale::netcheck::Runner`.
- State is polled, not watched, since `bestool` has no event feed. One poller task owns the latest observation in a `watch` channel. It polls only while the sampler has a session open, and for the thirty minutes after, as NFO's Sampling has it, so a device nobody is talking to does not inspect containers. It polls again at once when an act ends.
- The state `starting` or `restarting` is the act under way laid over the observation, not something `bestool` reports, so the published value combines the two and republishes when the slot is taken or released.

## Checklist

### Wire

- [ ] `crates/bliti-core/src/channel/messages.rs`: add `Tamanu`, `TamanuActs { acts }`, `TamanuAct { act }`, `TamanuAccepted`, `TamanuFailed { reason }`, with their wire names (`tamanu`, `tamanu-acts`, `tamanu-act`, `tamanu-accepted`, `tamanu-failed`) in `to_json`, the parser and the type-name list; pin `act` critical for `tamanu-act` in `critical_members`
- [ ] `crates/bliti-core/src/channel/generate.rs`: generators for the five messages
- [ ] `crates/bliti-core/src/channel/messages/tests.rs`: round trip and critical-member cases, as for the Tailscale messages
- [ ] Add the new variants to the exhaustive matches that M3 extended: `crates/bliti/src/client.rs` (diagnostic client), `crates/bliti/src/session.rs` (the pass-through arm near line 358) and `crates/bliti/src/network/session/open.rs`
- [ ] A corpus entry beside `crates/bliti-wire-compat/corpus/tailscale-act-sign-in-by-key.json` for `tamanu-act`. `wire-breaks.toml` stays empty until the first release

### Reading Tamanu

- [ ] `tamanu/run.rs`: the `Runner` trait and the command implementation: `bestool tamanu find` (absent when it prints nothing), `bestool tamanu status --json` (parse stdout whatever the exit status), `bestool tamanu start`, `bestool tamanu restart`, each with a timeout and the program looked up on the path
- [ ] `tamanu/status.rs`: serde structs for the status JSON, and the reduction to a snapshot: `Running` when `any_short` is false; `Stopped` when every `up`-expected expectation is `down`; otherwise `Degraded` with a reason naming the short expectations. Ignore `unknown` expectations. Web and facility server versions from the `tamanu-frontend` and `tamanu-facility-api` expectations: the highest `version_actual` across running instances, else `expected_version`, with the mismatch and disagreement cases NFO gives `warning`
- [ ] `tamanu/fixtures/`: `status-running.json`, `status-degraded.json`, `status-stopped.json`, `status-drift.json`, and a README saying they are written from `bestool`'s own serialised structs and not captured from a device, until one is
- [ ] `tamanu/state.rs`: `Observed` (`Absent`, `Standing(Snapshot)`, `Broken(reason)`), and the poller task with its session gating and immediate re-poll

### Facts

- [ ] `facts/tamanu.rs`: `tamanu-state`, `tamanu-web-version`, `tamanu-facility-server-version`, mapping each state to its `status` and reason as NFO has it, including `absent` (versions `skipped`), `broken` (versions `broken` with the same reason) and the version `warning` and `failed` cases
- [ ] `crates/bliti/src/facts.rs` and `session.rs::facts_now`: register the module and append the entries, which the two-second fact poll then sends on change and ends as needed

### The stream and the acts

- [ ] `tamanu/acts.rs`: `Act` (`Start`, `Restart`) with wire names, `listed` by state (`start` while stopped, `restart` while running or degraded, none otherwise), and the `Slot` shared across every session so one act is under way at a time
- [ ] `tamanu.rs::serve`: answer `tamanu` with `tamanu-acts`, re-send it when the listed acts change, answer each `tamanu-act` with `tamanu-accepted` or `cannot-act` (not listed at `$['act']`, busy at `$`), and log every act with the client's `name` and `version`
- [ ] Carry out an accepted act on a task detached from the stream, so it finishes after the stream ends: run the command, re-poll, send `tamanu-failed` on the asking stream if the state does not reach `running`, with a reason naming the services still short, and bound the whole act
- [ ] `session.rs`: route a stream whose first message is `tamanu` to `tamanu::serve`, and a `tamanu-act` outside one to the same no-op arm Tailscale's has
- [ ] `device.rs` and `main.rs`: construct the `Tamanu` handle once, hand it to every session beside `tailscale`, and hand it the sampler's session count. Add `Tamanu::none()` and pass it where the session tests in `crates/bliti/src/session/tests.rs` pass `Tailscale::none()`

### Tests

- [ ] Status reduction: every fixture gives its state, reason and versions; `unknown` expectations do not degrade; disagreeing instances give the highest version and a warning
- [ ] Absent and broken: `find` empty is `absent`; missing program, unparseable output and timeout are `broken`
- [ ] Facts: each state and version case maps to the status and reason NFO gives
- [ ] Acts: listed by state, none while an act is under way, `cannot-act` for an unlisted act and for a second act while one is under way, across two streams
- [ ] The act lifecycle with a fake `Runner`: `starting` or `restarting` is reported from acceptance, the state returns to what is observed when it ends, a run that ends short sends `tamanu-failed` naming the short services, and the act completes after the asking stream closes
- [ ] Stream tests in `tamanu/tests.rs`, driving `serve` over an in-memory duplex stream with a fake `Runner`, as `tailscale/tests.rs` does

### Device

- [ ] `services/README.md`: a "For Tamanu" section saying `bestool` ships in the device's OS image and is run from the path, and that the daemon runs as root, which `bestool` needs to read running container versions
- [ ] Check on the prototype device (the `bliti-prototype` skill) that `bestool tamanu status --json` there matches the fixtures, and replace the fixtures with a capture if it does not
