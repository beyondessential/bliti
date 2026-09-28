# Country from nearby networks

## What a scan reports

- [x] An access point's Country element gives `country`, upper case, and `environment` as `indoor`, `outdoor` or `any` (verifies spec: CFG)
- [x] A Country element whose third octet names an operating class table gives `country` and no `environment` (verifies spec: CFG)
- [x] No Country element, or one not starting with two ASCII letters, gives neither member (verifies spec: CFG)

## The domain changing with no proposal

- [x] A regulatory change the device did not make is probed, and the session is woken where the capabilities changed (verifies spec: NET, CFG)
- [x] A regulatory change leaving the capabilities as they were wakes nothing (verifies spec: CFG)
- [x] A change arriving while the radios are being probed is probed again
- [x] `state` carries capabilities changed with no proposal, once (verifies spec: CFG)
- [ ] On the prototype device, with the country unset, joining an access point that advertises a country moves the device to that country's domain, and the open session's next `state` carries the wider capabilities (verifies spec: NET, CFG)
- [ ] On the prototype device, leaving that access point returns it to the world domain, and `state` carries the narrower capabilities (verifies spec: NET, CFG)

## Suggesting the country

- [x] The country's Scan scans every adapter (verifies spec: NSCR)
- [x] A device that cannot scan offers no Scan beside the country (verifies spec: NSCR)
- [x] Nothing is suggested before a scan
- [x] Unset, the country most access points name is the primary action, and using it sets the field without proposing (verifies spec: NSCR)
- [x] Every other country heard is offered beside the suggestion (verifies spec: NSCR)
- [x] A tie suggests each tied country as an equal primary action (verifies spec: NSCR)
- [x] Set to another country, the suggestion is a line beneath the field (verifies spec: NSCR)
- [x] Set to a country heard but not suggested, that country is not offered again
- [x] Set to the country suggested, nothing is said of it (verifies spec: NSCR)
- [x] A scan hearing no country the device offers says so (verifies spec: NSCR)
- [x] A scan from a wireless connection suggests the country too (verifies spec: NSCR)
- [x] An access point heard by two radios counts once
- [x] Countries the device does not offer, and codes that are no country, are not counted (verifies spec: NSCR)
- [ ] On the prototype device, scanning from the country section somewhere with real access points suggests the country the operator is standing in
