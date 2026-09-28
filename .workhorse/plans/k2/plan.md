# K2: remember recent devices

## Technical notes

- Storage is `sessionStorage`, which gives the tab lifetime WEB asks for: it survives a reload and a restored tab, and closing the tab clears it. Each launch of the installed app is a fresh session, so starts empty.
- Store the fragment text (the base32 payload) rather than anything derived, so a remembered code goes back through `client.readCode` exactly as a typed or scanned one does. That puts it through the same `read_local_name` → version → `matches` checks in `client.connect` with no second path to keep in step.
- The Recent list gains an entry in `connect()` once `client.connect` resolves, and in `comeBack()` once the device is back (which also moves it to the top). The hostname is updated from the `hostname` fact whenever one arrives while connected.
- "Reload while connected" needs its own marker beside the list: the payload of the device the page is on, set when a channel opens, kept through a going-away, and cleared by `disconnect()`, by `endGoing()`, and by `closed()` when no act is under way. On mount, with no fragment, a marker present means holding that payload.
- Clear the fragment with `history.replaceState` straight after taking it in the mount effect, whether or not it parsed.
- The chooser still needs a user gesture after a reload, because `reconnect`'s `device` handle does not survive. That is why the reload lands on "QR code read" rather than connecting.
