---
status: draft
---

# Shrink the QR code to fit the side of the case

Get the device QR code to 30 mm or less at a module size a phone still reads, through its payload, its encoding, and the text around it.

## Behaviour

QR currently carries a 65-byte payload (version marker, 32-byte presence token, 32-byte device static public key) as 104 base32 characters at level H, which comes out at v10, 40 mm.
The aim was a code of 30 mm or less, ideally 25 mm, that fits a 35 mm side face of the 90 × 90 × 35 mm case.
The settled design is level H, version 5, printed at 0.5 mm modules: 18.5 mm a side, 22.5 mm with its quiet zone.

Settled so far:

- The payload is 33 bytes: the version marker, a 16-byte presence token, and a 16-byte fingerprint of the device static public key.
- The client authenticates the device by checking the static key it receives in the handshake against the fingerprint, rather than by knowing the key beforehand.
- A device whose static key does not match the fingerprint fails the handshake, and the operator sees it as any failed handshake.
- The payload stays base32. Base45 buys no version at any size priced, cannot ride in a URL fragment, and makes a poor human-readable rendering.
- The code is generated at error correction level H, which comes out at version 5 (37 modules) for `BLITI:` and the 33-byte payload.
- The generator segments the code explicitly, so every board's code is the same version and that version is the smallest the text allows.
- The code carries no link. Its text is `BLITI:` followed by the 53 base32 characters of the payload, all in QR's alphanumeric set.
- Only the application's own camera, or the human-readable rendering typed in, reads a code. The follow-the-link path, and the fragment it delivers, go from QR and WEB.
- What a sticker looks like is a product decision outside bliti. Bliti provides the QR code, as SVG with its quiet zone, and the human-readable rendering alongside it for whoever lays the sticker out to use or not. QR's rule that the rendering is printed alongside the code goes.
- The SVG carries no physical size. Print size is left to whoever lays the sticker out; 0.5 mm modules is what the trial found reads, and is recorded for them, not imposed by the image.
- The rendering is the payload's 53 characters grouped for legibility, without the prefix.
- The reader takes the code text and anything a person would type for it: the prefix in any case or absent, the payload in any case, dashes and spaces ignored. One reading covers the code and the rendering.
- With no link, the application's camera is the only way a code is read, so it has to read a code at least as well as the phone's own camera app does: from the same distance, at the same size.
- The key schedule expands the 16-byte token twice: the pre-shared key is a BLAKE3 derivation of it under `bliti pre-shared key`, and the advertised handle is the first eight bytes of a derivation under `bliti advertised handle`. The handle and the Noise key are never derived the same way.
- The handshake is `Noise_NXpsk0_25519_ChaChaPoly_BLAKE2s`.
- The sticker is ready for a post-quantum handshake without a reprint: the fingerprint commits to the device's X25519 static key and to an ML-KEM-768 key, both derived from the root. The handshake stays classical until a later card moves it to a hybrid.
- In message 2 the device sends a digest of its ML-KEM key, so the client can check the fingerprint while the handshake is still classical.

With no link, only someone who knows what the code is for, and has the application, gets anything from it; a generic camera shows a string and opens nothing.
That is obscurity, not a security property: SEC still holds that anyone with the presence token can open a session, and a photograph of the code yields it either way.

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

### Handshake

Checked against `snow` 0.10, the crate `noise.rs` builds on, with empty payloads as today:

| pattern | message 1 | message 2 | wrong PSK rejected at message 1 |
| --- | --- | --- | --- |
| `NKpsk0` (today) | 48 B | 48 B | yes |
| `NXpsk0` | 48 B | 96 B | yes |
| `NXpsk2` | 48 B | 96 B | no |

`NXpsk0` (`-> psk, e` / `<- e, ee, s, es`), chosen:

- The PSK is mixed in before the first message, and the `e` token mixes the key under psk modifiers, so even an empty first payload carries a tag. A device rejects a client without the token at message 1, before any DH of its own and before sending its static key.
- The device static travels encrypted under `ee` and the PSK, so a passive listener never sees it, and neither does an active one without the token.
- `snow` exposes the received key through `get_remote_static()` once message 2 is read, which is where the client checks it against the fingerprint.
- Message 2 grows by 48 bytes, the static and its tag. With its length prefix it is 98 bytes: one notification at an ATT_MTU of 101 or more, chunked below that, as CHN already allows for every message.

