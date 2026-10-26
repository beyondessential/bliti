# Status screen

Build the Status screen of STAT as the landing for a connected device, and the `end-of-batch` marker of NFO it reads its rows by.
The device side is small and goes first, so the client can be built against real batches.

## Notes

- **No fallback for a device without `end-of-batch`.** Nothing has shipped, so the client waits on the marker and nothing else. A device that never sends one leaves the network and Tailscale rows checking, which is what a pre-marker build on the test device will show until it is updated.
- **A resumed feed replaces what the client holds.** The feed is declined while the page is hidden and resumed by subscribing to `default`, and an entry that ended while it was declined is never sent as `ended`. The first batch of every feed carries everything the device holds (NFO), so the client takes it as the whole set: entries not in it are dropped, along with their history. This is the reader's business by NFO, so it needs no spec change.
- **Notices once per message type.** `Refusal::CriticalMembers` carries only the member paths, so the client cannot tell which message type a refusal was about. The refusal gains the message's `type`, and the wasm event carries it, so the client can key notices on it.
- **Screen state.** `screen` gains `status` as the landing, and `readings` becomes a pushed screen like `power` and `network`. Whether Advanced is open is held in App, so it survives going down a level and back, and is reset when a channel opens.

## Device: `end-of-batch`

- [ ] `bliti-core` `channel/messages.rs`: add `Message::EndOfBatch`, written and read as `end-of-batch` with nothing beyond `type`, and add it to the list of known types
- [ ] `channel/generate.rs`: generate `EndOfBatch`; `messages/tests.rs`: round-trip it
- [ ] Add `EndOfBatch` to the exhaustive matches in `bliti/src/session.rs` (a no-op from a client), `bliti/src/network/session/open.rs` and `bliti/src/client.rs` (print it on the diagnostic client's feed)
- [ ] `bliti/src/session.rs` default feed: send `end-of-batch` after the opening facts and current readings, after each sampler batch from `live`, and after each change of facts with the entries it ends
- [ ] Session tests: the first batch is the facts and current readings then the marker; a sampler tick is one batch; a change of facts sends its ended entries and the new ones in one batch
- [ ] `bliti-web/src/tests.rs`: `end-of-batch` decodes to the JSON the app reads

## Device: refusals name their type

- [ ] `channel/envelope.rs`: `Refusal::CriticalMembers` carries the message's `type` alongside the member paths; update `Display` and every construction
- [ ] `bliti-web/src/lib.rs`: the `refused` event carries `type` for both refusals (the critical type's own name for `CriticalType`)

## Client: reading by batch

- [ ] `App.jsx`: buffer `fact` and `reading` messages as they arrive and apply them to `entries` and `history` together on `end-of-batch`, ended entries included
- [ ] `App.jsx`: track whether the current feed's first batch has ended; reset it on connect, on reconnect after an act, and on `resumeFeed`
- [ ] `App.jsx`: on the first batch of a feed, replace the held entries with the batch, dropping history for entries no longer held
- [ ] Leave `end-of-batch` out of the activity log, as readings are

## Client: rows

- [ ] New `status.js` beside `network.js`, pure and tested without a page, holding:
  - [ ] ways in use from `wired-carrier`, `wireless-network`, `hotspot` and `network-address` (non-link-local, per interface)
  - [ ] the network row: connected or not, the cable or wifi interface carrying `route: default` with the cable-then-wifi fallback by interface name, the hotspot with `hotspot-clients` where it has a value, and the not-connected details (cable with no address, wifi joined with no address)
  - [ ] the Tailscale row by STAT's table, with `tailscale-name` as the device name and `tailscale-key-expiry` at `warning` lifting well to warning and adding when it lapses
  - [ ] the Tamanu row by STAT's table, checking while there is no `tamanu-state`, with both versions where they carry a value
  - [ ] an unrecognised state's mark from its `status`, with its value as sent and its `reason`
  - [ ] every row checking until the first batch has ended; Tailscale hidden after that where there is no `tailscale-state`
- [ ] `tests/status.spec.js`: unit cases for each of the above, as `capabilities.spec.js` tests `capabilities.js`

## Client: the screen

- [ ] New `Status.jsx`: hostname title falling back to the code's last four characters (`remembered.js`), the software identity as `Identity` draws it, Disconnect beside the title, notices beneath it, the three rows, and Advanced
- [ ] Row component: mark (well, warning, not well, spinner for in progress), subject name, what it says, `reason` where the table calls for it, detail beneath; Change or Set up on the network row opening `network`
- [ ] Advanced disclosure, closed when a channel opens, leading to All readings, Network settings, and Power and battery
- [ ] `App.jsx` routing: `status` is the landing that `toLanding` and `popstate` return to; All readings, Power and battery, and Network settings each carry Back only; All readings loses its Power, Network and Disconnect buttons
- [ ] `App.jsx`: while a power act is under way, show the Status screen's title, identity and Disconnect, with the act in place of the rows and Advanced
- [ ] `App.jsx` notices: one per message type for as long as the channel is open, cleared when a channel opens or is let go; protocol faults stay as they are
- [ ] `styles.css`: rows, marks and the disclosure, following the mockup and `.workhorse/design/design-system.md`
- [ ] Trace the new code to STAT and NFO in the parenthetical style the client already uses

## Tests

- [ ] `tests/fake-client.js`: a helper that emits a batch followed by `end-of-batch`; `openChannel` ends on the first batch; `openNetworkScreen` goes through Advanced
- [ ] Update `navigation.spec.js`, `power.spec.js`, `network.spec.js` and `remembered.spec.js` for the Status screen landing and Back-only screens
- [ ] New page tests: rows checking until the first batch ends; a batch applied whole (an address moving interfaces never shows not connected); Advanced staying open across screens and closing on a new channel; Change opening Network settings, read-only when busy; a power act replacing the rows; one notice per refused type; a resumed feed dropping entries that ended while it was declined

## Finish

- [ ] Add the network row's checking state to the Status screen mockup
- [ ] `cargo fmt`, `just clippy`, `just test`, `just test-web`
- [ ] Put the build on the prototype device and check the screen against a real device
