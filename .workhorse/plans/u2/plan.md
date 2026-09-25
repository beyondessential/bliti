# Show only the scanned device in the browser's chooser

## Layout and filter

The payload is the version marker then the handle, nine bytes, 15 characters of base32.
The marker first keeps it readable whatever a later version does to the rest, matching the QR payload.
Since the marker is inside any prefix, the chooser filters on the exact name computed at the QR code's version, alongside the service UUID.
A device advertising another version does not appear; VER's report of an unsupported version is a SHOULD, reachable by the native client, which hears every advertisement.

The handle stays under the version marker, so VER's "no two versions produce a matching handle" still holds.

## Naming the device before the chooser opens

The page shows the expected name, ungrouped, before the Find button, in place of the "jumble of letters" copy: a device named that will show up, and it should be the only one in the list.
The wasm bindings expose the expected name on the QR code, beside `human` and `version`, so the page doesn't rebuild it.

A chooser closed with nothing picked (`NotFoundError`) can't be told apart from an empty list, so the page says conditionally that a device not listed may be off or out of range, and the Find button stays.

## Build

- [ ] bliti-core `key_schedule.rs`: handle over the constant alone; drop `RotationSalt` and `ROTATION_SALT_LEN`; new known-answer vector
- [ ] bliti-core `advertisement.rs`: payload is version then handle; `Advertised` loses `salt`; `matches` takes no salt; a constructor for the expected local name from a presence token
- [ ] bliti `device.rs` and `main.rs`: drop `SALT_ROTATION` and the rotation tick; keep the re-advertise on session open and end
- [ ] bliti `client.rs`: match on the fixed handle
- [ ] bliti-web `lib.rs`: expose the expected local name on `QrCode`; `read_local_name` follows the new `Advertised`
- [ ] bliti-web `client.js`: filter on `name` plus the service; keep the version and match checks on the pick; update the comment
- [ ] bliti-web `App.jsx`: show the name before the chooser; the conditional message after a chooser closes with nothing picked
- [ ] Tests for each of the above
