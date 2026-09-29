# T2: calibrated battery charge and low-battery shutdown

Charge from the cell voltage against a learnt curve, time left with a margin, an orderly shutdown at the floor, and the curve exposed over a curve stream and the command line. Along the way the control stream becomes the power stream, and `going-away` gains a cause. Specs: CHG, CRV, LOW, CTL, CSCR, NFO, VIEW, WEB, DEV.

## Hardware data still needed

- **The measured discharging curve** (CHG): a run on the production cell (Samsung INR21700-58E, CC5563F101, per F1), from full until the board gives out, logged every 10 s. The branch ships an interim curve (below) and may merge with it; the measured curve replaces it on a follow-up card.
- **What a finished charge looks like** in the cell voltage on each board, for CHG's "full as the voltage shows". During constant-voltage charging the voltage already sits at termination with charge still going in, so "at termination and still" alone reports full early. What's needed is whatever follows termination (the voltage relaxing back, or the recharge threshold).
- **The gauge's reading on a full cell**, for scaling its figure on mains until three charges have been learnt.
- **An untouched run-down**, still owed from the T2 comments: the true resting voltage after cutoff, and whether the board restarts the Pi by itself. The restart when mains returns is the hardware's own behaviour and stays out of the specs.

## Design notes

- The shutdown watcher belongs with the supply record in `facts/power/record.rs`: it already runs on its own 10 s thread whether or not sampling is on (DEV), which is what LOW requires.
- The shutdown is armed by the hardware, not by the build profile. The watcher only runs where the gauge answers and the `GPIO6` line exists, which is the same guard `record.rs` already applies, so a dev laptop never arms it and a dev build deployed to a prototype Pi does. Test the arming decision on its own, with no gauge and no line, so the guard can't regress into an OS-battery path.
- One supply state, not two histories. The record thread owns it and updates it every 10 s. The sampler's `Watch` reads from it instead of keeping its own 45 s window. Time-left rates need minutes of history, and they must keep accumulating while sampling is off.
- Curves live in `/var/lib/bliti/battery-curve.json`, beside `network.json`, written atomically (temporary file, then rename). The file holds the curve document plus device-local state that isn't part of the document, such as the gauge's full reading. What a run taught must be written before `going-away` is sent (CHG).
- Learning without a current sensor: time at the device's own draw stands in for charge. A run from full rescales the whole curve by the run's duration. A partial run is anchored at the curve's charge where it began, and refines only below. Weighting is exponential over runs, so a replaced cell takes over within a few.
- Charging curve: anchored at the discharging curve's figure when mains returns, and ending at full. Used once `learnt-from` reaches 3, and only within the voltage range it covers.
- The reported charge is renormalised between the floor in force and full, so V2's raised floor needs no re-learning.
- Accuracy and time: each curve carries `error` (a share of a full cell) and `duration` (seconds over its whole scale). The error is measured before each refinement and weighted like the learning; before any measurement it comes from `learnt-from`. `curves` sends `lasts`/`recharge` already scaled to the floor in force, so the client never needs the curve's shape. The live time-left readings use the recent rate, not `duration`. Their margin combines the curve's error at that rate with how much the rate has varied.
- Interim shipped curve: built from the 58E datasheet's low-rate discharge curve. The tail below 3.2 V comes from the 2026-09-25 v4 log (`~/bliti-v4-rundown/rundown.csv`, 3.21 V down to 2.571 V at the cut), whose shape near the cutoff is the board's, not the cell's. Duration: the 58E's rated capacity at the roughly 0.75 A measured draw.
- Command line and daemon: the daemon listens on a root-only Unix socket at `/run/bliti/battery.sock`, and the command line sends `curve`, `load` and `reset` through it as a client would, getting `curves`/`accepted`/`refused` back. Where no daemon answers, the command line works on the curve file directly. The daemon holds the curve in memory, so it must be the one to change it while running.
- `battery_direction` in `facts/power.rs` gives `charging` on mains whenever the gauge reads 99 % or less. Full cells seldom reach that, so a full device on mains reports `charging` forever. CHG's definition of full replaces the test.
- The `bliti-wire-compat` baseline (`c4d4ed6`) predates the control stream, so renaming `control` to `power` and adding `cause` breaks nothing it can see. No critical members are added, so `wire-breaks.toml` is unchanged.

## Rename: control stream to power stream

The CTL rename touches `bliti-core` (`channel/messages.rs`, `channel/generate.rs`), `bliti` (`control.rs`, `session.rs`, `client.rs`, session tests), and `bliti-web` (`lib.rs`, `app/src/Control.jsx`, `app/src/client.js`, `app/tests/control.spec.js`, `app/tests/fake-client.js`). The opening message becomes `power`, and `going-away` gains `cause`.

