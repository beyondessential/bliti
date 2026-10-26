# Tamanu status and restart (N3)

Specs: [TMU](../../specs/control/tamanu.md), and the Tamanu section of [NFO](../../specs/device-info.md). Shaped like Tailscale (M3): `crates/bliti/src/tailscale/` and `facts/tailscale.rs` are the template.

## What `bestool` gives

- State and versions come from `bestool tamanu status --json`. It prints the JSON and then exits non-zero when anything is short, so parse stdout whatever the exit status.
- Per expectation the JSON has `name`, `expected_state`, `running`, `min_count`, `status` (`up`, `down`, `short`, `absent`, `forbidden`, `unknown`), `reason`, `expected_version`, and `instances[]` with `version_actual` (only while that instance runs) and `version_status` (`match`, `mismatch`, `unknown`). `any_short` is true for a short or forbidden expectation and for any version mismatch. That is `running` versus not.
- Web is the `tamanu-frontend` expectation, versioned by `TAMANU_FRONTEND_VERSION`. Facility server is `tamanu-facility-api`, versioned by `TAMANU_VERSION`. Both carry `expected_version` even with nothing running, which is the configured fallback the spec asks for.
- `unknown` expectations (a DB signal that could not be read) are not short, and do not count toward `degraded`.
- `stopped` is every `up`-expected expectation at `down`; any other shortfall is `degraded`.
- Acts are `bestool tamanu start` and `bestool tamanu restart`. `start` normalises: starts missing required services and stops and disables forbidden ones, which is what the spec's `start` says. `restart` starts missing ones first, then restarts in bulk or, for the frontend and API, one instance at a time behind a readiness probe. It is slow, so the device must not block its other work on it.
- The daemon runs as root, so `bestool`'s sudo re-exec for reading container versions does not trigger.
- A Seedling host takes a separate path: `status` ignores `--json` and prints a table with no versions, and `start` and `restart` act through the Seedling daemon. State there cannot be read structurally, so the device reports `broken` and lists no act until `bestool` can give structured status on Seedling.

## Checklist

- [ ] Wire: `tamanu`, `tamanu-acts`, `tamanu-act`, `tamanu-accepted`, `tamanu-failed` in `bliti-core` messages, with `ACT` critical, and the corpus entry M3 added for its act
- [ ] `tamanu` module: run `bestool` with a timeout, parse the status JSON into a snapshot (fixtures from a real run, as M3 did for Tailscale), run an act and wait for `running` with a bound
- [ ] `facts/tamanu.rs`: `tamanu-state` and the two version entries, including `broken` and omitted-when-not-installed
- [ ] `starting` and `restarting` reported for the life of an act, re-reading state as the act ends
- [ ] Tamanu stream in the session: acts listed by state, re-sent on change, one act at a time across streams, carried out after the stream ends, logging
- [ ] Tests: state mapping, fixtures for running, degraded, stopped, drift and unreadable; act listing by state; refusal while an act is under way; failure naming the short services
- [ ] `services/README.md`: `bestool` as a runtime dependency of a device that reports Tamanu
