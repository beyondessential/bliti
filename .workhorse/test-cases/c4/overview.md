# Report the Iti hardware generation

## Device

- [x] HAT EEPROM identifying a Tamanu Iti of generation 4 reports `product` "Tamanu Iti v4" (verifies spec: NFO)
- [x] An identifying HAT EEPROM wins whatever else is fitted, TPM or not (verifies spec: NFO)
- [x] Backup supply and TPM, no HAT EEPROM: `product` is `broken` with a reason naming the missing EEPROM (verifies spec: NFO)
- [x] Backup supply and TPM, with a HAT EEPROM that does not identify a Tamanu Iti: `product` is `broken` (verifies spec: NFO)
- [x] Backup supply, no TPM, no HAT EEPROM: `product` is "Tamanu Iti v3" (verifies spec: NFO)
- [x] Backup supply, no TPM, a HAT EEPROM that does not identify a Tamanu Iti: no `product` (verifies spec: NFO)
- [x] No backup supply: no `product`, TPM or not (verifies spec: NFO)
- [x] On the v4 prototype with its EEPROM unprogrammed, `product` is `broken`
- [ ] On the v3 test device, `product` is "Tamanu Iti v3"
- [ ] A published HAT EEPROM carrying the Tamanu Iti identifiers reads as its generation, from the real firmware node (needs the contents #F4 defines)
- [x] A backup board is fitted from the supply's first look that finds its gauge, answering or not, and not before (verifies spec: NFO)

## Client

- [x] Status identity line reads "Tamanu Iti v4 · Ubuntu 26.04" where `product` has a value (verifies spec: STAT)
- [x] Status identity line falls back to the board's short form where `product` is absent or broken (verifies spec: STAT)
- [x] Device view header shows `product`, `board` with its revision, `os` and `kernel` by their values, none shortened (verifies spec: VIEW)
- [x] Device view header shows a broken `product`'s reason in its place (verifies spec: VIEW)
- [x] `product` gets no tile of its own (verifies spec: VIEW)
