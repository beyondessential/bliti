# Rebuild bliti-web as a real client: test cases

Coverage this card owes. The harness is E1's; the envelope and lifecycle scenarios below are E1's. D1's readings, tiles, and graphs are covered on D1.

Three levels own different things: Rust owns protocol and parse behaviour, Playwright owns the view and the envelope lifecycle fed decoded messages, and the Bluetooth end-to-end path is manual against real hardware.

## Wire-contract envelope (verifies spec: BLI-MSG)

Each case names the level that owns it. An independently written client should pass the same cases from the spec alone.

### Skipping what is not recognised

- [ ] A message whose `type` the receiver does not recognise is skipped, nothing is sent in reply, and the stream stays open (Rust, both directions).
- [ ] A member the receiver does not recognise inside a message whose type it does is skipped, and the rest of the message is read (Rust, both directions).
- [ ] A client skips an unrecognised message type and unrecognised members from the device, and the view carries on (Playwright).
- [ ] Skipping never closes the stream or the connection, in any of the above.

### Reporting what is broken

A peer that breaks the base protocol is a fault, not a version difference, and is reported rather than skipped.

- [ ] Bytes that are not valid UTF-8, not valid JSON, or JSON that is not an object are reported and close the stream they arrived on (Rust).
- [ ] An object with no `type`, or whose `type` is not a string, is reported and closes the stream (Rust).
- [ ] A recognised type missing a required member, or carrying one as the wrong JSON type, is reported and closes the stream (Rust).
- [ ] A message beyond one mebibyte is reported and closes the stream, without the receiver having buffered it (Rust).
- [ ] None of the above closes the connection or disturbs another stream: the reporting stream keeps delivering (Rust).
- [ ] The client surfaces a protocol fault to the operator rather than showing a blank or frozen view (Playwright).
- [ ] The device logs a protocol fault (Rust / manual: check the device log).

### Delimiting

- [ ] A message is read back byte-identical when its four-byte big-endian prefix is split across stream reads (Rust).
- [ ] Several messages in one read are separated correctly (Rust).

### Naming and version skew

- [ ] `device-hello` is the first message on the device's reporting stream; `client-hello` is the first on the client's control stream (Rust).
- [ ] Each end sends its hello without waiting for the other's, and either arrival order works (Rust).
- [ ] The device's `name` and `version` are shown in the view (Playwright).
- [ ] The device records the client's `name` and `version` (Rust / manual: check the device log).
- [ ] No version is compared above the base protocol version: a client older than the device, and a device older than the client, both reach an open channel and a device view.
- [ ] A device advertising a base protocol version the client does not implement is reported as exactly that, before any handshake is attempted (Rust).
- [ ] A `name` or `version` of an unexpected shape changes nothing: both are opaque and neither end parses them.

### Subscription streams

- [ ] Static data is pushed on the reporting stream without being asked; live data arrives only on a subscription stream.
- [ ] Opening a stream with `subscribe` yields that topic's data on that same stream (Rust, with a test topic).
- [ ] Closing the stream ends the subscription and the device sends nothing further (Rust).
- [ ] The device acts on end of stream rather than waiting for its own side to close: after the client half-closes, the device stops sending and closes its side (Rust). This is the case a half-close silently breaks.
- [ ] A stream reset, rather than a graceful close, ends the subscription the same way (Rust).
- [ ] A client that goes away without closing, and a dropped connection, each end subscriptions without either end timing anything out (Rust).
- [ ] Data in flight when the stream ends is discarded by the client rather than treated as a fault (Playwright).
- [ ] A `subscribe` for a topic the device does not recognise is skipped: the stream stays open, carries nothing, and nothing errors (Rust).
- [ ] Two subscriptions are independent: closing one leaves the other delivering (Rust).
- [ ] Hiding the page closes the client's subscription streams; showing it opens fresh ones (Playwright, page-visibility).

## The prototype is gone

- [ ] No send-text affordance in the client, and no device-side print of client text.
- [ ] The `www/index.html` and `www/app.js` prototype no longer exist; the app is the built bundle.

## Installable and offline (verifies spec: BLI-WEB)

- [ ] The application runs from its hosted origin with no installation.
- [ ] After the application has been loaded once, it works offline: it loads and reaches the device-finding UI with no connectivity.

## End-to-end on real hardware (manual, against tamanu-iti-v4-prototype)

- [ ] A sticker followed by its link opens the app with the payload in the fragment, finds the device, runs the handshake, opens the channel, and shows a device view with no readings.
- [ ] A sticker captured by the in-app camera reaches the same open channel and device view.
- [ ] The device view shows identity and addresses and the device's version, and updates when the addresses change.

## Build and CI

- [ ] CI builds the static bundle (wasm + app) and keeps it as an artefact.
- [ ] Local serving reaches a phone over an HTTPS origin, so Bluetooth and the camera are available.
