# Shrink the QR code to fit the side of the case

Scenarios verifying the 35-byte payload, the `BLITI:` code at v5 level H, the `NXpsk0` handshake with its fingerprint check, and the split versions.
The print trial that chose the parameters is recorded in the working doc; the cases below are what the built system owes.

## The code

- [ ] Every payload is 35 bytes: payload version, 16-byte token, 18-byte fingerprint (verifies spec: QR)
- [ ] The code text is `BLITI:` followed by 56 base32 characters with no padding (verifies spec: QR)
- [ ] Every code is version 5 at level H, across many random payloads, including ones heavy in the digits `2`–`7`, as QR recommends
- [ ] The last four characters of the encoded payload lie within the fingerprint (verifies spec: QR)
- [ ] The export file name and the remembered-device label use the last four characters of the encoded payload (verifies spec: WEB)
- [ ] The SVG carries the code alone with its quiet zone and no physical size (verifies spec: QR)
- [ ] The same board produces a byte-identical SVG every time

## Reading

- [ ] The code text, the code text in lower case, the payload behind any prefix ending in `:`, and the payload with no prefix all read to the same payload (verifies spec: QR)
- [ ] Only text up to the last `:` is discarded, so a prefix holding a `:` of its own still reads (verifies spec: QR)
- [ ] Dashes and whitespace anywhere are ignored (verifies spec: QR)
- [ ] `0` reads as `O`, `1` as `I` and `8` as `B`, our reader's optional hardening
- [ ] A payload whose first byte is an unsupported payload version is reported as such even when its length differs from 35 bytes (verifies spec: QR)
- [ ] Text that is not a payload and a payload at an unsupported payload version are reported distinctly (verifies spec: QR)
- [ ] The web application reads a code pasted as the code's text (verifies spec: WEB)
- [ ] The web application opened with a fragment in its address does nothing with it

## Key schedule

- [ ] Known-answer: from a fixed root, the token, PSK, handle, X25519 static, ML-KEM-768 encapsulation key, KEM key digest, fingerprint and code text are pinned (verifies spec: KEY)
- [ ] ML-KEM-768 key generation from a seed matches NIST ACVP keyGen vectors (verifies spec: KEY)
- [ ] The handle and the PSK of the same token differ (verifies spec: KEY)

## Handshake

- [ ] A client with the right token and fingerprint completes the handshake and exchanges messages (verifies spec: CHN)
- [ ] A client with a wrong token fails at message 1, and the device sends it no second message (verifies spec: CHN, SEC)
- [ ] A device whose X25519 static does not match the fingerprint fails the handshake at the client, which sends nothing further (verifies spec: CHN, SEC)
- [ ] A device whose KEM key digest does not match the fingerprint fails the handshake the same way (verifies spec: CHN)
- [ ] A fingerprint mismatch is reported to the operator as any failed handshake (verifies spec: CHN, WEB)
- [ ] The CLI and the web client both connect to a device through the new handshake

## Versions and finding the device

- [ ] A code's payload version and the advertised marker are read separately; a code at an unsupported payload version is reported as such before any scan (verifies spec: VER)
- [ ] The chooser is filtered on the local name for every marker the client implements that reads the code's payload version (verifies spec: WEB, VER)
- [ ] An advertisement at a marker the client does not implement is reported as unsupported, not as a mismatch (verifies spec: VER)
- [ ] Codes carry payload version 1 and devices advertise version marker 1 (verifies spec: VER)
- [ ] The name shown before the chooser is the one for the highest marker considered (verifies spec: WEB)

## Camera

- [x] The scanner asks the camera for a stream of at least 1920 × 1080 from the rear camera
- [ ] On an Android phone 20 cm above the code, the web application reads every code the camera app reads
- [ ] The same on an iPhone or iPad, through a Web Bluetooth browser
- [ ] The same through a desktop webcam, where the browser has no QR detector

## Printing

- [ ] A v5 level H code at 0.5 mm modules with a 10% and a 15% blot reads in the web application on Android
- [ ] The same codes read through the chosen protective pouch
- [ ] A v5 code at 0.5 mm modules with its quiet zone fits a 35 mm side face of the case