## Checklist

### Wire messages (`bliti-core`)

- [x] `channel/messages.rs`: `Message::Control` becomes `Message::Power`, on the wire as `power`; update the known-types list and the doc comments.
- [x] `channel/messages.rs`: `GoingAway` gains `cause: String`, always written and required when parsed.
- [x] `channel/messages.rs`: add `Curve`, `Curves { document: Option<Json>, lasts: Option<Span>, recharge: Option<Span> }`, `Load { document: Json }` and `Reset`, where `Span` is `{ duration, margin }` in seconds. `accepted` and `refused` are reused. The document stays raw JSON here; the device validates it.
- [x] `channel/generate.rs`: generators for `power`, the four curve-stream types, and `going-away` with a cause.
- [x] `channel/messages.rs` tests: round-trip and parse for each new or changed type; `going-away` without `cause` fails to parse.
- [x] `cargo test -p bliti-wire-compat` passes unchanged.

### Power stream and going-away cause (`bliti`)

- [x] Rename `crate::control` to `crate::power` (`control.rs` → `power.rs`, `control/systemd.rs`, `control/tests.rs`). "Control stream" becomes "power stream" in every doc comment and log line; `Controller` keeps its name.
- [x] `session.rs`: dispatch `Message::Power` to `power::serve`; update the module doc listing the stream roles.
- [x] `power.rs`: going-away carries `manual-control` for an accepted act.
- [ ] `power.rs`: a low-battery entry point on `Controller` that takes the same accepted slot an act does (so later acts are refused, CTL), sends `going-away` with `power-off` and `low-battery`, then powers off. It is callable from the record thread through a handle.
- [x] `client.rs`: follow the rename.
- [ ] `power/tests.rs`, `session/tests.rs`: rename; `going-away` carries its cause; an act asked for after a low-battery shutdown has begun is refused.

### Supply state and curves (`bliti`, `facts/power/`)

- [ ] `facts/power/supply.rs`: shared supply state (`Arc<Mutex<…>>`), updated by the record thread every 10 s. It holds the voltage and charge history, the curves, the run or charge being recorded, and the confirmation clock. `Watch` in `power.rs` reads its history from here.
- [ ] `facts/power/curve.rs`: `Curve { points, learnt_from, error, duration }` and `Document { discharging, charging }`, serde to CRV's shape with rounding to four places.
- [ ] `facts/power/curve.rs`: validation with CRV's rules, each failure a reason naming what is wrong (e.g. the first point above the floor).
- [ ] `facts/power/curve.rs`: lookup (voltage to charge by interpolation) and renormalisation between the floor and full.
- [ ] `facts/power/shipped-curve.json`: the interim curve, loaded with `include_str!`, with `learnt-from` 0 and the count-derived error.
- [ ] `facts/power/curve.rs`: load `/var/lib/bliti/battery-curve.json` at start, falling back to the shipped curve when missing, and logging and falling back when unreadable. Save atomically.
- [ ] `power.rs` `battery_charge`: discharging curve while external power is absent. On mains, the charging curve where it is learnt from three or more charges and the voltage lies within it, otherwise the gauge scaled by its full reading. 1 once the charge has finished.
- [ ] `facts/power/supply.rs`: detect a finished charge as the voltage reaching termination, then falling back and holding still; tune against the hardware data. Record the gauge's reading at that moment as its full reading.
- [ ] `power.rs` `battery_direction`: on mains, `charging` until full and `idle` once full, replacing the `charge <= 0.99` test.
- [ ] `power.rs`, `battery.rs`: `battery-charge` is `warning` below 0.2 and `failed` below 0.05 while the battery is carrying the device, for backup-supply and OS batteries alike.
- [ ] `facts/power/record.rs`: DEV reports carry our charge beside the gauge's (`charge`, `gauge_charge`).

### Learning (CHG)

- [ ] Record a run from `Lost` (or from start on battery), as timed voltage samples.
- [ ] On a LOW shutdown: measure the discharging curve's error against the run, then refine it (a run from full rescales the whole curve; a partial run is anchored where it began), update `duration` and `learnt-from`, and save, all before `going-away`.
- [ ] Record a charge from `Restored` (a known start) until full. On completion, measure the error, then refine the charging curve (creating it on the first charge), update `duration` and `learnt-from`, and save.
- [ ] Count-derived error for a curve not yet measured, and for the gauge's figure on mains.
- [ ] Report each refinement on stderr with `learnt-from` and the error.
- [ ] Unit tests on synthetic runs: a full run, a partial run, a replaced cell taking over within a few runs, and a run not ending at the floor teaching nothing.

