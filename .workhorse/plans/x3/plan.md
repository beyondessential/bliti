# Tailscale and Tamanu actions on the Status screen

The Status screen's Tailscale and Tamanu rows offer the acts their streams list (STAT), a Tailscale screen under Advanced holds every Tailscale act with the full network check and ping (TSCR), and a sign-in screen signs the device in by auth key or login link.
The device's one change is carrying Tailscale's own netcheck report in the result (TSC).

## Device work

The network check's result carries Tailscale's own report as `report` (TSC), so the Tailscale screen can share everything Tailscale found.
The device already runs `tailscale netcheck --format=json` and parses the members it maps; it keeps the command's output text as it is and sends it as `report` beside them.
The protocol's result schema and its tests change with it, and the shipped client gains nothing to parse: it passes `report` on untouched.

- [ ] `crates/bliti/src/tailscale/netcheck.rs`: `Runner::run` returns the output text with the parsed `Report` (a small struct holding both), and `result` inserts it as `report`
- [ ] Every `Runner` impl follows: `Command`, `Absent` in `tailscale.rs`, the fakes in `tailscale/tests.rs` and `session/tests.rs`
- [ ] Tests in `netcheck/tests.rs` and `tailscale/tests.rs`: `report` is the runner's text byte for byte, and is present in every netcheck result

## Wasm bridge

The messages are all in `bliti-core` already (`Tailscale*`, `Tamanu*`); the browser has no way to open either stream.

- [ ] Move the stream handles out of `crates/bliti-web/src/lib.rs` (817 lines) into `src/streams.rs` before adding two more
- [ ] `Channel::tailscale` and `Channel::tamanu`, each opening an `Exchange` with `Message::Tailscale` / `Message::Tamanu`, as `power` does
- [ ] `TailscaleHandle::act(act, params)`, with `params` a plain object of `method`, `key`, `name`, `peer`, building `Message::TailscaleAct` (the key as a `Secret`); `TamanuHandle::act(act)`
- [ ] A text QR in `bliti-core`: lift the rendering out of `wifi_qr::svg` into one taking any text, which `wifi_qr` then calls; expose it as `text_qr(text)` for the login link
- [ ] Tests in `src/tests.rs` for the act shapes: a sign-in by key carries `KEY`, `NAME` only where given, a ping `PEER` only where given

## Client plumbing

- [ ] `app/src/client.js`: `tailscale()` and `tamanu()` beside `power()`, saying each act in the activity record without the key or the link
- [ ] `app/tests/fake-client.js`: script both streams as the power stream is, with `window.__blitiTailscaleSent` / `__blitiTamanuSent` and a `__blitiTailscaleStream` / `__blitiTamanuStream` to push `*-acts`, `tailscale-login-link`, `tailscale-result` and `*-failed`
- [ ] `app/src/acts.js`: a hook owning one stream for as long as its screen is mounted, holding the listed acts, the act being asked, the confirmation open, the refusal or failure reason, and for Tailscale the latest result and login link; reused by the Status rows, the Tailscale screen and the sign-in screen
- [ ] `app/src/services.js`, pure: the row act tables of STAT by state, the wording of every act, the netcheck verdict (`can connect` where any relay answered, else blocked with captive portal, no internet, or Tailscale unreachable), and the share text (hostname, when it ran, the result as JSON without `report`, then `report` as sent)
- [ ] `app/src/share.js`: `navigator.share` where the browser has it, otherwise copy to the clipboard; used by the netcheck result and the login link

## Status screen

- [ ] `Status.jsx`: the Tailscale and Tamanu rows take their act from `services.js` over what the hook lists, with the confirmation beneath the row in place of the button, the button held off while an act is asked, and the reason in the row's detail until the next ask or leaving
- [ ] Network check on the row: under way shows "Checking…" in place of the button; the verdict joins the detail with a Details link to the Tailscale screen
- [ ] Cancel while waiting for approval: confirmed, with Keep waiting the filled button
- [ ] Connect opens the sign-in screen
- [ ] Advanced gains Tailscale between Network settings and Power and battery, only where the device reports `tailscale-state`
- [ ] `styles.css`: the row's confirmation and busy state, `.facts`, the relays list and the form rows, from the mockup's new rules

## Tailscale and sign-in screens

- [ ] `App.jsx`: screens `tailscale` and `sign-in`, each through `openScreen` so history goes one level per screen; the latest netcheck result held per channel in App, set from either screen, cleared when a channel opens
- [ ] `Tailscale.jsx`: details (state as the row renders it, name, tailnet, sign-in lapse as a date and from now); acts in the order Connect, Turn on, Turn off, Log out, with Turn off and Log out confirmed in TSCR's words; the network check with Share and Run again; ping with an optional machine field
- [ ] `SignIn.jsx`, titled Connect to Tailscale: auth key first, masked with Show, Scan filling the field through `scanner.js`'s `scan` with a `read` taking any text; the device name field filled from `hostname`, sent as `NAME` only where changed; a link to sign in by login link and back
- [ ] Login link: waiting until the first link, then the latest link as a QR from `text_qr`, Open (new tab), Copy and Share, with the line saying it connects once approved; on the stream closing or a failure, the reason and Get a new link
- [ ] Leaving on its own once `tailscale-state` is `running` or `needs-approval`, going back so its history entry goes with it; the key dropped on unmount

## Tests

- [ ] Pure tests for `services.js` without a page, as `capabilities.spec.js` does: each row table, each verdict, the share text carrying `report` untouched
- [ ] `app/tests/status-acts.spec.js`: both streams opened on the Status screen and closed on leaving it; no acts from a device that never answers; Tamanu Start and Restart confirmed, `restarting` from the feed, refusal and failure reasons; each Tailscale state's act; the network check verdicts and Details; Cancel's confirmation
- [ ] `app/tests/tailscale-screen.spec.js`: the Advanced entry only with `tailscale-state`; acts in order and only as listed; Turn off and Log out wording; ping with and without a machine; Share's text
- [ ] `app/tests/sign-in.spec.js`: by key with and without a changed name; by link, the latest link shown, a new link after a lapse or a closed stream; leaving on reaching connected or waiting for approval; Back returning to the Status screen or the Tailscale screen, whichever it was opened from
- [ ] `cargo fmt`, `just clippy`, `just test`, `just test-web`, `just build`
