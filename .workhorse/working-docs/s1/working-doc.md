---
status: draft
---

# Encode the QR payload for alphanumeric mode

Work out whether the sticker QR code can use QR alphanumeric mode (upper-case URL, Base45, or a bare `BLITI:` prefix) to stay coarse enough to scan as the payload grows from 33 to 65 bytes.

## Behaviour

The 65-byte payload (version marker, presence token, device static public key) has already landed in QR.
The fragment is 104 base32 characters and the code is generated at level H.
What this card decides is the text wrapped around that payload, and possibly its encoding.

## Implementation options

Sizes over a random 65-byte payload, as version and module count (the side of the symbol without quiet zone).
At a fixed print size, fewer modules means larger modules.
Two optimisers: libqrencode (via `qrencode`) and the `qrcode` 0.14.1 crate the generator uses.

| option | text | H libqrencode | H crate | Q crate | M crate |
| --- | --- | --- | --- | --- | --- |
| today | `https://bliti.tamanu.app/#` + base32 | v10, 57 | v10, 57 | v8, 49 | v7, 45 |
| no trailing slash | `https://bliti.tamanu.app#` + base32 | v9, 53 | v10, 57 | v8, 49 | v6, 41 |
| upper-case URL | `HTTPS://BLITI.TAMANU.APP/#` + base32 | v9, 53 | v10, 57 | v8, 49 | v6, 41 |
| bare prefix | `BLITI:` + base32 | v8, 49 | v8, 49 | v7, 45 | v5, 37 |
| bare prefix, Base45 | `BLITI:` + Base45 | v8, 49 | | | |

The crate's automatic segmentation misses v9 at level H for both URL variants, which sit within a few bits of the boundary.
The crate can build a code from explicit segments (`Bits` with `push_byte_data` and `push_alphanumeric_data`, then `QrCode::with_bits`), so the generator can segment by hand and reach what libqrencode reaches.
Hand segmentation also makes the version a property of the generator rather than of an optimiser's heuristics.

Bit arithmetic behind the table, at level H (v8 holds 688 data bits, v9 800, v10 976):

- Today: 26 byte-mode characters (≈220 bits with header) plus 104 alphanumeric (≈585) is ≈805, just over v9.
- Upper-case URL: 25 alphanumeric (≈151), `#` as its own byte segment (20), then the fragment (≈585) is ≈756, inside v9.
- Dropping only the trailing `/` from the lower-case URL (`https://bliti.tamanu.app#…`, which a browser resolves to the same page) saves 8 bits, ≈797, just inside v9. It keeps the URL lower case, so it costs link claiming nothing.
- Bare prefix with base32: ≈618 bits. With Base45 (98 characters): ≈585. Both v8; v7 (528) is out of reach either way.

Base45 in the URL design is not viable: its alphabet includes space and `%`, which a URL fragment cannot carry literally, and percent-encoding them costs more than Base45 saves.
Base45 also makes a poor human-readable rendering (symbols, `0`/`O`), so adopting it would mean the rendering and the code carry different characters.

## Open questions

- [ ] Does dropping the trailing slash survive every path a scan takes: generic camera apps on Android and iOS, the web app's own reader, and a future App Link or Universal Link (whose path matching may treat an empty path differently from `/`)?
- [ ] What result from the print trial would push the design from the URL to the bare prefix?
- [ ] What physical size is the code printed at, and on what (label stock, enclosure material)? Needed for the print-and-scan trial.

## Trade-offs

- A native application claiming links is possible in future but not planned, so link claiming is kept possible where it is free and not treated as a hard constraint.
- An upper-case URL never wins a version over the lower-case URL without its trailing slash, at any of H, Q or M. Dropping the slash gets the same code with the URL still lower case, which makes the App Links and Universal Links case question moot for sizing. The lower-case rule in QR can stay.
- A generic phone camera opening the web app is nice to have. The URL stays unless the print trial shows its version reads badly off an enclosure, in which case the bare prefix wins.
- Error correction level is open down to M, to be settled by the print trial rather than fixed at H up front.

## Testing notes

- Print each candidate at the real sticker size and scan with a spread of phone cameras (generic camera app and the web app's own reader), including a scuffed or partly covered code.
