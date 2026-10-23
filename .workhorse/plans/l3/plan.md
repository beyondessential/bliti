# Wired carrier fact

Adds the `wired-carrier` fact of NFO on the device and renders it as a tile in the browser client, per NFO ("Network") and VIEW ("Aggregates and scales").

## Notes

- The device already keeps an idle wired port up with no addressing (`idle` in `network/render/networkd.rs`), so carrier is observable on a port whose candidates are all off or absent. Nothing in the network backend changes.
- Facts are re-read by `facts::facts()` every `FACTS_POLL` in a session and resent by `session.rs` when `same_facts` says they differ. A port that disappears is sent `ended` by the same loop. So the device work is one more source in `facts()`, with no new push path.
- Read carrier from `/sys/class/net/<interface>/carrier`, which agrees with the `LowerUp` flag the rtnl observer sends the selector. The file errors with `EINVAL` on an interface that is administratively down, which is reported `broken` with the error as its reason.
- The set of wired interfaces is `network::wired::interfaces(root)`, which takes the sysfs root as an argument, so tests lay out a fake `/sys/class/net` in a temporary directory.
- `boolean` is a catalogue kind only. Entries are not versioned, so no version number moves (VER).
- A client that does not know `boolean` falls back to the stringified value, so a device ahead of an installed client degrades to "true"/"false".

## Device

- [x] `crates/bliti-core/src/channel/readings.rs`: add `kind::BOOLEAN`, and `Entry::boolean(at, name, bool)` beside `Entry::text`, with a unit test that it carries `kind` `boolean`, a JSON bool value and `status` `passed`
- [x] `crates/bliti/src/facts/network.rs`: add `carrier(at)` yielding one `wired-carrier` entry per wired interface, with the `interface` trait from `interface_trait` (so `route` is carried), `broken` with the read error as the reason where the carrier file cannot be read
- [x] Factor the sysfs root so `carrier` takes it as `carrier_in(root, at, route)`, `carrier(at)` passing `SYS_CLASS_NET`
- [x] `crates/bliti/src/facts.rs`: call `network::carrier(at)` from `facts()`, after `network::addresses`
- [x] Rust tests in `facts/network.rs`, over a fake sysfs root:
  - [x] a wired port with carrier `1` is `true`, one with `0` is `false`, both `passed`
  - [x] a port with no `device` link, a wireless port (`wireless` or `phy80211`) and loopback produce nothing
  - [x] an unreadable carrier file is `broken` with no value and a reason
  - [x] two ports are told apart by `interface` and nothing else, so `identity_key` differs
  - [x] `route` moving between ports does not change either port's `identity_key`
- [x] Rust test in `session.rs` that `same_facts` calls a `wired-carrier` flipping from `false` to `true` a change, and the same value taken at a later `at` no change, so a plug or unplug is sent and a settled port is not resent
- [x] `every_entry_carries_a_status` in `facts.rs` already walks `facts()`, so it covers `wired-carrier` on any machine with a wired port with no further test

## Client

- [x] `crates/bliti-web/app/src/readings.js`: put `wired-carrier` after `network-address` in `TILE_ORDER`, give it the label `Cable` in `LABELS`, and add a `boolean` case to `formatValue` returning `Plugged in` or `No cable` for a `wired-carrier` and `Yes`/`No` for any other boolean name
- [x] `crates/bliti-web/app/src/readings.js`: make `numberOf` return null for `boolean` (it already does by default; add a test so a later change cannot graph one)
- [x] `crates/bliti-web/app/src/Readings.jsx`: a `CarrierTile` rendered from `renderTile`'s `wired-carrier` case, headlining one line per interface in interface-name order, as `AddressTile` does for addresses, with each line naming the interface; the reveal repeats the lines through `Line`
- [x] Check an `ended` `wired-carrier` removes its line and, when it was the last, the tile (existing `isEnded` handling in the entry store should cover this; confirm against the tile grouping)
- [x] Playwright tests in `crates/bliti-web/app/tests/readings.spec.js`, using the `fact` and `emit` helpers:
  - [x] a port with carrier `true` and one with `false` render as separate lines, naming each interface, in the `Cable` tile
  - [x] the tile sits directly after `Address` in the order
  - [x] a later `wired-carrier` for the same interface replaces its line rather than adding one
  - [x] a `wired-carrier` sent `ended` drops that port's line and leaves the other
  - [x] a `passed` face is not coloured, for both values
  - [x] a `broken` `wired-carrier` shows no value, with its reason
  - [x] a `boolean` entry of a name the client does not recognise renders from its name and the stringified value (the generic path, per VIEW)

## Finish

- [x] `cargo fmt`, `cargo clippy` and `cargo test` for the workspace, and the Playwright suite for the client (`just --list` for the recipes)
- [ ] Redeploy to the prototype device and check against the real hardware that unplugging and replugging the cable on a device joined to wifi changes the tile (see the `bliti-prototype` skill)
