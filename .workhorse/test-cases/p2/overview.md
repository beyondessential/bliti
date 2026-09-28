# Hotspot tile shows its QR code

## The device

- [x] The `hotspot` fact carries the running hotspot's passphrase as its `passphrase` trait (verifies spec: NFO)

## The code

- [x] A plain SSID and passphrase encode as `WIFI:T:WPA;S:<ssid>;P:<passphrase>;;` (verifies spec: VIEW)
- [x] `\`, `;`, `,`, `"` and `:` in either are backslash-escaped (verifies spec: VIEW)
- [x] A value that looks like hexadecimal is not quoted (verifies spec: VIEW)
- [x] The drawn code scans back to the URI it encodes (verifies spec: VIEW)
- [x] The code draws in the page's colours, with no fixed white or black
- [ ] A current Android camera joins the hotspot from the code, with a passphrase containing `;`
- [ ] A current iOS camera joins the hotspot from the code, with a passphrase containing `;`
- [ ] The code scans off a phone screen held out at arm's length

## The tile

- [x] Opening the hotspot tile reveals its passphrase and a code captioned "Scan to join" (verifies spec: VIEW)
- [x] The code is behind the tap, not on the face (verifies spec: VIEW)
- [x] A new passphrase redraws the code and leaves one hotspot tile (verifies spec: VIEW)
- [x] A hotspot with no value offers neither passphrase nor code (verifies spec: VIEW)
- [ ] The code shows on a provisional hotspot, marked as trial like the rest of the tile
