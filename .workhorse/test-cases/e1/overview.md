# Rebuild bliti-web as a real client — test cases

Coverage this card owes. The harness is E1's; the envelope and lifecycle scenarios below are E1's. D1's readings, tiles, and graphs are covered on D1.

Three levels own different things: Rust owns protocol and parse behaviour, Playwright owns the view and the envelope lifecycle fed decoded messages, and the Bluetooth end-to-end path is manual against real hardware.

## Wire-contract envelope (verifies spec: BLI-MSG)

- [ ] A device ignores a message type a client sends that it does not recognise, and the channel stays open (Rust).
- [ ] A device ignores fields it does not recognise within a message it does, and reads the rest (Rust).
- [ ] A client ignores a message type the device sends that it does not recognise, and the view carries on (Playwright).
- [ ] A client ignores unrecognised fields in a message it does recognise (Playwright).
- [ ] No exchange refuses to proceed on the other end's version: a client older than the device, and a device older than the client, both reach an open channel and a device view.
- [ ] The device's reported software version is shown in the view (Playwright).
- [ ] The client names itself and its version to the device, and the device records it (Rust / manual: check the device log).
- [ ] Static identity is pushed once on connect, unsolicited; live data arrives only after subscribing.
- [ ] Hiding the page drops the subscription; returning to it restores the subscription (Playwright, page-visibility).

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
