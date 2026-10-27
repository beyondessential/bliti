# Simple network screens

## Technical notes

- The simple states and the moves between them (NSET) live in a pure module beside `network.js`: the classifier, which returns a simple state or "outside"; one function per move (join wifi, use the cable, hotspot on, hotspot off, change the hotspot passphrase), each taking the configuration the device holds and returning the document to propose; and the way each row of the Network screen shows. Tested without a page, as `capabilities.spec.js` is.
- The security kind for a scanned network reuses `joinWith` in `capabilities.js`.
- The session is lifted out of `Network.jsx`'s mount into something App owns, so Change can open it, read the first `state`, and either show the busy note or go on to the Network screen. The four simple screens share that one session. NSCR keeps its own.
- Each simple screen pushes a history entry, so the browser's back goes up one level (WEB). Leaving to the Status screen closes the session.
- The hotspot's QR code reuses the protocol module's `hotspot_qr`, as `Readings.jsx` draws it.
- The generated hotspot passphrase is drawn once per session and held with it.
