# Tailscale status and control

Implements the Tailscale facts of NFO ("Tailscale") and the Tailscale stream of TSC on the device, with the matching message types in `bliti-core`. No client UI: the Status screen row and the Control screen section belong to H3.

## Notes

### Shape

- A new `crates/bliti/src/tailscale.rs`, with submodules, laid out as `power.rs` and `network.rs` are: a client for tailscaled, a watcher that holds the state, a controller shared by every session, the netcheck runner, and the tailnet restriction.
- The controller is one shared object, as `power::Controller` is, because the one-act-at-a-time rule spans every Tailscale stream on every channel. It holds the act under way, the cached state, and a watch channel the streams listen on for a change in the acts they list.
- `session.rs` gains a `Message::Tailscale` arm beside `Power` and `Curve`, handing the stream to `tailscale::serve`. The `Context` it passes around gains the controller.

### Talking to tailscaled

- State, the acts and ping go through tailscaled's local API on its unix socket (`/var/run/tailscale/tailscaled.sock`), as a small client of our own over `hyper` and `hyper-util`. No crate covers it: `tailscale-localapi` offers status, whois and certificates only, none of sign-in, logout, start, preferences or ping.
- `netcheck` goes through the `tailscale netcheck --format=json` command, because Tailscale runs that probe in the CLI process and not in the daemon, which is also why it works for a device that is not signed in. A subprocess is already out of pattern for this daemon, which talks D-Bus and netlink, so it is confined to this one module.
- The local API is not a stability promise by Tailscale. The client is therefore one module behind a trait, so everything above it is tested against a fake, and the image pins the `tailscale` package it ships (see "Deployment").
- Checked against Tailscale 1.104.2, on a signed-in machine for the read-only calls, and for the rest in a sandbox: a Headscale control server in a container and a throwaway `tailscaled` in userspace networking, with its own socket and state, so nothing touches a real tailnet. The recorded bodies are in `crates/bliti/src/tailscale/fixtures/`, with a README saying how to rebuild the sandbox.
- A write to `prefs` from an ordinary user is refused with a 403 (`prefs write access denied`). tailscaled accepts writes from root or from its configured operator, so the daemon's running as root is what makes every act possible.
- Turn on and off: `PATCH prefs` with the masked-prefs body flattened, `{"WantRunningSet": true, "WantRunning": false}` and `true`. Stopped and Running were each reached within a second. A body nesting the value under `Prefs` turned it off once and then did not turn it on, so only the flat form is used.
- Sign in by key: read `prefs`, set `ControlURL` where the device uses another control server, `WantRunning` and `Hostname`, `POST start` with `{"UpdatePrefs": <those prefs>, "AuthKey": <key>}`, then `POST login-interactive`, as `tailscale up` does when there is no node key. The control server must travel in `start`: patching `ControlURL` through `prefs` does not reach the running control client, which went on talking to Tailscale's own. A good key logs in within about a second (`LoginFinished` on the bus) and reaches Running within two.
- A rejected key leaves the state at `NeedsLogin` with no failure status. The bus repeats an `ErrMessage` with backoff, and `Health` says "the last login error was: ...". The text did not echo the key, only its length, but the client scrubs the key from it regardless.
- Sign in by link: the same `start` without a key, then `login-interactive`. The URL arrives as `BrowseToURL` on the bus within a second, and a second `login-interactive` produces a new URL. `LoginFinished` follows the person's approval, then `Starting` and `Running`.
- Logout: `POST logout` answers 204 and the states go `Stopped`, `NoState`, `NeedsLogin`, with `LoggedOut` true and `WantRunning` false.
- Recovery from an expired sign-in is the same `start` with a key and `login-interactive`, and Running follows within two seconds.
- While stopped, `status` keeps `Self`, `CurrentTailnet` and `KeyExpiry`, `HaveNodeKey` stays true, `Peer` is null, and `Health` leads with "Tailscale is stopped.". That matches NFO, which keeps the name, tailnet and key expiry `passed` while `stopped`. The `Health` text is only a connection warning while `running`.

### State

