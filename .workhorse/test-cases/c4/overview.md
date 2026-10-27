# Report the Iti hardware generation

## Device

- [ ] HAT EEPROM identifying a Tamanu Iti of generation 4 reports `product` "Tamanu Iti v4" (verifies spec: NFO)
- [ ] An identifying HAT EEPROM wins whatever else is fitted, TPM or not (verifies spec: NFO)
- [ ] Backup supply and TPM, no HAT EEPROM: `product` is `broken` with a reason naming the missing EEPROM (verifies spec: NFO)
- [ ] Backup supply and TPM, with a HAT EEPROM that does not identify a Tamanu Iti: `product` is `broken` (verifies spec: NFO)
- [ ] Backup supply, no TPM, no HAT EEPROM: `product` is "Tamanu Iti v3" (verifies spec: NFO)
- [ ] Backup supply, no TPM, a HAT EEPROM that does not identify a Tamanu Iti: no `product` (verifies spec: NFO)
- [ ] No backup supply: no `product`, TPM or not (verifies spec: NFO)
- [ ] On the v4 prototype with its EEPROM unprogrammed, `product` is `broken`
- [ ] On the v3 test device, `product` is "Tamanu Iti v3"

## Client

- [ ] Status identity line reads "Tamanu Iti v4 · Ubuntu 26.04" where `product` has a value (verifies spec: STAT)
- [ ] Status identity line falls back to the board's short form where `product` is absent or broken (verifies spec: STAT)
- [ ] Device view header shows `product`, `board` with its revision, `os` and `kernel` by their values, none shortened (verifies spec: VIEW)
- [ ] Device view header shows a broken `product`'s reason in its place (verifies spec: VIEW)
- [ ] `product` gets no tile of its own (verifies spec: VIEW)
