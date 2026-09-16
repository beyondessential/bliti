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
- [ ] Remove `DeviceMessage::Unknown` and `parse_client_message`'s error-reply behaviour, replaced by the skip rule below.

Keep everything protocol-shaped: the key schedule, advertisement matching, the handshake, the streams, the framing. The wasm crate keeps its exported surface for those; only the demonstration message goes.

## Wire-contract envelope (BLI-MSG)

The envelope is the part that outlives the card. Specs: `.workhorse/specs/messages.md`.

- [ ] **Naming and version.** The device includes its own software version in its on-connect identity message; the client displays it. The client sends its own name and version to the device, which logs it. Neither end branches on what it was told.
- [ ] **Skip unknowns, both directions.** Unknown message types and unknown fields are ignored and the channel continues — device ignoring what a newer client sends, client ignoring what a newer device sends. In Rust, this means an unknown JSON `type` deserialises to a skipped/ignored variant rather than an error, and structs tolerate unknown fields (no `deny_unknown_fields`). No error is sent back and nothing closes the channel.
- [ ] **No version gate.** Confirm nothing anywhere refuses to proceed on the other end's version. The sticker-version check in `Sticker::new`/`read_local_name` is a *sticker payload* version, not a protocol version, and stays.
- [ ] **Subscribe / unsubscribe.** Add client→device subscribe and unsubscribe messages. Static identity is pushed once on connect (as today); live data flows only while subscribed. E1 carries no live data itself, so the subscribe path can be exercised by the harness and by D1's readings; wire the messages and the lifecycle, not a specific stream of readings.
- [ ] **Page visibility.** The client drops its subscription on the browser's page-hidden signal and restores it on page-shown. This lives in the React app.

Version-numbering source: decide what "the client's version" and "the device's version" are (crate version, build stamp) and thread both through. Note the choice here once made.

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