- A watcher task subscribes to `watch-ipn-bus` and holds the latest status, so state changes arrive as events and nothing polls `status` on a Raspberry Pi. It reconnects with a short delay when the socket goes away, and holds `broken` for as long as tailscaled does not answer.
- `facts()` stays synchronous and reads the cached state, so the 2-second facts poll of `serve_default` sends a change within two seconds, which is how `wired-carrier` is observed too. No new push path.
- A bus message carries `State` as a number, `Prefs`, `BrowseToURL`, `ErrMessage` and `LoginFinished`; only `Running` (6) has been seen. The watcher uses the bus as the trigger and maps `status`'s `BackendState` name, so the numbers are never ours to get wrong.
- Mapping to `tailscale-state`, from `status`: `Running` is `running`, `Starting` and `NoState` are `starting`, `Stopped` is `stopped`, `NeedsMachineAuth` is `needs-approval`, and `NeedsLogin` is `expired` where `Self.Expired` is true, which a control-plane expiry produced alongside a past `KeyExpiry`, `HaveNodeKey` true and `LoggedOut` false, and `needs-login` where there is no node key, as after a logout or on a device that never signed in. `NeedsMachineAuth` was not seen, since Headscale does not emit it, and is from Tailscale's source; the others are fixtures.
- `tailscale-name` is the device's MagicDNS name without its trailing dot, `tailnet` is `CurrentTailnet.Name` (`bes.au` on ours; the control server's host on Headscale), `tailscale-key-expiry` is `Self.KeyExpiry`, where null or absent, or Go's zero time, means the sign-in does not lapse. The connection `warning` carries the first of `Health` as its reason, while `running` only: a stopped or logged-out device leads with "Tailscale is stopped.".
- The device reports none of the four entries where `tailscaled` is not installed, found by the socket's absence at startup and not at each poll, so a daemon that is down is `broken` and a daemon that was never there is nothing.

### Acts

- The acts listed are a pure function of the cached state, in one place, and the table in TSC is its test. A change in them is pushed as `tailscale-acts` on every open stream from the same watch channel.
- An act holds one slot in the controller, taken on acceptance and released on carried out, failed or ended. The refusal names the act under way.
- A transition act is carried out when the watcher sees the target state, or fails on a deadline. The deadlines are constants in one place, since TSC leaves the bounds to us: a few tens of seconds for `turn-on`, `turn-off` and `log-out`, a few minutes for `sign-in` so a person has time to open a link.
- An act waiting on the outside (`netcheck`, `ping`, a login link) runs as a task owned by a guard held by its stream, so the stream ending by any route drops the guard and the task with it. The other acts are spawned on the runtime and outlive the stream, as TSC requires.
- Sign-in by `auth-key` reads `prefs`, sets `WantRunning` and the hostname, calls `start` with those prefs and the key, then `login-interactive`. Sign-in by `login-link` is the same without a key, and forwards each `BrowseToURL` the bus announces as `tailscale-login-link`. The key and the URL are held in memory only, never in a `tracing` field, an error chain or a `reason`, so the error types in the client carry no request bodies.
- The log line for every act asked for copies the shape of `Controller::ask` in `power.rs`, with the client's `name` and `version` from `Peer`.

### Tailnet restriction

- Set by a flag on the daemon, `--tailnet <name>`, compared with `CurrentTailnet.Name`. It is set in the service unit, which is where our own tailnet is hardcoded.
- At startup the daemon reads the status once. Signed in to another tailnet is a refusal to start with an error saying so. A daemon that does not answer is not a refusal, since the restriction applies when something signs in, and the device stays reachable to diagnose.
- After every `sign-in` reaches `running` or `needs-approval`, the controller compares the tailnet. A mismatch calls `logout`, then fails the act with the reason TSC gives.
- Tailscale also has a feature called Tailnet Lock, which signs node keys and is unrelated. The flag and the code avoid that name.

### Netcheck and relay names

