# Implement L1's key schedule and the NKpsk0 handshake

The specs (KEY, QR, CHN, SEC) already describe the target; this card brings the code up to them.
No spec changes are expected.

## Design notes

- `key_schedule` gains a `Root`, derived by the argon2id step that used to produce the token.
  The argon2id inputs and parameters are unchanged, so the old token vectors become the root vectors.
- `Root` yields a `DeviceKeys`: the presence token and the `DeviceStaticKey`, both by BLAKE3 `derive_key`.
  The static private key is clamped per RFC 7748 when derived, and its public half comes from `curve25519-dalek`'s clamped base multiplication.
  `curve25519-dalek` is already in the tree under `snow`, at the same version and with default features off, so it adds nothing to the wasm bundle.
- `QrPayload` carries the device public key alongside the token: 65 bytes, 104 base32 characters.
- `Handshake::initiator(psk, device_public)` and `Handshake::responder(psk, device_static)`; `connect_initiator` and `accept_responder` take the same.
- The device caches the root with its board ID, platform serial and source kind, and derives `DeviceKeys` on every start.
  An old cache holding a token fails to parse and is simply rederived.
- `VERSION` stays at 1: nothing has shipped.
- The wire-compat corpus holds application messages only, and no fixture in the repo carries a QR payload or handshake bytes, so nothing there needs regenerating.

## Checklist

- [x] `bliti-core` key schedule: `Root`, `DeviceKeys`, `DeviceStaticKey`, `DevicePublicKey`, with known-answer tests
- [x] `bliti-core` QR payload at 65 bytes / 104 characters, with round-trip tests
- [x] `bliti-core` Noise `NKpsk0` handshake and stream helpers, with a wrong-static-key test
- [x] `bliti` device: identity cache holds the root; session, device and `bliti qr` pass the keys
- [x] `bliti` CLI client passes the device public key
- [x] `bliti-web` passes the device public key
- [x] Comments and docs that describe `NNpsk0` or ephemeral-only keys rewritten
- [x] `cargo fmt`, clippy, tests across the workspace, wasm build and web tests
