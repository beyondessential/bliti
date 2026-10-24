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
- The endpoints below are from memory of Tailscale's own CLI and are to be confirmed against the version on the prototype device before building on them: `status`, `watch-ipn-bus`, `prefs` (read and masked patch), `start` carrying an auth key, `login-interactive`, `logout`, `ping`.

### State

- A watcher task subscribes to `watch-ipn-bus` and holds the latest status, so state changes arrive as events and nothing polls `status` on a Raspberry Pi. It reconnects with a short delay when the socket goes away, and holds `broken` for as long as tailscaled does not answer.
- `facts()` stays synchronous and reads the cached state, so the 2-second facts poll of `serve_default` sends a change within two seconds, which is how `wired-carrier` is observed too. No new push path.
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

- The JSON names regions by number with latency in nanoseconds and has no names. Names come from the DERP map, which `tailscale debug derp-map` prints while tailscaled holds one. We cache the map in the state directory (`/var/lib/bliti`) every time the daemon sees one, so a device that has ever been signed in names relays afterwards, signed in or not. A device that never has reports `name` as null.
- Whether tailscaled holds a DERP map before it has ever signed in is not known and is checked on the prototype. If it does, the cache is a fallback only.
- `udp` is the `UDP` member; `ipv4` and `ipv6` are `IPv4` and `IPv6`; `nat-varies` is `MappingVariesByDestIP`, null where it is null; `captive-portal` is `CaptivePortal`. `relays` is the region latencies sorted ascending and converted to seconds, with the four-decimal rounding NFO applies to facts left alone since results are not facts.

### Ping

- `tailscale ping` has no JSON output, so ping uses the local API's `ping` with the disco type, which answers per probe with the latency, whether the endpoint was direct, and the relay's region. The device sends probes until one is direct or the probe count is spent, and reports `direct` if any was.
- With no `PEER`, the device pings a default given by a second daemon flag, `canopy` in our service unit. No flag, no default, and the act fails as TSC says.

### Wire

- New `Message` variants in `bliti-core/src/channel/messages.rs`: `Tailscale`, `TailscaleActs`, `TailscaleAct`, `TailscaleAccepted`, `TailscaleLoginLink`, `TailscaleResult`, `TailscaleFailed`, each with parse and serialise arms, an entry in the proptest generator in `generate.rs`, and a corpus line.
- `TailscaleAct` is read as `Act` is, with the critical members `ACT`, `METHOD`, `KEY`, `NAME` and `PEER` in upper case on the wire and a member absent where TSC says it may be. Each critical member gets an entry in `wire-breaks.toml` with its reason, or the ledger test fails.
- `session.rs` matches `Message` exhaustively, so each new variant needs an arm, the device-only ones joining the "answers only a device sends" arm.
- `bliti-web` has a message list in its tests that needs the same variants.

### Deployment

- `services/bliti.service` gains `After=tailscaled.service` and no `Requires`, so the daemon starts when Tailscale is not installed or is down.
- `services/README.md` lists `tailscale` as a package, pinned.
- The daemon runs as root already, which the socket needs.

### Tests

- The client is tested against a fake local API on a temporary unix socket, serving recorded responses for each state, so the whole controller is exercised without a Tailscale.
- Controller tests cover the act table by state, the one-act lock across two streams, a stream ending mid-`netcheck`, a stream ending mid-`turn-on` leaving it to finish, a deadline expiring into `tailscale-failed`, the restriction refusing a foreign sign-in, and the secrets staying out of the log.
- On the prototype device (see the `bliti-prototype` skill), by hand: sign in by auth key and by login link, turn off and on, log out, netcheck while signed out, ping the default.

### To confirm on the prototype before building

- The local API endpoints and their payloads, for the version the image will ship.
- Whether a device that has never signed in has a DERP map.
- What `expired` looks like in status and in the bus.
- Whether `ping` over the local API reports direct versus relayed per probe.