### Time left (NFO, CHG)

- [ ] `battery-time-to-empty` while discharging and `battery-time-to-full` while charging, each with a `margin` trait in seconds. `skipped` until a rate is established; `ended` when the direction changes.
- [ ] Rate from the recent charge history; margin from the figure's error at that rate plus the rate's variation.
- [ ] `lasts` and `recharge` for `curves`, from each curve's duration and error between the floor and full.
- [ ] `upower.rs`: `TimeToEmpty`/`TimeToFull`; `sysfs.rs`: `time_to_empty_now`/`time_to_full_now`. `skipped` where the OS gives none, with no margin.
- [ ] Tests on the rate and margin calculation, and on the entry switching with the direction.

### Low-battery shutdown (LOW)

- [ ] `facts/power/supply.rs`: a confirmation state machine. It arms only where the gauge and `GPIO6` both answer. It powers off after 60 s of readings below 2.8 V with external power absent. A reading at or above the floor, a failed read, or external power returning restarts the count. Nothing begins within 2 minutes of system boot, read from boot time, not from bliti's start.
- [ ] `device.rs`: pass the `Controller` handle into `record_supply`.
- [ ] On confirmation: save the learning, then the low-battery shutdown; report "powering off for low battery" with volts and time away.
- [ ] Where power-off is not available, report so on stderr each time the 60 s is held.
- [ ] Tests: confirmation window, each reset condition, the boot grace, no arming without gauge and line, and no second shutdown after an accepted act.

### Curve stream (CRV)

- [ ] `crate::battery` (new): `serve` for a curve stream. It answers `curve` with `curves`, and answers `load` and `reset` exactly once with `accepted`/`refused`. It refuses both where no backup supply is managed.
- [ ] Broadcast `curves` to every open curve stream on each change (load, reset, refinement) through a `tokio::sync::watch`.
- [ ] Log each `load` and `reset` with the peer's `hello` name and version.
- [ ] `session.rs`: dispatch `Message::Curve` to `battery::serve`.
- [ ] `session/tests.rs`: the exchange, a refused invalid document, `curves` reaching a second stream after a load, and refusal with no backup supply.

### Command line and socket (CRV)

- [ ] `services/bliti.service`: `RuntimeDirectory=bliti`, for `/run/bliti/`.
- [ ] `crate::battery`: listen on `/run/bliti/battery.sock` (mode 0600), speaking newline-delimited JSON messages of the curve stream.
- [ ] `main.rs`: `bliti battery-curve export`, `import <FILE|->` and `reset`. Each goes through the socket where the daemon answers, and works on the curve file directly otherwise. A refusal's reason goes to stderr with a non-zero exit.
- [ ] Log each load and reset from the command line.
- [ ] Tests: the command line against a socket served in-process, and against the file with no daemon.

### Web client (`bliti-web`)

- [ ] `src/lib.rs`: `control()` becomes `power()` and `ControlHandle` becomes `PowerHandle`. Add `curve()`, whose handle has `load(document)`, `reset()` and `close()`, and describe `curves` to JS.
- [ ] `app/src/client.js`: `power()` and `curve()` wrappers.
- [ ] `app/src/Control.jsx`: Battery section after Power, as in the Control screen battery section mockup: the full-charge and recharge lines, the learning sentence, then Export (downloads the document as JSON), Import (file picker, then confirmation) and Reset (confirmation), each with its subtitle, and a refused reason.
- [ ] `app/src/App.jsx`: a `going-away` with cause `low-battery` adds "Its battery is low." under "Shutting down…".
- [ ] `app/src/Readings.jsx`, `app/src/readings.js`: fold `battery-time-to-empty` and `battery-time-to-full` into the battery reveal, labelled "Time left" and "Time to full". Render `margin` as "±" beside a value, for any reading carrying it.
- [ ] `app/src/styles.css`: `.learnt`, `.margin`.
- [ ] `app/tests/fake-client.js`: the curve stream and `going-away` cause.
- [ ] `app/tests/control.spec.js`: the Battery section, its absence until `curves` arrives, the import and reset confirmations, and a refused import.
- [ ] `app/tests/readings.spec.js`: time left and time to full with margins.
- [ ] A test for the low-battery going-away wording.

### Wrap-up

- [ ] `just fmt`, `just clippy`, `just test`, `just test-web`.
- [ ] Deploy a dev build to the v4 prototype (bliti-prototype skill) and check that the shutdown arms there (test cases).
- [ ] Create the follow-up card for the measured 58E curve.
- [ ] Tick covered test cases in `.workhorse/test-cases/t2/overview.md`.
