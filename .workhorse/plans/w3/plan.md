# Simple network screens

The Network, Cable, Join wifi and Share hotspot screens of NSET, WIFI and SHARE, built in `crates/bliti-web/app` over the configuration session NSCR already speaks.
The work is all client-side: the device side (K3's one-step apply, L3's `wired-carrier`) has landed.

## Technical notes

- The simple states and the moves between them (NSET) live in a pure module beside `network.js`: the classifier, which returns a simple state or "outside"; one function per move (join wifi, use the cable, hotspot on, hotspot off, change the hotspot passphrase), each taking the configuration the device holds and returning the document to propose; and the way each row of the Network screen shows. Tested without a page, as `capabilities.spec.js` is.
- The security kind for a scanned network reuses `joinWith` in `capabilities.js`.
- A configuration carrying no wired candidate is outside the simple states: nothing in it can use the cable, so no move from it could stay simple.
- The session hook closes its stream at once on `close()`, not on the next render, so a session the network settings screen opens straight after is not busy behind it. Coming back to a simple screen from the network settings opens the session again.
- The session is lifted out of `Network.jsx`'s mount into something App owns, so Change can open it, read the first `state`, and either show the busy note or go on to the Network screen. The four simple screens share that one session. NSCR keeps its own.
- Each simple screen pushes a history entry, so the browser's back goes up one level (WEB). Leaving to the Status screen closes the session.
- The hotspot's QR code reuses the protocol module's `hotspot_qr`, as `Readings.jsx` draws it.
- The generated hotspot passphrase is drawn once per session and held with it.
- The session's state reuses `network.js`'s `reduce` as it stands: it already tracks what the device holds, capabilities, `busy`, the outcome and failure, each candidate's state, scan results and refusals. A move's document goes in as `fromDocument(document)` on `proposed`. The simple screens read the reducer and never edit through it.
- `generatePassphrase` in `network.js` already makes the read-aloud passphrase SHARE asks for, and `validate` already holds a passphrase to 8 to 63 characters; export the length check rather than writing another.
- The Network screen's live state comes from `waysInUse` in `status.js`, so "in use" means what it means on the Status screen.
- `.networks` is NSCR's scan list in `styles.css`; the Join wifi list takes its own class so the two don't style each other.
- App's `screen` is `network` for NSCR today. It becomes `settings`, and the simple screens take `network`, `cable`, `wifi` and `hotspot`.

## The pure module

- [x] `src/setup.js`: `simpleState(document)` returning `cable`, `cable-hotspot`, `wifi` or `null` for outside, by NSET's "the cable", "wifi", "on" and "off"
- [x] `src/setup.js`: the moves `joinWifi(document, network)`, `useCable(document)`, `hotspotOn(document, fresh)`, `hotspotOff(document)` and `hotspotPassphrase(document, passphrase, fresh)`, each turning things off by `enabled` false and leaving every other member as it is; `fresh` is the SSID and generated passphrase for a hotspot not yet set up
- [x] `joinWifi` replaces wifi at its index with only the members WIFI's table lists, adds it at the end where there is none, and sets `hidden` only where capabilities offer it
- [x] `src/setup.js`: `networksHeard` reading a scan's security by `joinWith`, and `namedSecurity(capabilities)` taking the first of `psk-sae`, `psk` and `sae` offered
- [x] `src/setup.js`: `waysOf(document, entries)` giving each way's on/off mark and what it shows, per NSET's table
- [x] `src/setup.js`: `joinFailure(failure, index)` sorting a failure into passphrase, out of range (stopped at `carrier`) or other
- [x] `src/network.js`: export the passphrase length check for WIFI and SHARE
- [x] `tests/setup.spec.js`: every move from every simple state lands in the state NSET's table gives and stays simple; the outside cases (two wireless candidates, cable and wifi both on, wifi and hotspot both on, wired candidates partly on, no wired candidate); turned-off members keep their settings; wifi replaced in place, added at the end, `hidden` only where offered; the security kind for each advertised set; failure sorting

## The session

