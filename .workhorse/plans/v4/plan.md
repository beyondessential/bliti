# Print QR labels for provisioned Itis

Adds the `label` step (PLBL) and the client's label list and A4 sheet (PSHT).
The device only hands over the payload, the version and a time; the PDF is drawn in the browser.

## Notes

- **Handing the payload to the steps.**
  `name::establish` derives the whole `QrPayload` and throws all but the suffix away, and the registry gets a `watch::Receiver<Option<Name>>`.
  Replace the watch's content with a small `Established { name: Name, payload: Option<QrPayload> }` in `provision/name.rs`, so `encryption`'s `Named` and `tailscale`'s `Known` read `.name.suffix` and the new step reads `.payload`.
  Two parallel channels would say the same thing twice.
  `payload` is `None` where the suffix is `FAULT`.
- **Version.**
  `eeprom/hat.rs::describe` already turns EEPROM bytes into `Held::Iti(Version)` and never returns `Image`, so the step reads the chip's head through `eeprom::Memory`, calls `describe`, and takes a `Held::Iti` version; otherwise `hardware::place::place`.
  Expose that as one `pub(super)` function in `eeprom.rs` (`held_version(memory)`) rather than reaching into `hat`.
- **PDF in Rust, in `bliti-web`.**
  One pure function, labels in and PDF bytes out, with the layout constants beside it, so `cargo test` covers the grid and the fill wording and the browser only downloads bytes.
  A small writer crate such as `pdf-writer` fits (vector rectangles for modules, the standard Helvetica and Courier fonts, no embedding, since every character is ASCII); pick it with `cargo add` and check it builds for `wasm32-unknown-unknown`.
  Ask the user before taking a bigger dependency.
- **QR modules.**
  The sheet code reads each payload text with `QrPayload::read` and takes `to_qr_code()`, so a payload that is not one is refused and the code is the one QR specifies (level H, version 5).
  Modules are drawn as filled squares across the 45-module area including the quiet zone, scaled to 30 mm.
- **Client storage** follows `provision/tailscale.js`: local storage, every access guarded, unreadable counts as none.
  Unlike the key, writes are read-modify-write so another tab's additions are never lost, and the list listens for the `storage` event.
- **Spec comments** `// spec: PLBL` and `// spec: PSHT` go on the code that implements them, as the other steps do.

## Device

- [ ] `provision/name.rs`: add `Established { name, payload }`; `establish` returns it, keeping `named` for the suffix
- [ ] `provision/daemon.rs`: publish `Established` on the watch; advertise `established.name.render()`
- [ ] `provision/steps.rs`, `steps/encryption`, `steps/tailscale`: read the suffix from `.name.suffix` (`Known` and `Named` change type only)
- [ ] `steps/eeprom.rs`: add `held_version(memory) -> Result<Option<Version>, String>` over `describe`
- [ ] `steps/label.rs` (new): the `Label` step, `id` `label`, title "Make the label", `done()` always `NotDone` with no reason; `run()` waits for the established payload, fails with the device's cause where there is none, takes the version from the EEPROM then from `place`, fails "could not be placed and needs looking at" where neither gives one, and sets the result `{payload, version, at}` with `at` in RFC 3339 UTC
- [ ] `steps.rs`: add `label` last in `all`, after `bootloader`, and `pub mod label`
- [ ] Check the fake registries (`provision/fake.rs`, `fake_system.rs`) and the console's result printing show the new step sensibly
- [ ] `steps/label/tests.rs`: version from an EEPROM holding an Iti beats placement; blank EEPROM falls back to placement; neither fails; no payload fails and carries none; result members and an RFC 3339 `at`; `done()` is `NotDone`; a second run gives a later `at`

## Sheet, in bliti-web (Rust)

- [ ] Add the PDF crate to `crates/bliti-web/Cargo.toml` with `cargo add`
- [ ] `bliti-web/src/sheet.rs` (new): `POSITIONS` = 27, `fill(count) -> String` (the three wordings of PSHT, "15% filled", "3 sheets, last 70% filled", "2 sheets"), and `pdf(labels) -> Result<Vec<u8>, ...>` placing 50 x 30 mm cells in a centred 3 x 9 grid, row by row, adding pages, cut lines only around labels present, the code filling the height, suffix large and "Tamanu Iti v" + version small
- [ ] `bliti-web/src/lib.rs`: export `sheet_pdf(labels)` and `sheet_fill(count)` through wasm-bindgen
- [ ] Tests in `sheet.rs`: page count at 1, 27, 28 and 55 labels; cell origins for the first, tenth and twenty-eighth; no cut line for an empty position; fill wording at 1, 4, 27, 28, 54, 55; a payload that is not one is refused; no passphrase can reach it (the input type has none)
- [ ] `just build-wasm`, then check the bundle grows by a small amount only

## Client

- [ ] `provision/labels.js` (new): the list in local storage, add / replace by payload / mark printed / remove, latest `at` per suffix, a week's memory of removed `at`s without their payload, cross-tab safe writes, memory fallback with a flag for "lost when closed"
- [ ] `provision/useLabels.js` (new): the list as React state, refreshed on the `storage` event
- [ ] `provision/state.js`: `labelOf(steps)` reading `{payload, version, at}` from the `label` step's result when it has `passed`
- [ ] `provision/useProvision.js`: on each `provision-state`, offer the label to the list (once per `at`)
- [ ] `provision/Labels.jsx` (new): title "Labels", Back, ticked unprinted labels, per-label Added/Printed line, Remove (confirm where unprinted), the download as primary action with the fill wording, not offered while none is ticked; mark ticked labels printed on download; Blob download named by date
- [ ] `provision/Provision.jsx`: a `labels` view reachable while finding a device and when finished; it is the page's own state, not an act
- [ ] `provision/Finding.jsx`: "Labels (N not printed)" link beneath, where the list holds a label
- [ ] `provision/Done.jsx`: say the label was added, show the sheet download as the primary action while one is unprinted, the Labels link, and never show the payload
- [ ] `styles.css`: the list rows, with the checkbox `width: auto; padding: 0` as `.pick` does (the global `input { width: 100% }` stretches it otherwise)
- [ ] Say that labels are lost when the page is closed where storage cannot be used

## Tests

- [ ] `tests/fake-provision-client.js`: nothing to add if `emitState` already carries a result; otherwise let a test set one on a step
- [ ] `tests/provision-labels.spec.js` (new): a finished run adds a label; seeing the same result again adds nothing, also after removal; a later `at` replaces and unprints; the list shows across two pages sharing storage; unprinted ticked and printed not; download is a PDF, marks the ticked labels printed, and the button reads with the fill wording; Remove confirms an unprinted label; the payload text never appears on the finished screen
- [ ] Extend the finished-screen and finding-screen tests for the new link and primary action
- [ ] Check `just` lint and format targets, and `cargo fmt`, before finishing

## Wrap-up

- [ ] Draft the test-cases checklist from PLBL and PSHT
- [ ] Read the finished specs against the code once for drift