- `netcheck` runs the `tailscale netcheck --format=json` command, as decided. Its JSON names regions by number with latency in nanoseconds and no names.
- A device that has never signed in has no relay map in its daemon: `derpmap` answers 200 with `null`. The command then fetches Tailscale's default map over HTTPS from its control plane, and aborts with "netcheck cannot continue" if it cannot. So where a network blocks that control plane, the unsigned test fails with the command's reason as a `tailscale-failed`, which says the control plane is out of reach but not what else is wrong. TSC's note is worded to claim no more than that.
- Names come from the daemon's map, served at `derpmap` with each region's `RegionID`, `RegionCode` and `RegionName` once the device has signed in. We cache that map in the state directory (`/var/lib/bliti`) every time the daemon sees one, so a device that has been signed in names relays afterwards, signed in or not. A device that never has reports `name` as null.
- `udp` is the `UDP` member; `ipv4` and `ipv6` are `IPv4` and `IPv6`; `nat-varies` is `MappingVariesByDestIP`, null where it is null; `captive-portal` is `CaptivePortal`. `relays` is the region latencies sorted ascending and converted to seconds, with the four-decimal rounding NFO applies to facts left alone since results are not facts.

### Ping

- `tailscale ping` has no JSON output, so ping uses the local API's `ping` with the disco type, which answers one probe per request, as a 200 whatever the outcome, with `LatencySeconds`, `Endpoint`, `DERPRegionID`, `DERPRegionCode` and an `Err`. A relayed answer has an empty `Endpoint` and the relay's region; a direct one has `Endpoint` set to the peer's address and a region of 0 with no code; an unknown peer is `Err` "no matching peer". The call takes an IP address only, so `PEER` is resolved against the status's peers by `HostName`, or by `DNSName` without its trailing dot, to the first of its `TailscaleIPs`. The device sends probes until one is direct or the probe count is spent, and reports `direct` if any was.
- With no `PEER`, the device pings a default given by a second daemon flag, `canopy` in our service unit. No flag, no default, and the act fails as TSC says.

### Wire

- New `Message` variants in `bliti-core/src/channel/messages.rs`: `Tailscale`, `TailscaleActs`, `TailscaleAct`, `TailscaleAccepted`, `TailscaleLoginLink`, `TailscaleResult`, `TailscaleFailed`, each with parse and serialise arms, an entry in the proptest generator in `generate.rs`, and a corpus line.
- `TailscaleAct` is read as `Act` is, with the critical members `ACT`, `METHOD`, `KEY`, `NAME` and `PEER` in upper case on the wire and a member absent where TSC says it may be. Each critical member gets an entry in `wire-breaks.toml` with its reason, or the ledger test fails.
- `session.rs` matches `Message` exhaustively, so each new variant needs an arm, the device-only ones joining the "answers only a device sends" arm.
- `bliti-web` needs no change: it builds only the messages a client sends and holds no exhaustive match on `Message`. The control screen's use of these streams is H3's.

### Deployment

- `services/bliti.service` gains `After=tailscaled.service` and no `Requires`, so the daemon starts when Tailscale is not installed or is down.
- `services/README.md` lists `tailscale` as a package, pinned.
- The daemon runs as root already, which the socket needs.

### Tests

- The client is tested against a fake local API on a temporary unix socket, serving recorded responses for each state, so the whole controller is exercised without a Tailscale.
- Controller tests cover the act table by state, the one-act lock across two streams, a stream ending mid-`netcheck`, a stream ending mid-`turn-on` leaving it to finish, a deadline expiring into `tailscale-failed`, the restriction refusing a foreign sign-in, and the secrets staying out of the log.
- On the prototype device (see the `bliti-prototype` skill), by hand: sign in by auth key and by login link, turn off and on, log out, netcheck while signed out, ping the default.

### To confirm on the prototype before building

- What Tailscale's own control plane does differently from Headscale: `NeedsMachineAuth` for a device awaiting approval, the shape of a lapsed key, and the text of a rejected key.
- That the pinned `tailscale` package is the version the shapes above were recorded against.

## Confirm on the prototype first

Read-only parts of the local API are already confirmed on a signed-in machine, as the notes record. What remains changes a device's standing and decides the shape of the client's acts, so it is confirmed before they are written. Record what is found in the notes above, and keep the recorded payloads as the fixtures the tests replay.

