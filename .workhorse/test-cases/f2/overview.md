# Key schedule and NKpsk0 handshake

## Key schedule

- [x] The root from the canonical board ID matches its known-answer vector at the production parameters (verifies spec: KEY)
- [x] The root derivation's wiring matches its known-answer vector at small parameters (verifies spec: KEY)
- [x] The presence token from the canonical root matches its known-answer vector (verifies spec: KEY)
- [x] The device static private and public keys from the canonical root match their known-answer vectors (verifies spec: KEY)
- [x] The device static private key is clamped per RFC 7748 (verifies spec: KEY)
- [x] The presence token, the static key and the root are all distinct (verifies spec: KEY)

## Deriving on the device

- [x] The cache holds the root, the board ID, the platform serial and the winning source kind, and reads back to the same root (verifies spec: KEY)
- [x] A cache holding a presence token rather than a root is treated as absent and rederived (verifies spec: KEY)

## QR code

- [x] The payload is 65 bytes: version marker, presence token, device static public key, and round-trips (verifies spec: QR)
- [x] The fragment is 104 base32 characters without padding, and matches its known-answer rendering (verifies spec: QR)
- [x] A payload carrying the token alone, or a byte too many, is malformed rather than read (verifies spec: QR)
- [x] URL, fragment and human rendering all round-trip at the new length (verifies spec: QR)

## Handshake

- [x] The protocol name is `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s` (verifies spec: CHN)
- [x] A client holding the device's presence token and static public key completes the handshake and carries traffic both ways (verifies spec: CHN)
- [x] A client with the wrong presence token fails the handshake (verifies spec: CHN)
- [x] An impostor holding the right presence token but another static key fails the handshake (verifies spec: SEC)
- [x] A client holding the right presence token but the wrong static public key fails the handshake, at the Noise layer and against a device session (verifies spec: SEC)

## On the prototype

- [ ] `bliti qr` on the prototype prints a code whose fragment is 104 characters, and the web app reads it, finds the device and opens a channel
- [ ] The CLI client (`bliti connect`) opens a channel to the prototype with the regenerated code
- [ ] A code printed before this change is no longer matched by the prototype
- [ ] The prototype's old identity cache is rederived on first start, and later starts log that the root is cached
