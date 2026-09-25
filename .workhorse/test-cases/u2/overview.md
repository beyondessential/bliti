# Show only the scanned device in the browser's chooser

## Advertised name

- [x] The handle is the keyed hash of the presence token over the constant alone, pinned by a known-answer vector (verifies spec: KEY)
- [x] The local name is nine bytes, version then handle, as 15 characters of base32, pinned by a known-answer vector (verifies spec: ADV)
- [x] A name at a version the client does not hold still yields its version (verifies spec: ADV, VER)
- [x] A name from another QR code does not match (verifies spec: ADV)
- [ ] A device advertises the same name across a restart and across sessions opening and ending (verifies spec: ADV)

## Web application

- [x] The name the device advertises is shown before the chooser opens, with the line that it should be the only one listed (verifies spec: WEB)
- [x] The chooser is filtered on that exact name and the bliti service (verifies spec: WEB)
- [x] A chooser closed with nothing picked says a device not listed may be off or out of range, and the Find button stays (verifies spec: WEB)
- [x] Any other failure to connect is reported as itself, not as nothing picked (verifies spec: WEB)
- [x] The expected name the wasm module computes matches what that device advertises (verifies spec: WEB, ADV)

## On hardware

- [ ] With two devices advertising, scanning one device's QR code opens a chooser listing only that device, under the name the page showed (verifies spec: WEB)
- [ ] With the scanned device off, the chooser lists nothing, and closing it shows the conditional message (verifies spec: WEB)
- [ ] The native client's scan reports the matching device and passes over the other (verifies spec: ADV)