- [x] Read `status`, `prefs`, `watch-ipn-bus`, `derpmap` and a `ping` on a signed-in machine, and note the shapes
- [x] Save a `status` body for each state the sandbox can be put in, signed in, stopped, expired and signed out, and a `ping` body for each outcome, under `crates/bliti/src/tailscale/fixtures/`, with keys and addresses blanked
- [x] Turn Tailscale off and on through `prefs`, and note the body, the states reached and the events
- [x] Confirm the requests that sign in with an auth key and a hostname, start a login by link and announce its URL on the bus, and log out, against a sandbox control server; the sequences are in the notes
- [x] Confirm what `expired` looks like, and fix the mapping in the notes
- [x] Confirm whether `derpmap` answers on a device that has never signed in: it does not, and the netcheck command's behaviour there is in the notes
- [x] Capture a `ping` answer that is direct, and note how it differs from the relayed one
- [ ] On the prototype device, confirm the shapes above against Tailscale's own control plane: a device awaiting approval (`NeedsMachineAuth`), a lapsed key, and a rejected key's message (see the `bliti-prototype` skill, and take care that nothing is signed out over the Tailscale it is reached by)
- [x] `crates/bliti/src/tailscale/fixtures/README.md`: how the fixtures were recorded and how to rebuild the sandbox, so they can be re-recorded when the pinned package moves

## Wire (`bliti-core`)

- [ ] `crates/bliti-core/src/channel/messages.rs`: add variants `Tailscale`, `TailscaleActs { acts }`, `TailscaleAct { act, method, key, name, peer }` (the last four optional), `TailscaleAccepted`, `TailscaleLoginLink { url }`, `TailscaleResult { result }` (a JSON object), `TailscaleFailed { reason }`, with doc comments citing TSC
- [ ] Same file: `to_json` and the parse arms for each, `type` names `tailscale`, `tailscale-acts`, `tailscale-act`, `tailscale-accepted`, `tailscale-login-link`, `tailscale-result`, `tailscale-failed`, and the type-name list the file keeps beside `"acts"`, `"act"` and `"accepted"`
- [ ] Same file: `critical_members("tailscale-act")` returns `act`, `method`, `key`, `name`, `peer`, with a comment saying why each is critical, as `"act"` has
- [ ] `crates/bliti-core/src/channel/generate.rs`: generate every new variant, the optional members of `TailscaleAct` present and absent, beside the power stream's
- [ ] `crates/bliti-core/src/channel/messages/tests.rs`: round trip each variant, and a test that `tailscale-act` serialises its members in upper case and reads them back from upper case only, and that an unknown critical member makes the message unprocessed, as the existing `act` tests do
- [ ] `wire-breaks.toml`: five `[[critical]]` entries for `tailscale-act`, one per member, each with its reason from the TSC note; run `cargo test -p bliti-wire-compat` and add what the ledger test asks for
- [ ] `crates/bliti-wire-compat/corpus/`: one raw `tailscale-act` file pinning a `sign-in` by `auth-key` with `ACT`, `METHOD`, `KEY` and `NAME` as they go on the wire
- [ ] `crates/bliti-web/src/tests.rs`: no change expected; confirm the crate builds for `wasm32-unknown-unknown` with the new variants

## Device: the client

- [ ] `cargo add` in `crates/bliti`: `hyper` (client, http1), `hyper-util` (tokio), `http-body-util` and `bytes`, versions as `cargo add` resolves them; add the `process` feature to `tokio` for netcheck
- [ ] `crates/bliti/src/tailscale.rs`: module root, with `mod client; mod state; mod acts; mod netcheck; mod relays; mod ping; mod restriction;`, `use` before `mod`
- [ ] `tailscale/client.rs`: a `LocalApi` trait with `status`, `watch` (a stream of notifications), `prefs`, `set_running(bool)` (the flat masked-prefs body), `start(prefs, auth_key)`, `login_interactive`, `logout`, `ping` (one probe), `derp_map`, and an error type that carries no request body, so a key cannot reach a log through one
- [ ] `tailscale/client.rs`: the implementation over `hyper` on `tokio::net::UnixStream` at the default socket path, gated on `cfg(unix)` as `battery::socket` is, with a per-call timeout
- [ ] `tailscale/client.rs`: a fake `LocalApi` for tests, holding a scripted status and recording the calls it receives

