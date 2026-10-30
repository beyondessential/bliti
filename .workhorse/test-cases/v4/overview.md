# Print QR labels for provisioned Itis

Scenarios for the `label` step (PLBL) and the client's label list and sheet (PSHT).
Device scenarios run in the Rust tests of `steps/label`, sheet scenarios in `bliti-web`'s `sheet.rs`, and the page's in `tests/provision-labels*.spec.js`.

## The step

- [x] The step always reports `not-done` (verifies spec: PLBL)
- [x] The result carries `payload`, `version` and an RFC 3339 UTC `at`, and nothing else (verifies spec: PLBL)
- [x] The payload is the text of the QR code of the board ID the device established at start (verifies spec: PLBL)
- [x] An EEPROM holding a Tamanu Iti gives the version, ahead of the board's hardware (verifies spec: PLBL)
- [x] An EEPROM another HAT wrote, a blank one, or one that cannot be read leaves the hardware to place the board (verifies spec: PLBL)
- [x] A board neither the EEPROM nor the hardware places fails, saying it needs looking at, and carries no payload (verifies spec: PLBL)
- [x] A device that established no board ID fails and carries no payload (verifies spec: PLBL)
- [x] Carried out again, the step gives a later `at` (verifies spec: PLBL)
- [ ] On a provisioned board, the step runs last and a client receives its result after `bootloader` passes (verifies spec: PLBL, RUN)
- [ ] The console shows the payload, version and time as the result of the step (verifies spec: PCON)

## The list

- [x] A finished run adds its label, and the finished screen says so without showing the payload (verifies spec: PSHT, PWEB)
- [x] The same result seen again adds nothing, whether the label is unprinted, printed or removed (verifies spec: PSHT)
- [x] A later result for the same board replaces its label and leaves it unprinted (verifies spec: PSHT)
- [x] A removed label keeps no payload, and its time is remembered for a week (verifies spec: PSHT)
- [x] The list is the same in every tab of the browser, and a tab learns of another's label without a reload (verifies spec: PSHT)
- [x] Where storage cannot keep the list, the page says the labels are lost when it is closed (verifies spec: PSHT)
- [ ] A removed label's time is forgotten a week after its removal, so a later result for that board adds it (verifies spec: PSHT)

## Choosing and downloading

- [x] The list is offered while finding a device and on the finished screen, saying how many are unprinted (verifies spec: PSHT)
- [x] The unprinted labels are ticked, printed ones are not, and any can be ticked (verifies spec: PSHT)
- [x] Nothing is offered to download while no label is ticked (verifies spec: PSHT)
- [x] The sheet is the primary action of the finished screen while a label is unprinted (verifies spec: PSHT)
- [x] The button says how full the sheet is for one page, several pages and full pages (verifies spec: PSHT)
- [x] A download marks the labels on it printed, and a printed label can be downloaded again (verifies spec: PSHT)
- [x] Removing an unprinted label asks first, and a printed one goes at once (verifies spec: PSHT)

## The sheet

- [x] A4 portrait, three across and nine down, filled from the top left, with a new page when one is full (verifies spec: PSHT)
- [x] A cut line around each label present and none around an empty position (verifies spec: PSHT)
- [x] Each label carries the suffix and "Tamanu Iti v" and the version, and nothing of the board's record (verifies spec: PSHT)
- [x] The code fills the label's height with its quiet zone, and a rendered sheet scans back to each payload (verifies spec: PSHT)
- [x] A payload that is not one, or a version that is not one, is refused (verifies spec: PSHT)
- [ ] Printed from a laser printer at actual size on sticker paper, the labels measure 50 by 30 mm and the codes scan with the operator app
- [ ] The sheet downloads with the page offline, once it is ready offline (verifies spec: PSHT)
