---
status: draft
---

# Shrink the QR code to fit the side of the case

Get the device QR code to 30 mm or less at a module size a phone still reads, through its payload, its encoding, and the text around it.

## Behaviour

QR currently carries a 65-byte payload (version marker, 32-byte presence token, 32-byte device static public key) as 104 base32 characters at level H, which comes out at v10, 40 mm.
The aim is a code of 30 mm or less, ideally 25 mm, that fits a 35 mm side face of the 90 × 90 × 35 mm case.

Settled so far:

- The payload is 33 bytes: the version marker, a 16-byte presence token, and a 16-byte fingerprint of the device static public key.
- The client authenticates the device by checking the static key it receives in the handshake against the fingerprint, rather than by knowing the key beforehand.
- A device whose static key does not match the fingerprint fails the handshake, and the operator sees it as any failed handshake.
- The payload stays base32. Base45 buys no version at any size priced, cannot ride in a URL fragment, and makes a poor human-readable rendering.
- The generator segments the code explicitly, so every board's code is the same version and that version is the smallest the text allows.
- The code carries no link. Its text is `BLITI:` followed by the 53 base32 characters of the payload, all in QR's alphanumeric set.
- Only the application's own camera, or the human-readable rendering typed in, reads a code. The follow-the-link path, and the fragment it delivers, go from QR and WEB.
- What a sticker looks like is a product decision outside bliti. Bliti provides the QR code, as SVG with its quiet zone, and the human-readable rendering alongside it for whoever lays the sticker out to use or not. QR's rule that the rendering is printed alongside the code goes.
- The rendering is the payload's 53 characters grouped for legibility, without the prefix.
- The reader takes the code text and anything a person would type for it: the prefix in any case or absent, the payload in any case, dashes and spaces ignored. One reading covers the code and the rendering.
- With no link, the application's camera is the only way a code is read, so it has to read a code at least as well as the phone's own camera app does: from the same distance, at the same size.
- The key schedule expands the 16-byte token twice: the pre-shared key is a BLAKE3 derivation of it under `bliti pre-shared key`, and the advertised handle is keyed on a derivation under `bliti advertised handle`. The handle key and the Noise key are never the same bytes.

With no link, only someone who knows what the code is for, and has the application, gets anything from it; a generic camera shows a string and opens nothing.
That is obscurity, not a security property: SEC still holds that anyone with the presence token can open a session, and a photograph of the code yields it either way.

Still to be settled: the error correction level, by the print trial.

### Prefix or bare payload

Settled on the prefix. The reasoning, for the record:

At 33 bytes the prefix costs nothing: `BLITI:` + 53 characters and the bare 53 characters land on the same version at every level (H v5, Q v4, M v3, L v3), by both libqrencode and the crate.
The tightest is M, where the prefixed text (≈338 bits) sits 14 bits under the v3 boundary.

For a prefix:

- A reader rejects a foreign code, such as the device's own hotspot QR code (VIEW) or any other code in frame, on six characters, before decoding.
- A camera app shows `BLITI:…`, which tells a person holding the device what the code belongs to.

For the bare payload:

- The code text is exactly the fragment form `QrPayload::read` already takes, so no new form is parsed.
- A camera app shows an anonymous string, which is the obscurity above at its strongest.
- The payload already validates itself: 53 characters exactly, base32 alphabet, a version marker the reader recognises. QR's own Reed-Solomon makes a misread that passes all three implausible, so the prefix's early validation is mostly a clearer error message.

Dropping the link also removes the path in WEB and QR where following the link delivers the payload in a fragment. Only the application's own camera, or the human-readable rendering typed in, reads a code.

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

- KEY: the presence token becomes 16 bytes of the root's derivation. KEY's keyed hash is BLAKE3 keyed mode, which takes a 32-byte key, so the handle has to be keyed on a 32-byte expansion of the token (the PSK, or a derivation of its own) rather than on the token directly.
- CHN: the PSK becomes a derivation of the token rather than the token itself, overturning "used exactly as KEY produces it with no further derivation".
- SEC: 128 bits against guessing. The token is never sent, the handle is a keyed hash of it, and the handshake cannot be tested offline without a DH secret, so no path offers an offline search at that size.

#### Key fingerprint: cost

- CHN: `NKpsk0` needs the responder static before the handshake. `NXpsk0` sends it encrypted in the second message, and the client checks it against the fingerprint before accepting the session. The second message grows by 48 bytes.
- The `NKpsk0` note that the client's first message is encrypted to the device static key goes away. The handshake payloads are empty (`noise.rs` writes `&[]`), so that confidentiality protects nothing today.
- SEC: impersonating a device takes a keypair whose fingerprint matches, a 128-bit second preimage. Searching board IDs against the fingerprint still costs one argon2id derivation per guess, as searching against the key does.
- KEY: the fingerprint is the first 16 bytes of a BLAKE3 derivation of the device static public key, under a context string of its own.

## Open questions

- [ ] Which handshake pattern replaces `NKpsk0`, for Tech design: `NXpsk0` is the closest, with alternatives (a later psk position, for one) compared and the SEC argument written out. The key it sends is the device static key of KEY, the same X25519 key the code carries today; the advertisement carries only the handle and never the key.