## Device: state and facts

- [ ] `tailscale/state.rs`: `State` with the six wire names and the `status` each carries (NFO "Tailscale"), and `Snapshot` holding the state, name, tailnet, key expiry and the first health message
- [ ] `tailscale/state.rs`: `Snapshot::from_status(&Status, now)` as a pure function implementing the mapping in the notes, tested against each recorded fixture
- [ ] `tailscale/state.rs`: the watcher task, reading `watch` and re-reading `status` on each notification, publishing on a `tokio::sync::watch`, holding `Broken(reason)` while the socket does not answer and retrying with a delay
- [ ] `tailscale.rs`: the `Tailscale` handle, cheap to clone, with `none()` for a device without Tailscale and `start(socket, options)` otherwise, found by the socket's absence at startup
- [ ] `crates/bliti/src/facts/tailscale.rs`: `entries(at, &Snapshot, now)` yielding `tailscale-state`, `tailscale-name`, `tailnet` and `tailscale-key-expiry` with the statuses and reasons NFO gives, and nothing for a device without Tailscale
- [ ] `facts/tailscale.rs` tests, over hand-built snapshots: every state and its `status`; `running` with a health message is `warning`; key expiry `passed`, `warning` inside seven days, `failed` once lapsed, `skipped` where it does not lapse or none is held, each with its reason; name and tailnet `skipped` while signed out and `passed` while `expired`; all four `broken` with the state's reason; all four absent without Tailscale
- [ ] `crates/bliti/src/session.rs`: `facts_now` takes the `Tailscale` handle and appends `facts::tailscale::entries`, both call sites updated; a test that `same_facts` sees a state change as a change

## Device: acts

- [ ] `tailscale/acts.rs`: `Act` with `name` and `from_name` as `power::Act` has, and `listed(&Snapshot, netcheck_available) -> Vec<Act>` as a pure function; a table-driven test of the TSC table across every state, `broken`, and no Tailscale
- [ ] `tailscale/acts.rs`: the slot, taken on acceptance and released on drop, and `ask(request, peer)` returning either an accepted run or a `CannotAct { at, reason }` for an act not listed (`$['act']`), another under way (`$`), an unknown `METHOD` (`$['method']`) and a missing `KEY` (`$['key']`), logging the client's `name` and `version` as `Controller::ask` does
- [ ] `tailscale/acts.rs`: carrying out `turn-on`, `turn-off` and `log-out`, each by one `LocalApi` call and then waiting on the watcher for the target state with a deadline, failing with `tailscale-failed` on the deadline or on the call's error
- [ ] `tailscale/acts.rs`: `sign-in` by `auth-key`, passing `NAME` or the device's hostname, reading and updating `prefs` and calling `start` then `login-interactive`, waiting for `running` or `needs-approval` with the sign-in deadline; a rejected key arrives as an `ErrMessage` on the bus with no change of state, so the first one seen after `login-interactive` is the `tailscale-failed` reason, with the key scrubbed from it
- [ ] `tailscale/acts.rs`: `sign-in` by `login-link`, forwarding each URL the bus announces as `tailscale-login-link`, starting it as the key sign-in does without a key, failing when the link lapses or the deadline passes
- [ ] `tailscale.rs`: `serve(stream, tailscale, peer)` answering `tailscale` with `tailscale-acts`, then selecting on stream reads, the watcher (pushing `tailscale-acts` when the list changes) and an act's messages, answering each act with `tailscale-accepted` or `cannot-act`
- [ ] `tailscale.rs`: acts that wait on the outside (`netcheck`, `ping`, a login link) run on tasks the stream owns and aborts when it ends by any route; the others are spawned detached, their messages dropped once the stream is gone
- [ ] `crates/bliti/src/session.rs`: a `Message::Tailscale` arm in `serve_stream` handing the stream to `tailscale::serve`, a no-op arm for a `TailscaleAct` outside a Tailscale stream, the device-only variants added to the "answers only a device sends" arm, and `Tailscale` added to `Context`
- [ ] Controller tests with the fake client: acceptance and the busy refusal across two streams; a stream dropped mid-`sign-in` by login link leaves the state as it stood; a stream dropped mid-`turn-on` leaves it to finish; each deadline becomes `tailscale-failed`; the state change after `turn-off` pushes a new `tailscale-acts`; an auth key is absent from every log line and every message the device sends, asserted over a captured `tracing` output

