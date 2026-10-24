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
- Checked against Tailscale 1.104.2 on a signed-in machine, read-only: `status` (and `status?peers=false`), `prefs`, `watch-ipn-bus`, `derpmap` and `ping`. Turning on and off was run on the same machine with a root-privileged request: `PATCH prefs` with the masked-prefs body flattened, `{"WantRunningSet": true, "WantRunning": false}` and `true`. Stopped was reached within a second and Running again within a second. A body nesting the value under `Prefs` turned it off once and then did not turn it on, so only the flat form is used. Not checked: `start` carrying an auth key, `login-interactive` and `logout`, which are from memory of Tailscale's own CLI and are confirmed before building on them. A write to `prefs` from an ordinary user is refused with a 403 (`prefs write access denied`), which was seen; tailscaled accepts writes from root or from its configured operator, so the daemon's running as root is what makes every act possible.
- While stopped, `status` keeps `Self`, `CurrentTailnet` and `KeyExpiry`, `HaveNodeKey` stays true, `Peer` is null, and `Health` leads with "Tailscale is stopped.". That matches NFO, which keeps the name, tailnet and key expiry `passed` while `stopped`. The `Health` text is only a connection warning while `running`.

### State

- A watcher task subscribes to `watch-ipn-bus` and holds the latest status, so state changes arrive as events and nothing polls `status` on a Raspberry Pi. It reconnects with a short delay when the socket goes away, and holds `broken` for as long as tailscaled does not answer.
- `facts()` stays synchronous and reads the cached state, so the 2-second facts poll of `serve_default` sends a change within two seconds, which is how `wired-carrier` is observed too. No new push path.
- A bus message carries `State` as a number, `Prefs`, `BrowseToURL`, `ErrMessage` and `LoginFinished`; only `Running` (6) has been seen. The watcher uses the bus as the trigger and maps `status`'s `BackendState` name, so the numbers are never ours to get wrong.
- Mapping to `tailscale-state`, to confirm on the device: `Running` is `running`, `Starting` and `NoState` are `starting`, `Stopped` is `stopped`, `NeedsMachineAuth` is `needs-approval`, and `NeedsLogin` is `needs-login`, or `expired` where the node key is held and its expiry has passed. The `expired` test is the least certain mapping and gets a test against a recorded status of each kind.
- `tailscale-name` is the device's MagicDNS name without its trailing dot, `tailnet` is `CurrentTailnet.Name`, `tailscale-key-expiry` is `Self.KeyExpiry`, with Go's zero time read as "does not lapse". The connection `warning` carries the first of `Health` as its reason.
- The device reports none of the four entries where `tailscaled` is not installed, found by the socket's absence at startup and not at each poll, so a daemon that is down is `broken` and a daemon that was never there is nothing.

### Acts

- The acts listed are a pure function of the cached state, in one place, and the table in TSC is its test. A change in them is pushed as `tailscale-acts` on every open stream from the same watch channel.
- An act holds one slot in the controller, taken on acceptance and released on carried out, failed or ended. The refusal names the act under way.
- A transition act is carried out when the watcher sees the target state, or fails on a deadline. The deadlines are constants in one place, since TSC leaves the bounds to us: a few tens of seconds for `turn-on`, `turn-off` and `log-out`, a few minutes for `sign-in` so a person has time to open a link.
- An act waiting on the outside (`netcheck`, `ping`, a login link) runs as a task owned by a guard held by its stream, so the stream ending by any route drops the guard and the task with it. The other acts are spawned on the runtime and outlive the stream, as TSC requires.
- Sign-in by `auth-key` goes through `start` with the key and optional hostname. Sign-in by `login-link` calls `login-interactive` and forwards each URL the bus announces as `tailscale-login-link`. The key and the URL are held in memory only, never in a `tracing` field, an error chain or a `reason`, so the error types in the client carry no request bodies.
- The log line for every act asked for copies the shape of `Controller::ask` in `power.rs`, with the client's `name` and `version` from `Peer`.

### Tailnet restriction

- Set by a flag on the daemon, `--tailnet <name>`, compared with `CurrentTailnet.Name`. It is set in the service unit, which is where our own tailnet is hardcoded.
- At startup the daemon reads the status once. Signed in to another tailnet is a refusal to start with an error saying so. A daemon that does not answer is not a refusal, since the restriction applies when something signs in, and the device stays reachable to diagnose.
- After every `sign-in` reaches `running` or `needs-approval`, the controller compares the tailnet. A mismatch calls `logout`, then fails the act with the reason TSC gives.
- Tailscale also has a feature called Tailnet Lock, which signs node keys and is unrelated. The flag and the code avoid that name.

### Netcheck and relay names

- The JSON names regions by number with latency in nanoseconds and has no names. Names come from the DERP map, which the local API serves at `derpmap` with each region's `RegionID`, `RegionCode` and `RegionName`. We cache the map in the state directory (`/var/lib/bliti`) every time the daemon sees one, so a device that has ever been signed in names relays afterwards, signed in or not. A device that never has reports `name` as null.
- Whether `derpmap` answers on a device that has never signed in is not known and is checked on the prototype. If it does, the cache is a fallback only.
- `udp` is the `UDP` member; `ipv4` and `ipv6` are `IPv4` and `IPv6`; `nat-varies` is `MappingVariesByDestIP`, null where it is null; `captive-portal` is `CaptivePortal`. `relays` is the region latencies sorted ascending and converted to seconds, with the four-decimal rounding NFO applies to facts left alone since results are not facts.

