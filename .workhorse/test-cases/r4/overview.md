# Check the hardware and program the EEPROM

Scenarios for the steps `hardware` (PHW) and `eeprom` (PEEP).
The automated ones run over fakes of the buses, the lines and the TPM; the cases on a real board are run against F4 once the whole flow is built.

## Hardware check

- [x] A good board passes every check and is placed as 4.0, with the result's checks in order (verifies spec: PHW)
- [x] A USER press seen since the daemon started is not asked for again (verifies spec: PHW)
- [x] Without a press, the `user` notice opens, closes on a press, and the next check follows (verifies spec: PHW)
- [x] A CASE lever left alone fails after five minutes, closes its prompt, and the board is still placed (verifies spec: PHW)
- [x] A USER press that never comes fails that check alone (verifies spec: PHW)
- [x] Contact bounce on CASE is neither a press nor a release (verifies spec: PHW)
- [x] A CASE line that cannot be read fails with a reason (verifies spec: PHW)
- [x] A sensor at the wrong part ID fails naming the ID, and the board is not placed (verifies spec: PHW)
- [x] Every check is carried out whatever failed before it (verifies spec: PHW)
- [x] A board no version matches is not placed, and the step fails saying so (verifies spec: PHW)
- [x] The check's done-state is always not done (verifies spec: RUN)
- [ ] On a real board: USER, CASE, both sensors and the TPM pass, and the board is placed as 4.0
- [ ] On a real board: the sensors do not heat the BME680's gas sensor and nothing else holds I²C3

## EEPROM step

- [x] A blank EEPROM is written without asking once writes stop being refused, with the `id-wp` notice open until then (verifies spec: PEEP)
- [x] A write that reports success and reads back wrong is tried again (verifies spec: PEEP)
- [x] Writes refused for five minutes fail the step with the check-the-bridge reason and close the prompt (verifies spec: PEEP)
- [x] An EEPROM already holding the image passes without writing (verifies spec: PEEP)
- [x] An earlier image of the same Iti version is written without asking (verifies spec: PEEP)
- [x] Another HAT, another Iti version and unrecognised contents ask first, with non-printable characters replaced (verifies spec: PEEP)
- [x] A declined overwrite fails and leaves the chip untouched (verifies spec: PEEP)
- [x] A board that cannot be placed, a medium without the image, and an unreadable EEPROM each fail with their own reason (verifies spec: PEEP)
- [x] The done-state is read off the chip: done, not done with what it holds, and unknown with why (verifies spec: PEEP)
- [x] The HAT parser reads the v4.0 image, rejects a legacy header, a wrong signature and truncated input, and names a blank chip (verifies spec: PEEP)
- [x] The chip is read in chunks and written a page at a time with its write cycle waited out (verifies spec: PEEP)
- [ ] On a real board: the step writes the CAT24C256 over `/dev/i2c-9` while ID WP is bridged, and reads back identical
- [ ] On a real board: after a reboot the firmware publishes the Iti under `/proc/device-tree/hat/`
