# Show only the scanned device in the browser's chooser

## Naming the device before the chooser opens

With the salt gone, the local name a device advertises is a function of its QR code alone: the handle from the presence token, then the version marker.
The page computes it from the QR code it has read and shows it before the operator opens the chooser, in place of the "jumble of letters" copy: a device named that will show up, and it should be the only one in the list.

The page shows all 15 characters at the application's own version, ungrouped, because that is exactly what the chooser displays.
The filter still matches only the first 12, so a device on another version appears under a name that differs in its last three characters, and the check on the pick reports the version mismatch as VER requires.

This belongs in the web app spec's "Finding the device" section when the specs are drafted.
The wasm bindings expose the expected name on the QR code, beside `human` and `version`, so the page doesn't rebuild it.