### Ping

- `tailscale ping` has no JSON output, so ping uses the local API's `ping` with the disco type, which answers one probe per request with `LatencySeconds`, `Endpoint`, `DERPRegionID`, `DERPRegionCode` and an `Err`. A relayed answer was seen with an empty `Endpoint` and the relay's region; the direct shape, presumably a set `Endpoint`, is not yet seen. The device sends probes until one is direct or the probe count is spent, and reports `direct` if any was.
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

- The requests that change standing: sign-in with a key, login by link, logout, turning on and off.
- Whether `derpmap` answers on a device that has never signed in.
- What `expired` looks like in status and on the bus, and the other backend state names.
- What a direct `ping` answer carries.

## Confirm on the prototype first

Read-only parts of the local API are already confirmed on a signed-in machine, as the notes record. What remains changes a device's standing and decides the shape of the client's acts, so it is confirmed before they are written. Record what is found in the notes above, and keep the recorded payloads as the fixtures the tests replay.

- [x] Read `status`, `prefs`, `watch-ipn-bus`, `derpmap` and a `ping` on a signed-in machine, and note the shapes
- [ ] Save a `status` body for each state the prototype can be put in, signed in, signed out, stopped, and with the key expired, under `crates/bliti/src/tailscale/fixtures/`, with keys and addresses blanked
- [x] Turn Tailscale off and on through `prefs`, and note the body, the states reached and the events
- [ ] Confirm the requests that sign in with an auth key and a hostname, start a login by link and announce its URL on the bus, and log out, on a machine that can be repaired by hand and not over Tailscale; write down the verbs, paths and bodies in the client's module comment
- [ ] Confirm what `expired` looks like (backend state, `HaveNodeKey`, `Self.KeyExpiry`, bus events), and fix the mapping in the notes
- [ ] Confirm whether `derpmap` answers on a device that has never signed in; if it does, record that the cache is a fallback only
- [ ] Capture a `ping` answer that is direct, and note how it differs from the relayed one

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
- [ ] `tailscale/client.rs`: a `LocalApi` trait with `status`, `watch` (a stream of notifications), `set_running(bool)`, `sign_in_with_key(key, hostname)`, `login_interactive`, `logout`, `ping` (one probe), `derp_map`, and an error type that carries no request body, so a key cannot reach a log through one
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
- [ ] `tailscale/acts.rs`: `sign-in` by `auth-key`, passing `NAME` or the device's hostname, waiting for `running` or `needs-approval` with the sign-in deadline, and a rejected key as `tailscale-failed` whose reason carries the client's message and never the key
- [ ] `tailscale/acts.rs`: `sign-in` by `login-link`, forwarding each URL the bus announces as `tailscale-login-link`, failing when the link lapses or the deadline passes
- [ ] `tailscale.rs`: `serve(stream, tailscale, peer)` answering `tailscale` with `tailscale-acts`, then selecting on stream reads, the watcher (pushing `tailscale-acts` when the list changes) and an act's messages, answering each act with `tailscale-accepted` or `cannot-act`
- [ ] `tailscale.rs`: acts that wait on the outside (`netcheck`, `ping`, a login link) run on tasks the stream owns and aborts when it ends by any route; the others are spawned detached, their messages dropped once the stream is gone
- [ ] `crates/bliti/src/session.rs`: a `Message::Tailscale` arm in `serve_stream` handing the stream to `tailscale::serve`, a no-op arm for a `TailscaleAct` outside a Tailscale stream, the device-only variants added to the "answers only a device sends" arm, and `Tailscale` added to `Context`
- [ ] Controller tests with the fake client: acceptance and the busy refusal across two streams; a stream dropped mid-`sign-in` by login link leaves the state as it stood; a stream dropped mid-`turn-on` leaves it to finish; each deadline becomes `tailscale-failed`; the state change after `turn-off` pushes a new `tailscale-acts`; an auth key is absent from every log line and every message the device sends, asserted over a captured `tracing` output

## Device: netcheck, relays and ping

- [ ] `tailscale/netcheck.rs`: run `tailscale netcheck --format=json` with a timeout and map its output to the TSC result, `relays` sorted by latency in seconds with the relay object of TSC; the runner behind a trait so tests supply recorded output
- [ ] `tailscale/netcheck.rs` tests over recorded output: all-reachable, UDP blocked with HTTPS-only relays, IPv6 absent, a captive portal, `null` `MappingVariesByDestIP`, and no relay answering (an empty `relays` with `udp` false), the last a `tailscale-result` and not a failure; a runner that cannot start is `tailscale-failed`
- [ ] `tailscale/relays.rs`: the relay name cache in the state directory, written atomically, read at start, refreshed from the local API's `derpmap` when the device enters `running` and on each netcheck where it answers; a missing or unreadable file is an empty cache and logged, never an error
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
