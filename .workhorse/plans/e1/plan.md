# Rebuild bliti-web as a real client

The framework every later feature is built on: the prototype browser client replaced by a React single-page application, the wire-contract envelope that every feature inherits, and the serving, CI, and test scaffolding around them. The protocol stays in Rust/wasm and the boundary does not move.

The reasoning behind these decisions lives in `.workhorse/working-docs/d1/working-doc.md` (the "Implementation options" and "Wire contract" headings). This plan executes them.

End state that makes the card reviewable before any feature exists: a sticker read by link or by camera, the device found, the handshake run, the channel open, and a device view present with the device's version shown and no readings on it yet.

## Strip the prototype

- [ ] Remove the hand-written page and script: `crates/bliti-web/www/index.html` and `crates/bliti-web/www/app.js`.
- [ ] Remove the send-text demonstration message and the device-side print behind it:
  - `ClientMessage::Text` in `crates/bliti-core/src/channel/messages.rs` and its tests
  - `Channel::send_text` in `crates/bliti-web/src/lib.rs`
  - the `ClientMessage::Text` arm and its `println!` in `converse`/`serve_stream` in `crates/bliti/src/session.rs`
  - the send-text path in the CLI client (`crates/bliti/src/client.rs`) and its `--text` argument in `crates/bliti/src/main.rs`
- [ ] Remove `DeviceMessage::Unknown` and `parse_client_message`'s error-reply behaviour, replaced by the three outcomes below. Nothing is answered on the wire any more.

Keep everything protocol-shaped: the key schedule, advertisement matching, the handshake, the streams, the framing. The wasm crate keeps its exported surface for those; only the demonstration message goes.

## Wire-contract envelope (BLI-MSG)

The envelope is the part that outlives the card, and it is now specified to the wire in `.workhorse/specs/messages.md`. The test for it: another team could write a client from BLI-CHN and BLI-MSG alone and stay compatible.

Message set this card defines: `client-hello`, `device-hello`, `subscribe`. Nothing else.

- [ ] **Delimiting.** Application messages on a yamux stream are length-delimited with a four-byte big-endian prefix, which `write_message`/`read_message` in `crates/bliti-core/src/channel/stream.rs` already do. Add the one-mebibyte ceiling: `read_message` currently reads a `u32` length and allocates it unbounded, so it needs the same refusal `Reassembler` gives at the Noise layer, closing the stream rather than the connection.
- [ ] **Stream roles.** Device opens its reporting stream on connect and sends `device-hello` first (it already opens one and sends `Identity`; the hello goes in front). Client opens a control stream on connect and sends `client-hello` first. Roles come from who opened the stream and its first message, never from the stream id.
- [ ] **Hello messages.** Both carry `name` and `version` as opaque strings. Nothing parses, compares, or orders them: the client displays the device's, the device logs the client's. Keeping them opaque is what makes the no-gate rule enforceable rather than merely intended.
- [ ] **Member names and criticality.** Names are lower case letters, digits and hyphens, matched case-insensitively, wholly lower or wholly upper. Upper case marks the member critical. Mixed case, or the same name twice in any casing, is malformed. Applies at every depth, to names only and never to values.
- [ ] **Three outcomes, not two.** The parse layer must keep these apart, because collapsing any two of them loses the property the envelope exists for:
  - *fault* (broken peer): not UTF-8, not JSON, not an object, no string `type`, mixed-case or duplicated name, a required member missing or the wrong JSON type, over one mebibyte. Report and close the stream.
  - *critical unknown* (newer peer): an unrecognised upper-case member. Do not process that object, report it, and **leave the stream open**. Not a fault.
  - *ignorable unknown* (newer peer): an unrecognised lower-case member, or an unknown `type`. Skip silently and carry on.
