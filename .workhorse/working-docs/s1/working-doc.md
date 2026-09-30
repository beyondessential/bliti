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

### Physical size

The dev prints are v10 at 40 mm side, so a module is 0.70 mm.
That is taken as the smallest module a phone reads reliably at the expected distance, so the version sets the size.

| version | modules | code side | with 4-module quiet zone |
| --- | --- | --- | --- |
| v10 | 57 | 40.0 mm | 45.6 mm |
| v9 | 53 | 37.2 mm | 42.8 mm |
| v8 | 49 | 34.4 mm | 40.0 mm |
| v7 | 45 | 31.6 mm | 37.2 mm |
| v6 | 41 | 28.8 mm | 34.4 mm |
| v5 | 37 | 26.0 mm | 31.6 mm |
| v4 | 33 | 23.2 mm | 28.8 mm |

The case is 90 × 90 × 35 mm.
A 40 mm code only fits the top or bottom; the goal is a code of 30 mm or less (25 mm ideally) that fits a 35 mm side face.

- 30 mm needs v6 or smaller: a URL at level M (v6) or the bare prefix at M (v5). Nothing at H or Q gets there.
- 25 mm needs v4 (the bare prefix at L), or v5 with modules shrunk to 0.68 mm (the bare prefix at M, or a URL at L).
- On a 35 mm face, a printed quiet zone leaves v6 at 34.4 mm, with no margin, and v5 at 31.6 mm. A light, plain case surface could serve as the quiet zone instead.

Print medium: paper first. A plastic pouch or similar for dust and water is being looked at. A glossy cover adds glare, which eats into the error correction margin just as level M or L shrinks it.

### Segmentation is content-dependent in the crate

Across two random payloads the crate put today's URL at M at v7 once and v6 once, although the character classes are identical.
Its optimiser appears to split out runs of base32's digits `2`–`7` differently depending on the payload.
Every board's code would then be not only larger than it needs to be but a different size from board to board, which a fixed sticker layout cannot absorb.
Hand segmentation fixes the version for every board.

### Shrinking the payload

The payload is ≈585 of the ≈618 bits in the bare-prefix code, so it dominates everything above.
Two cuts, independent of each other, each taking 16 bytes off:

- **Short token.** Carry a 16-byte presence token and expand it to the 32-byte Noise PSK on both ends. Payload 49 bytes.
- **Key fingerprint.** Carry a 16-byte fingerprint of the device static public key rather than the key, and switch the handshake to a pattern that sends the device static in the handshake. Payload 49 bytes.
- **Both.** Payload 33 bytes, the size before the device authentication redesign, with the device authentication property kept.

Versions by libqrencode (the crate, segmented by hand, should match), as URL without trailing slash / bare prefix, and the bare prefix's side at 0.70 mm:

| payload | chars | H | Q | M | L |
| --- | --- | --- | --- | --- | --- |
| 65 bytes | 104 | v9 / v8, 34.4 mm | v8 / v7, 31.6 mm | v6 / v5, 26.0 mm | v5 / v4, 23.2 mm |
| 49 bytes | 79 | v8 / v7, 31.6 mm | v7 / v5, 26.0 mm | v5 / v4, 23.2 mm | v5 / v4, 23.2 mm |
| 33 bytes | 53 | v7 / v5, 26.0 mm | v6 / v4, 23.2 mm | v5 / v3, 20.3 mm | v4 / v3, 20.3 mm |

At 33 bytes, the bare prefix gets to 26 mm at level H, and a URL to 28.8 mm at level Q.
The payload cut buys back the error correction that the encoding alone would have to spend.

#### Short token: cost

- KEY: the presence token becomes 16 bytes of the root's derivation. The advertised handle's keyed hash takes a 16-byte key without change.
- CHN: the PSK becomes a derivation of the token rather than the token itself, overturning "used exactly as KEY produces it with no further derivation".
- SEC: 128 bits against guessing. The token is never sent, the handle is a keyed hash of it, and the handshake cannot be tested offline without a DH secret, so no path offers an offline search at that size.

#### Key fingerprint: cost

- CHN: `NKpsk0` needs the responder static before the handshake. `NXpsk0` sends it encrypted in the second message, and the client checks it against the fingerprint before accepting the session. The second message grows by 48 bytes.
- The `NKpsk0` note that the client's first message is encrypted to the device static key goes away. The handshake payloads are empty (`noise.rs` writes `&[]`), so that confidentiality protects nothing today.
- SEC: impersonating a device takes a keypair whose fingerprint matches, a 128-bit second preimage. Searching board IDs against the fingerprint still costs one argon2id derivation per guess, as searching against the key does.
- KEY: the fingerprint is a hash of the device static public key, under a context string of its own.

## Open questions

- [ ] Does dropping the trailing slash survive every path a scan takes: generic camera apps on Android and iOS, the web app's own reader, and a future App Link or Universal Link (whose path matching may treat an empty path differently from `/`)?
- [ ] What result from the print trial would push the design from the URL to the bare prefix?
- [ ] Is the case surface light and plain enough to serve as the quiet zone on a side face?
- [ ] Which payload cuts to take: short token, key fingerprint, both, or neither?

## Trade-offs

- A native application claiming links is possible in future but not planned, so link claiming is kept possible where it is free and not treated as a hard constraint.
- An upper-case URL never wins a version over the lower-case URL without its trailing slash, at any of H, Q or M. Dropping the slash gets the same code with the URL still lower case, which makes the App Links and Universal Links case question moot for sizing. The lower-case rule in QR can stay.
- A generic phone camera opening the web app is nice to have. The URL stays unless the print trial shows its version reads badly off an enclosure, in which case the bare prefix wins.
- Shrinking the payload is priced here, with the encoding, not treated as fixed.
- Error correction level is open down to M, to be settled by the print trial rather than fixed at H up front.

## Testing notes

- First print-and-scan run, at 0.70 mm modules:
  - today's v10 H (40 mm) and the no-slash URL at v9 H (37.2 mm), as baselines
  - no-slash URL at M, v6 (28.8 mm)
  - bare prefix at M, v5 (26.0 mm)
  - bare prefix at L, v4 (23.2 mm)
- Each candidate on paper, and in the protective pouch once one is chosen, since glare costs error correction.
- Print each candidate at the real sticker size and scan with a spread of phone cameras (generic camera app and the web app's own reader), including a scuffed or partly covered code.