`NXpsk2` was considered and rejected: with the PSK only at the end of message 2, the device runs its DH and sends its static key to anyone who connects, and only the client ever checks the PSK. That spends device work on strangers and hands them the static key.

`NNpsk0` with the device proving its identity some other way (a signature over the handshake hash, say) was not pursued. `NX` already authenticates the responder static inside Noise, with the analysis Noise publishes for it.

`XXpsk*` and other patterns with an initiator static do not fit: the client has no long-term identity, and the PSK is what authenticates it.

#### Every pattern `snow` 0.10 implements

Run end to end with empty payloads, X25519, ChaChaPoly and BLAKE2s. Sizes are with a psk modifier, which adds 16 bytes to the first message of a pattern without one there already.

The first letter is what the client has, the second what the device has: `N` no static key, `K` a static the other end knows in advance, `X` a static sent in the handshake, `I` a static sent immediately, in the first message. A `1` defers that party's authenticating DH by one message.

| pattern | messages | bytes | client static | how the client learns the device static | QR payload, code at H and 0.5 mm | verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `N`, `K`, `X` | 1 | 48 / 48 / 96 | none / known / sent | known in advance | n/a | One-way: no device ephemeral, so no forward secrecy and no reply channel. |
| `NN` | 2 | 48/48 | none | never | 17 B, v3, 14.5 mm | The token is the only credential, so a photograph of the code impersonates the device. Breaks SEC. |
| `NK` (today) | 2 | 48/48 | none | in the QR code | 49 B, v7, 22.5 mm | Works; costs the code two versions over `NX`. |
| `NX` | 2 | 48/96 | none | in the handshake, checked against a fingerprint | 33 B, v5, 18.5 mm | **Chosen.** |
| `NK1` | 2 | 48/48 | none | in the QR code | 49 B, v7 | `NK` with `es` moved to message 2, giving up the one thing `NK` has over `NX` (a first message encrypted to the device's key) at `NK`'s size. |
| `NX1` | 3 | 48/96/16 | none | in the handshake | 33 B, v5 | `NX` with the device's authentication deferred to a third message: a round trip more for nothing, as there is no identity to hide or KEM to fit. |
| `KN`, `KK`, `KX` and deferred `K1N`, `K1K`, `KK1`, `K1K1`, `K1X`, `KX1`, `K1X1` | 2 or 3 | 48 to 96 | known to the device in advance | varies | n/a | The device would need every client's key beforehand. Clients are arbitrary phones the device has never met. |
| `XN`, `XK`, `XX` and deferred `X1N`, `X1K`, `XK1`, `X1K1`, `X1X`, `XX1`, `X1X1` | 3 or 4 | 48/48–96/64(/16) | sent in message 3 | varies | 17 to 49 B | The client would need a long-term key, and nothing authorises one: the token is what authenticates a client. A message and 48 bytes more. |
| `IN`, `IK`, `IX` and deferred `I1N`, `I1K`, `IK1`, `I1K1`, `I1X`, `IX1`, `I1X1` | 2 or 3 | 96/48–96(/16) | sent in message 1 | varies | 17 to 49 B | As `X*`, with the client's key in the first message instead. |

psk position, which behaves the same across every pattern above:

| position | wrong token caught | effect |
| --- | --- | --- |
| `psk0` | message 1, by the device | The device answers no one without the token. Conventional for a key known before the handshake, and what CHN uses today. |
| `psk1` | message 1, by the device | Same outcome as `psk0` here; the token is mixed after the client's ephemeral rather than before. Nothing to choose between them with empty payloads. |
| `psk2` | message 2, by the client | The device does its DH and replies to anyone. Under `NX` that reply carries the device's static key. |
| `psk3` (three-message patterns) | message 3, by the device | The device replies to anyone first, as `psk2`. |

Not covered by the table: `snow`'s `hfs` modifier (hybrid forward secrecy with a post-quantum KEM) is a modifier, not a pattern, and was not evaluated.

### Post-quantum

Devices stay in the field for years, and a cryptographically relevant quantum computer is a plausible risk inside that span.
What it threatens, under `NXpsk0` as chosen:

- **Recorded sessions, decrypted later.** The PSK is mixed into every key, so breaking X25519 is not enough: the attacker also needs the token, a 128-bit symmetric secret that quantum search does not reduce to anything practical. Anyone who has seen the sticker has the token, though. For them, a recorded session is protected only by `ee`, which a quantum computer breaks.
- **Impersonating the device.** A token holder receives the device's X25519 static key in message 2. A quantum computer recovers the private key from it, and the attacker can then answer as the device. Opening a session with the token is already possible classically (SEC); impersonating the device is what changes.
- **Not threatened:** the token and the fingerprint. A 16-byte fingerprint resists a quantum second-preimage search at about 2⁶⁴ sequential BLAKE3 evaluations.

The sticker is what cannot be changed once printed; the handshake can change with a software update.
So what the fingerprint commits to matters more than which handshake runs today.
ML-KEM keys are far too large for the code (an ML-KEM-768 encapsulation key is 1184 bytes), but the fingerprint does not care how large the key it covers is. Moving from carrying the key to carrying its fingerprint is what makes a post-quantum device key possible at all.

Libraries:

- `snow` 0.10's `hfs` modifier uses `pqcrypto-kyber`: round-3 Kyber rather than FIPS 203 ML-KEM, in C, which does not build for `wasm32-unknown-unknown` without extra toolchain. Not usable as is.
- `clatter` 2.3 (2026-08): `no_std` Noise with PQNoise KEM patterns and true hybrid handshakes (DH and KEM in the same messages), ML-KEM-512/768/1024 through the pure-Rust RustCrypto `ml-kem` backend, PSK supported. It has not been formally audited, and its README does not mention wasm.

Options:

| option | sticker commits to | handshake now | recorded session, attacker has the token + a quantum computer | device impersonation by the same | cost now |
| --- | --- | --- | --- | --- | --- |
| 1. Classical | X25519 key | `NXpsk0`, `snow` | decrypted | possible | none |
| 2. Classical, sticker ready | X25519 key and an ML-KEM-768 key, both derived from the root | `NXpsk0`, `snow` | decrypted until the handshake goes hybrid | possible until then | the device derives one more key; no change to the sticker's size |
| 3. Hybrid ephemeral | X25519 key | `NXpsk0` plus an ephemeral ML-KEM in the same messages | protected | possible | `clatter` in place of `snow`; about 1.2 KB more each way over BLE |
| 4. Full hybrid | both keys | hybrid NX with the device static as both a DH and a KEM key | protected | prevented | as 3, and the device's KEM key is sent and checked against the fingerprint |

Chosen: option 2. It keeps every later choice open without a reprint: the handshake can go to 3 or 4 by software update, and every sticker printed meanwhile still verifies.
It fixes the KEM and its parameter set at print time.

#### Sticker readiness: how it works

- ML-KEM-768 seed: 64 bytes of BLAKE3's derive-key mode over the root under `bliti device kem seed`, read through its extendable output. The key pair comes from it by FIPS 203's deterministic key generation, which RustCrypto `ml-kem` (0.3) exposes as `FromSeed` over its 64-byte `Seed`.
- KEM key digest: `derive_key("bliti device kem key digest", encapsulation key)`, 32 bytes.
- Fingerprint: the first 16 bytes of `derive_key("bliti device key fingerprint", X25519 public key ‖ KEM key digest)`.
- Message 2 carries the KEM key digest as its payload: encrypted, and bound to the handshake. It grows from 96 to 128 bytes. The client recomputes the fingerprint from the received X25519 static and the digest.
- Later, a hybrid handshake sends the full encapsulation key (1184 bytes), and the client checks it against the same digest, so the fingerprint on every printed sticker still verifies.
- Why a digest and not the whole key now: equally secure, as the X25519 static is what authenticates the device classically and the digest only has to be bound to the handshake. Sending 1184 bytes the client cannot use yet would only lengthen message 2.
- `ml-kem` has not been independently audited. Here it only generates a key that is never used yet, but the key must be exactly the one FIPS 203 specifies, now and in every later version of the crate: a non-standard key fixed in a later release would change every fingerprint and orphan every sticker. Key generation is pinned to NIST's ACVP ML-KEM key generation vectors in a test.
- The generator needs the root, which it already derives from the board ID, so it computes both keys. The device derives the KEM key alongside its X25519 key; ML-KEM key generation is cheap next to the argon2id derivation.

### Fingerprint check

- The initiator is built from the presence token and the fingerprint, not a public key. After reading message 2 it takes the remote static and the KEM key digest from the payload, computes the fingerprint, and fails the handshake on a mismatch, before the handshake is reported finished and before any transport message is sent.
- The check lives in `bliti-core`'s `Handshake`, so the web client (through wasm) and the CLI share it rather than each remembering to make it.
- A mismatch is a `ChannelError::Handshake` like any other, matching the settled behaviour that the operator sees it as any failed handshake.

### Key schedule

- Presence token: the first 16 bytes of `derive_key("bliti presence token", root)`.
- PSK: `derive_key("bliti pre-shared key", token)`, 32 bytes, used at PSK position zero.
- Handle: the first 8 bytes of `derive_key("bliti advertised handle", token)`. Chosen over the alternatives below because it is one call with nothing redundant, and SEC keeps the handle fixed, so the message slot the other shapes leave is room for a decision already taken the other way:

| shape | handle | BLAKE3 calls | key separation from the Noise PSK | room for a handle that varies |
| --- | --- | --- | --- | --- |
| derive directly | `derive_key("bliti advertised handle", token)[..8]` | 1 | yes | none: a BLAKE3 context string is a fixed constant, so a varying handle means changing shape |
| key, then hash | `keyed_hash(derive_key("bliti advertised handle", token), constant)[..8]` | 2 | yes | the message slot: a time epoch in place of the constant gives a rotating handle |
| keyed on the PSK | `keyed_hash(psk, constant)[..8]` | 1 after the PSK | no: the Noise key also keys the hash | the message slot |

All three are a PRF of the token, so none changes what an observer learns. The constant in the last two carries nothing the context string does not already separate.
- Fingerprint: the first 16 bytes of `derive_key("bliti device key fingerprint", device static public key)`.
- The device derives the PSK from the token it derives; it never needs the fingerprint. The generator computes the fingerprint from the public key.
- The version marker stays at 1. Nothing has shipped, and the house rules say no version bumps for breaking changes before the first release.

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

- [ ] Does the iOS Camera app offer to open a registered custom scheme from a QR code? Only matters once a native application exists. The Android camera app does: tapping a `BLITI:` code, it tried to hand it to an application and reported that none was installed.

## Trade-offs

- No link. A generic phone camera opening the web app was nice to have, and a native application claiming links only a possibility. Against that, a code that opens nothing for a stranger is worth more, and it costs a version at every level.
- On the way there: an upper-case URL never won a version over the lower-case URL without its trailing slash, so the App Links and Universal Links case question never mattered for sizing.
- The `BLITI:` prefix over the bare payload. It costs no version at 33 bytes, and it gives the reader an early rejection of foreign codes, at the price of naming the code's purpose to anyone who scans it.
- A native iOS application is more likely than it was: iOS has no Web Bluetooth, so an iPhone or iPad operator otherwise needs a browser like Bluefy. `BLITI:` + payload is already a URI under RFC 3986 (`bliti` is a valid scheme, and schemes are case-insensitive), so such an application could register the `bliti` scheme and the system camera would hand the code to it. A phone without the application still gets only a string, so the obscurity holds. The bare payload would not have allowed this.
- Shrinking the payload is priced here, with the encoding, not treated as fixed. Both cuts are taken: 33 bytes buys back the error correction that encoding alone would have to spend, and it gets the bare prefix to 26 mm at level H.
- The key fingerprint gives up `NKpsk0`'s encryption of the client's first message to the device's key. That message carries an empty payload, so nothing is lost.
- Level H at v5, 0.5 mm modules. The print trial (round 2) settled it:
  - M (D series) is out. The camera app failed it at the smallest blot, and bliti at 15%.
  - Q (C series) is the smallest that works, and stays the fallback if a code has to shrink further: v4, 16.5 mm at 0.5 mm modules, surviving a 15% blot.
  - H (B series) read at every module size down to 0.5 mm, and through a 20% blot at 0.7 mm, once the scanner asked the camera for a full-resolution stream. At 0.5 mm it fits a 35 mm side face with room to spare.

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
- Still owed: the settled code (H v5, 0.5 mm modules) with a 10% and a 15% blot, on paper and in the protective pouch once one is chosen. The trial tested damage only at 0.7 mm.
- ML-KEM-768 key generation from a seed matches NIST's ACVP key generation vectors, so a later `ml-kem` release cannot silently change the key a fingerprint commits to.
- Known-answer tests pin the whole derivation from a fixed root: token, PSK, handle, X25519 static, ML-KEM key, KEM key digest, fingerprint, and the code text.
- A handshake against a device whose X25519 static, or whose KEM key digest, does not match the fingerprint fails, and is reported as any failed handshake.
- A client without the token is rejected at message 1, and never receives message 2.
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

### Trial scaffolding to remove

- [ ] `?scan-only` in `client.js`, which skips the Web Bluetooth check so an iPad can open the scanner. The page reads codes and connects to nothing.