- [x] `src/session.js`: a `useSetupSession` hook App owns, wrapping `client.configure` and `reduce`: `open()` resolving on the first `state` with busy or not, `propose(document)` checked by `validate` and `check` first, `scan()`, `close()`
- [x] App: Change and Set up call `open()`; busy closes it and sets the network row's note, cleared on the next tap or on leaving the Status screen; otherwise open the `network` screen
- [x] `Status.jsx`: the network row shows it is waiting while the session opens, and carries the busy note
- [x] App: close the session on returning to the Status screen by any way back, on disconnect, when an act takes the device away, and before opening NSCR from a simple screen
- [x] App: on a reconnect while a simple screen is open (`channel` changes), reopen with the reducer's `resume`; busy sends the operator to the Status screen with the note
- [x] A session that ends with the channel open: the showing screen says so and offers to open it again

## Navigation

- [x] App: `openScreen` pushes an entry from any screen, and `goBack` goes up one level; `toLanding` goes back as many entries as the operator is deep
- [x] App: the `popstate` guard asks before leaving any screen whose guard is set, not only NSCR's
- [x] Move NSCR's leave confirmation out of `Network.jsx` into a shared component, worded for the screen it is on, and use it from the simple screens
- [x] `Status.jsx`: Advanced's Network settings opens `settings`; rename every `network` reference to NSCR's screen

## The screens

- [x] `src/Setup.jsx`: the Network screen, with the line about phones and laptops, the three ways with their marks and what each shows, and the set-up-under-Advanced state offering only Network settings
- [x] `src/Setup.jsx`: the Cable screen, waiting for `wired-carrier`, proposing `useCable` once a cable is in while the cable is off, the applying spinner, return to Status on applied, the no-connection failure with the device's reason, and the cable-on cases that propose nothing
- [x] `src/JoinWifi.jsx`: the three steps; scan on opening with Refresh and the none-found line; the list by SSID with signal as a word, the joined network marked, networks the Iti can't join shown and not pickable; the cable and hotspot lines
- [x] `src/JoinWifi.jsx`: enterprise sends to NSCR (closing the session first) or back to the list; naming a network the list doesn't show
- [x] `src/JoinWifi.jsx`: the password step with show and hide, the saved passphrase filled in for the network wifi already names, the length check, and Connect proposing `joinWifi`
- [x] `src/JoinWifi.jsx`: connecting with no Cancel; return to Status on applied; wrong password back to step 2 with the field marked; out of range waiting for `default-route` then returning to Status; other failures with the device's reason; choose another network and change the password
- [x] `src/JoinWifi.jsx`: a typed name or password not yet proposed sets the guard and the browser's warning
- [x] Extract the hotspot QR from `Readings.jsx` into a component both it and Share hotspot draw
- [x] `src/ShareHotspot.jsx`: the switch proposing `hotspotOn` or `hotspotOff`, held while applying; joined devices beneath while on; the leave-wifi line while off on wifi; a failure's reason with the switch as the device holds it
- [x] `src/ShareHotspot.jsx`: SSID and password beneath, the password hidden until shown; changing it in place with Save and Cancel, the rejoin line while on, the length check, the guard while unsaved; the QR for what the device holds
- [x] `src/ShareHotspot.jsx`: a hotspot not yet set up shown as the hostname (or the Status screen's fallback title) with a passphrase drawn once per session, proposed on the first switch on or save
- [x] `styles.css`: the ways list with its check and highlight, the switch, the hotspot details and the Join wifi list, ported from the "Master redesign" mockup's CSS

## Tests

- [x] `tests/status-screen.spec.js`: Change and Set up open the Network screen; a busy session keeps the operator on the Status screen with the note, replacing the read-only test
- [x] `tests/network-setup.spec.js`: the Network screen's marks and states from the facts; outside the simple states, only Network settings is offered; the Cable screen waits, proposes once a cable is in and not before, returns to Status, and shows the no-connection failure
- [x] `tests/join-wifi.spec.js`: scan list, joined mark, unjoinable network, enterprise to Network settings, naming a network, the proposal replacing wifi in place, wrong password, out of range then joined, no Cancel, the leave guard
- [x] `tests/share-hotspot.spec.js`: the switch's proposal from each state, the leave-wifi line, password show, change, cancel and length, the QR, a hotspot not yet set up, the switch held while applying, a failure's reason
- [x] `tests/navigation.spec.js`: back from Join wifi returns to Network and from Network to Status, closing the session; back with edits not applied asks
- [x] `just test-web` and `just build` pass

## Checking on the device

- [ ] On the prototype: wifi to cable by plugging one in, cable and hotspot together, joining wifi from the hotspot, a wrong password, and the busy note from a second phone