## Device: netcheck, relays and ping

- [ ] `tailscale/netcheck.rs`: run `tailscale netcheck --format=json` with a timeout and map its output to the TSC result, `relays` sorted by latency in seconds with the relay object of TSC; the runner behind a trait so tests supply recorded output
- [ ] `tailscale/netcheck.rs` tests over recorded output: all-reachable, UDP blocked with HTTPS-only relays, IPv6 absent, a captive portal, `null` `MappingVariesByDestIP`, and no relay answering (an empty `relays` with `udp` false), the last a `tailscale-result` and not a failure; a runner that cannot start, or exits because it could not fetch the relay map, is `tailscale-failed` with its own reason
- [ ] `tailscale/relays.rs`: the relay name cache in the state directory, written atomically, read at start, refreshed from the local API's `derpmap` when the device enters `running` and on each netcheck where it answers with regions, since a device that has never signed in answers with none; a missing or unreadable file is an empty cache and logged, never an error
- [ ] `tailscale/relays.rs` tests over a temporary directory: a name found, an id not in the cache giving a null name, a corrupt file, a refresh replacing the map
- [ ] `tailscale/ping.rs`: resolve `PEER` against the peers in the status (bare or fully qualified name, never an address), or the configured default where absent; no default or no such peer is `tailscale-failed`
- [ ] `tailscale/ping.rs`: send probes until one is direct or the count is spent, report `reachable`, `path` as `direct` if any probe was, `relay` as a relay object for a relayed path, `latency` from the last answer on the reported path, and `peer`; no answer is a result with `reachable` false
- [ ] `tailscale/ping.rs` tests with the fake client: direct on the first probe, relayed then direct, relayed only, no answer, unknown peer, an address given as `PEER`, no default configured

## Device: tailnet restriction and the daemon

- [ ] `tailscale/restriction.rs`: `check_at_startup(&dyn LocalApi, Option<&str>)` returning an error naming both tailnets where the device is signed in to another, and `Ok` where it is signed out, in the accepted tailnet, or tailscaled does not answer
- [ ] `tailscale/acts.rs`: after a `sign-in` reaches `running` or `needs-approval`, compare the tailnet with the restriction, and on a mismatch call `logout` and fail with the reason TSC asks for
- [ ] `crates/bliti/src/main.rs`: `--tailnet <name>` and `--ping-default <name>` on the `Daemon` command, passed through `daemon()` to `device::run`, with the doc comments the other flags have
- [ ] `crates/bliti/src/device.rs`: build the `Tailscale` handle before the adapter is touched, so the restriction's refusal stops the daemon before it advertises; pass it through `Shared` and `session::run` beside the controller
- [ ] Restriction tests with the fake client: refuses another tailnet, accepts the same, accepts signed out, starts when the socket is dead, a foreign sign-in is logged out and failed, and an unrestricted device accepts any

## Deployment

- [ ] `services/bliti.service`: `After=tailscaled.service` with no `Requires`, and a comment saying why
- [ ] `services/bliti.service`: the `--tailnet` and `--ping-default` arguments for our deployment, with a comment that they are the fleet's
- [ ] `services/README.md`: list `tailscale` under the packages the device needs, pinned, and say what depends on its local API

## Finish

- [ ] `cargo fmt`, `cargo clippy` and `cargo test` for the workspace, including `-p bliti-wire-compat`
- [ ] Build for `wasm32-unknown-unknown` with `just build-wasm` to confirm `bliti-core` still builds for the client
- [ ] By hand on the prototype device: sign in by auth key and by login link, turn off and on, log out, netcheck while signed out and signed in, ping the default and a named peer, and start the daemon with `--tailnet` set to another tailnet and see it refuse
