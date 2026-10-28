# Case switch test cases

Scenarios for the `case-closed` and `case-last-opened` facts, the on-device history of openings, and the case tile in the device view.
Hands-on cases run on the v4 prototype; the switch is pressed and released by hand with the lid off.

## Which boards report it

- [ ] On a board whose HAT EEPROM records a Tamanu Iti v4.0, the first batch on a feed carries `case-closed` (verifies spec: NFO)
- [ ] On a v3, a plain Pi, and a board whose HAT EEPROM records any version other than 4.0, neither `case-closed` nor `case-last-opened` is sent and nothing is written to the history file (verifies spec: NFO)
- [ ] On a v4 whose HAT EEPROM is blank or unreadable (`product` broken), neither case fact is sent (verifies spec: NFO)

## The current state

- [x] With the lid pressing the switch, `case-closed` is `true` and `passed` (verifies spec: NFO)
- [x] With the switch released, `case-closed` is `false` and `warning`, with a reason saying the case is open (verifies spec: NFO)
- [x] Releasing the switch while a client is connected sends the change straight away, without waiting for the next sampling interval (verifies spec: NFO)
- [ ] A simulated bounce (the line toggling several times within a few milliseconds before settling) is reported and recorded as one change, not several (verifies spec: NFO)
- [x] Where the GPIO line cannot be opened, `case-closed` is `broken` with a reason naming the failure (verifies spec: NFO)

## The history file

- [x] Opening and then closing the case appends an `opened` line and then a `closed` line to `/var/lib/bliti/case-history.jsonl`, each with `event`, `time`, `boot.id`, `boot.offset` and `found` as `false` (verifies spec: NFO)
- [ ] `boot.id` matches `/proc/sys/kernel/random/boot_id` and `boot.offset` matches the time since boot when the switch moved (verifies spec: NFO)
- [ ] With no session open for over thirty minutes, so sampling has stopped, opening and closing the case still appends both lines (verifies spec: NFO)
- [ ] Opening the case and cutting power straight afterwards leaves the `opened` line in the file after the next boot (verifies spec: NFO)
- [x] Earlier lines are byte-identical after new events are appended; the file is only ever added to (verifies spec: NFO)

## Found when starting

- [x] Booting with the case open and an empty history appends one `opened` line with `found` as `true`, at the moment bliti started (verifies spec: NFO)
- [x] Booting with the case closed and an empty history appends nothing (verifies spec: NFO)
- [x] With the last line `closed`, powering off, opening the case, and booting appends an `opened` line with `found` as `true` (verifies spec: NFO)
- [x] With the last line `opened`, powering off, closing the case, and booting appends a `closed` line with `found` as `true` (verifies spec: NFO)
- [ ] Stopping bliti, moving the switch, and starting bliti again in the same boot appends a found line carrying the same `boot.id` as the line before it (verifies spec: NFO)
- [x] Starting with the switch matching the last line appends nothing (verifies spec: NFO)

## The latest opening

- [x] With no opening in the history, `case-last-opened` is not sent (verifies spec: NFO)
- [x] After an opening, `case-last-opened` carries the `time` of the latest `opened` line as its value, with its `boot` and `found` as traits, and is `passed` (verifies spec: NFO)
- [x] Opening the case while a client is connected sends `case-closed` as `false` and the new `case-last-opened` in the same batch (verifies spec: NFO)
- [x] Closing the case leaves `case-last-opened` unchanged (verifies spec: NFO)
- [x] With the history file unreadable, `case-last-opened` is `broken` with a reason (verifies spec: NFO)

## From the provisioning medium

- [ ] Opening and closing the case while running from the provisioning medium writes no history file (verifies spec: NFO)

## Device view

- [x] The case tile sits after the channel clients tile and before last boot, and its face says Closed or Open (verifies spec: VIEW)
- [x] With the case open, the tile's face takes the warning colour; closed, it is uncoloured (verifies spec: VIEW)
- [x] Opening the tile shows the latest opening as an elapsed time and as the instant recorded (verifies spec: VIEW)
- [x] Where the latest opening's `found` is `true`, the reveal says it was found when the device started rather than seen (verifies spec: VIEW)
- [x] `case-last-opened` never gets a tile of its own (verifies spec: VIEW)
- [x] With no `case-last-opened` sent, the tile still renders, with nothing about the latest opening in the reveal (verifies spec: VIEW)
- [ ] Opening the case on the prototype while the device view is open flips the tile to Open and updates the latest opening without a reload
- [x] A torn final line in the history is passed over when reading, and the next entry starts on a line of its own (verifies spec: NFO)
- [x] An edge that leaves the switch where it was appends nothing (verifies spec: NFO)
