# Report the Iti hardware generation

## Signals

- **HAT EEPROM.** The Pi firmware publishes a programmed HAT EEPROM under `/proc/device-tree/hat/` (`vendor`, `product`, `product_id`, `product_ver`, `uuid`, NUL-terminated). A blank EEPROM publishes nothing, the same as none fitted. v4 carries one, not yet programmed; v3 has none.
- **TPM.** Presence is the device node, as the board-ID TPM backend probes it (`/dev/tpmrm0`, else `/dev/tpm0`). v4 has one on SPI; v3 has none. The shared image always enables the overlay, so an absent node means no working chip.
- **Backup supply.** The X120x gauge acknowledging on I²C bus 1 at 0x36, as `facts::power` already looks for it. Both generations carry one (v3 X1201, v4 X1208), and nothing on the OS side tells those apart.

## The EEPROM placeholder

Which `vendor`/`product` values mean "Tamanu Iti", and which field records the generation, are #F4's to define. This card reads the HAT node through one function with a clearly marked placeholder match, so F4 only fills that in. Until then a v4 prototype reads as "TPM without identifying EEPROM" and reports `product` broken, which is correct for an unprogrammed board.

## Build

- [x] `facts::product` module: read the HAT node, probe the TPM node, take backup-supply presence; decide `product` per NFO §Product; unit-tested for each row and for none holding
- [x] Backup-supply presence without a second I²C owner: `Supply::fitted()` reports what the record thread's last look of the gauge found, so the facts poll never touches the bus
- [x] Add `product` to `facts::facts`, `// spec: NFO`
- [x] Web client: Status identity line takes `product`'s value where it has one, else `board` short, then `os` short (STAT); Device view header adds `product` by value, shows its reason where broken, and keeps every header fact by value (VIEW); `product` is a header fact, so it gets no tile
- [x] Deploy to v4 and check it reports `product` broken (EEPROM unprogrammed)
- [ ] Check v3 reports "Tamanu Iti v3" once the v3 test device is back online