- [ ] **Parsing.** `#[serde(tag = "type", rename_all = "kebab-case")]` cannot express any of this: it is case-sensitive, cannot report which unknown members it saw, and errors where we need three distinct outcomes. Parse in two stages instead. First read to a `serde_json::Map`, validate each name (well formed, not duplicated case-insensitively), and record which arrived upper case while lowering every key. Then deserialise the normalised object into the typed struct, with `#[serde(flatten)] unknown: Map<String, Value>` capturing what the struct does not know. Any captured name that arrived upper case is a critical unknown; the rest are ignorable. Recurse for nested objects.
- [ ] **Growth rule.** Adding a member is a lower-case name if the message still means something without it, an upper-case name if it does not. Neither needs a base version bump. Removing a member, or changing what one means, does.
- [ ] **Client display.** A critical unknown is shown to the operator as "this device said something this version of the app is too old to act on", with everything else still rendered. It must not blank the view, and must not look like a fault.
- [ ] **Subscription streams.** A client subscribes by opening a stream whose first message is `subscribe` with a `topic` string; the device sends that topic's data on that same stream; the client unsubscribes by closing the stream. There is no `unsubscribe` message. One stream per subscription.
- [ ] **End of stream is the unsubscribe, device side.** This is the load-bearing half. `close()` sends FIN and leaves the stream `SendClosed`, which yamux documents as "open for incoming messages" (`connection/stream.rs:57`): it is a half-close, so the device's write side stays open and the device will keep streaming unless it acts on the EOF. On the device, `read_message` returning `Ok(None)` on a subscription stream must stop the topic, close the device's side, and drop the subscription. Without this the page-visibility case silently does nothing.
- [ ] **Any termination unsubscribes.** A reset counts as well as a FIN: dropping a `Stream` in state `Open` sends RST (`connection.rs:516-529`), so a dropped or abandoned stream does reach the device. Handle reset, EOF, and connection loss through the same path, so no drop guard is needed and no client-side discipline is relied on. Prefer an explicit `close()` anyway, so the ordinary path is a graceful FIN rather than a reset.
- [ ] **In-flight data.** Samples already queued when the stream ends may still arrive at the client; discard them rather than erroring.
- [ ] **Unknown topic.** A device that does not know a topic skips the `subscribe` like any unrecognised message and sends nothing, leaving the stream open and empty. This is the older-device path and it must not error.
- [ ] **Page visibility.** The client closes its subscription streams on page-hidden and opens fresh ones on page-shown. Lives in the React app.
- [ ] **No gate above the base version.** The base protocol version in the advertisement is the one version that is switched on, and it covers the crypto, the exchange, and the fact that messages are JSON. The checks in `Sticker::new`/`read_local_name` are that gate and stay. Above it, nothing compares a version.

E1 defines no topic of its own, so the subscription path is exercised by the Playwright harness and by D1's `system` topic. Wire the mechanism, not a stream of readings.

Decide and record here: what `name` and `version` are for each end (crate version, build stamp).

## React single-page application

A React SPA built to static files with Vite, on npm. This brings a node toolchain into a repo that has none — the real cost — in exchange for the most ordinary possible client.

- [ ] Scaffold a vanilla Vite + React app (npm) under `crates/bliti-web/` (app sources alongside the wasm crate). Decide the exact layout: the wasm crate's `src/`/`Cargo.toml` stay; the JS app and its `package.json` sit beside them.
- [ ] Wire the wasm module in: `build.sh` already emits `wasm-bindgen --target web` output; have Vite consume the generated ES module and bindings rather than the browser importing them directly.
- [ ] Port the prototype's real behaviour into React: read a sticker by link fragment and by camera, scan and match, open the channel, drive Web Bluetooth, and render a device view (identity, addresses, device version) with no readings yet.
- [ ] The React half drives Web Bluetooth, the camera, and the interface; the wasm half stays the protocol. The boundary does not move.
- [ ] Graphs are drawn as SVG from the sample buffer with no charting dependency — but the buffer and the readings are D1, so E1 only leaves room for them.

Design sourcing for the view: the shipped prototype `www/index.html` styles are the visual baseline; cross-check `.workhorse/design/`.

## Serving and CI

- [ ] Repoint the stale `bliti-www` systemd user unit at this repository, and stand up an HTTPS proxy in front of it (`tailscale serve`), because the phone needs a secure origin for Bluetooth and the camera. Local serving is for development against a phone.
- [ ] CI builds the static bundle (wasm + Vite) and keeps it as an artefact, so production hosting at `https://bliti.tamanu.app/` is later wiring rather than work. Standing up that origin is **not** this card.

## Playwright harness

- [ ] A Playwright harness that fakes at the message layer: decoded messages fed directly to the app, no wasm and no BLE in the loop. It covers the view, the tile/tap behaviour (D1), and the envelope lifecycle (subscribe/unsubscribe around page visibility, skip-unknowns, version display).
- [ ] Nothing tries to fake Web Bluetooth. Protocol and transport coverage stays in Rust tests. Bluetooth is exercised manually against real hardware.

## Testing split

- Rust owns protocol and device: framing, handshake, streams, message shapes, skip-unknowns at the parse layer.
- Playwright owns the view and the envelope lifecycle at the message layer.
- Manual / agentic against `tamanu-iti-v4-prototype` owns the end-to-end Bluetooth path.