## Trade-offs

- No link. A generic phone camera opening the web app was nice to have, and a native application claiming links only a possibility. Against that, a code that opens nothing for a stranger is worth more, and it costs a version at every level.
- On the way there: an upper-case URL never won a version over the lower-case URL without its trailing slash, so the App Links and Universal Links case question never mattered for sizing.
- The `BLITI:` prefix over the bare payload. It costs no version at 33 bytes, and it gives the reader an early rejection of foreign codes, at the price of naming the code's purpose to anyone who scans it.
- Shrinking the payload is priced here, with the encoding, not treated as fixed. Both cuts are taken: 33 bytes buys back the error correction that encoding alone would have to spend, and it gets the bare prefix to 26 mm at level H.
- The key fingerprint gives up `NKpsk0`'s encryption of the client's first message to the device's key. That message carries an empty payload, so nothing is lost.
- Error correction level is open down to M, to be settled by the print trial rather than fixed at H up front.

## Testing notes

- First print-and-scan run: a three-page A4 sheet, each code with its own random payload and an ID.
  - Candidates: A is today's code (URL + 65 bytes, H, v10, automatic segmentation). B, C and D are `BLITI:` + 33 bytes at H v5, Q v4 and M v3, segmented by hand.
  - Sheet 1, clean, at 0.5, 0.6 and 0.7 mm modules: IDs A5 to D7, where the digit is the module size in tenths of a millimetre. This tests whether 0.70 mm really is the floor.
  - Sheet 2, at 0.7 mm with a light square blot over 5, 10, 15 and 20% of the code, clear of the finders: IDs B-5 to D-20. A is left out because it outlasts all of them.
  - Sheet 3 is a results table with columns for the first round's readers: an Android phone, an iPad, and a webcam through bliti in desktop Chrome.
  - Sheet 4 is an office round: blank columns for five more phones, read with their camera apps.
- The readers exercise different decoders. Desktop Chrome has no `BarcodeDetector`, so the webcam goes through bliti's `quircs` fallback (`scan.rs`). A phone camera app uses the platform's own decoder.
- The Android phone and the iPad are each read two ways: with the camera app, and with bliti in the browser. Android Chrome has `BarcodeDetector`. Safari has none, so bliti on the iPad falls back to `quircs`, the same decoder as the webcam. That gives five readers across three decoders.
- Digital thresholds before printing (largest blot still read by quirc and zbar across eight random payloads each): M v3 ≈ 5%, Q v4 ≈ 11%, H v5 ≈ 17%, today's H v10 ≈ 24%. Every clean code reads at every module size.
- The smaller symbols tolerate less than their level promises (M is nominally 15% of codewords, H 30%). A contiguous blot touches more codewords than the share of area it covers, and at v3 to v5 it also covers the alignment pattern.
- Trial codes are printed from random bytes of the right length. A print test needs no working handshake.
- For the sticker design rather than bliti: whether a light, plain case surface can serve as the quiet zone on a side face, which decides whether v5 (31.6 mm with a printed quiet zone) fits the 35 mm face comfortably.
- Each candidate on paper, and in the protective pouch once one is chosen, since glare costs error correction.
- Print each candidate at the real sticker size and scan with a spread of phone cameras (the web app's own reader, on a spread of phones), including a scuffed or partly covered code.

### Round 1 (Android, 20 cm rig)

A rig holds the phone flat and parallel to the sheet, 20 cm above it.

- Android camera app: read everything except C-20, D-5, D-10, D-15 and D-20.
- bliti in Chrome on the same phone: read nothing. The largest code read only with the sheet at 5 cm.
- The cause was capture, not decoding. Both go through ML Kit, but the scanner asked the camera for nothing beyond `facingMode`, so Chrome opened it at its default of about 640 × 480. At 20 cm that puts a 0.7 mm module at one or two pixels.
- Fixed in `scanner.js`: the scanner asks for up to 3840 × 2160 and continuous focus where the camera offers it. The trial resumes on that build.

### Round 2 (Android, 20 cm rig, scanner fix in)

Camera app and bliti in Chrome on the same Android phone:

| | clean, 0.5 to 0.7 mm | 5% | 10% | 15% | 20% |
| --- | --- | --- | --- | --- | --- |
| A, H v10 | both | | | | |
| B, H v5 | both | both | both | both | both |
| C, Q v4 | both | both | both | both | neither |
| D, M v3 | both | bliti only | bliti only | neither | neither |

- The scanner fix closes the gap: bliti now reads everything the camera app reads, and D-5 and D-10 besides.
- Every clean code reads at 0.5 mm modules. The 0.70 mm floor was pessimistic for this phone at this distance: B at 0.5 mm is 18.5 mm, under the 25 mm target at level H.
- Level H at v5 survives a 20% blot, where the digital check put its edge at about 17%. The platform decoder is more forgiving than `quircs` and `zbar`.
- Level M is out: the camera app fails it at the smallest blot.
- Not yet tested together: damage at 0.5 and 0.6 mm modules. The damage sheet is all at 0.7 mm.
