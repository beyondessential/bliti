# Diagnostic and informational data display

This card keeps the feature. The client framework it was going to build along the way is split off as the card below, because it is a rebuild of everything bliti-web is rather than part of any one feature, and because it wants reviewing on its own terms.

The framework card lands first. This card then rebases onto it and lands its readings and its view on top.

## Rebuild bliti-web as a real client

Replace the prototype browser client with the thing every later feature is built on, and settle how a client and a device talk across versions.

The prototype goes entirely: the hand-written page and script, the send-text demonstration message, and the device-side print behind it. In its place, bliti-web becomes a React single-page application built to static files with Vite on npm, installable and working offline once loaded, so a phone that has opened it before is useful at a camp with no connectivity. Everything protocol-shaped stays in Rust compiled to wasm exactly as it is now, and the boundary does not move; only what sits on the browser side of it changes.

This card also carries the part of the wire contract that every future feature inherits whatever its shape: each end names itself and its version, the device logging the client's and the client displaying the device's, with neither ever branching on what it was told. Each end skips message types and fields it does not recognise, in both directions. A client subscribes and unsubscribes rather than being pushed at unconditionally, and drops its subscription when the page is hidden. Nothing anywhere refuses to proceed on the grounds of the other end's version, because a device months behind the application is the normal case and not an error. The format for self-describing readings is deliberately left to the feature card, since it is a display convention and the next features pair display with an action.

When this lands the whole path works on real hardware: a sticker read by link or by camera, the device found, the handshake run, the channel open, and a device view present with no readings on it yet. That is what makes the card reviewable before any feature exists.

Around it: the local serving unit repointed at this repository with an HTTPS proxy in front so a phone can reach it, CI building the static bundle and keeping it as an artefact, and a Playwright harness that fakes at the message layer. Protocol and transport coverage stays in Rust, and nothing tries to fake Web Bluetooth. Production hosting at the sticker's origin is not this card.
