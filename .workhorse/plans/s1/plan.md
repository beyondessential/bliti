# Shrink the QR code to fit the side of the case

The QR code goes from a v10 level H code (40 mm at 0.7 mm modules) to a v5 level H code (18.5 mm at 0.5 mm modules), by cutting the payload from 65 to 35 bytes, dropping the URL for a `BLITI:` prefix, and moving the handshake to `NXpsk0` so the code carries a key fingerprint rather than the key.
The reasoning, sizes, print-trial results and rejected options are in the working doc at `.workhorse/working-docs/s1/working-doc.md`; this plan keeps the decisions and the build.

## Tech notes

### Payload and code

- Payload: payload version (1 byte), presence token (16), key fingerprint (18). 35 bytes is 56 base32 characters with no padding bits, fourteen groups of four.
- Code text: `BLITI:` + the 56 characters, as one alphanumeric segment, at level H. Built with the `qrcode` crate's `Bits` (`push_alphanumeric_data`, `push_terminator`, `QrCode::with_bits`) at the smallest version that takes it, which is v5 (354 of 368 data bits). The crate's automatic segmentation is content-dependent and can land a payload a version up, so it is not used.
- Reading: one reader for the code text and the typed rendering. Strip whitespace and dashes, strip a leading `bliti:` in any case, upper-case, base32-decode. The URL and fragment forms go.
- SVG stays unitless.

### Versions

- The single `VERSION` constant splits into a payload version (in the QR payload) and a version marker (in the advertisement), both 1. Marker 1 reads payload version 1.
- A client computes one local name per marker it implements that reads the code's payload version. Today that is one name, but `QrCode` in `bliti-web` returns a list and the chooser filters on all of them, so a second marker later needs no change to the client's shape.

### Key schedule (`key_schedule.rs`)

- Presence token: first 16 bytes of `derive_key("bliti presence token", root)`.
- PSK: `derive_key("bliti pre-shared key", token)`.
- Handle: first 8 bytes of `derive_key("bliti advertised handle", token)`. The keyed hash and its constant go.
- Device KEM key: ML-KEM-768 from a 64-byte seed, `derive_key("bliti device kem seed", root)` read through BLAKE3's extendable output (`Hasher::new_derive_key(..).update(root).finalize_xof()`), via the RustCrypto `ml-kem` crate's `FromSeed`. Add with `cargo add`.
- KEM key digest: `derive_key("bliti device kem key digest", encapsulation key)`.
- Fingerprint: first 18 bytes of `derive_key("bliti device key fingerprint", x25519 public ‖ KEM key digest)`.
- `ml-kem` is unaudited. It only generates a key here, but the fingerprint commits to that exact key, so key generation is pinned to NIST ACVP ML-KEM-768 keyGen vectors in a test. A non-conformant release would otherwise change every fingerprint silently.

### Handshake (`channel/noise.rs`)

- `Noise_NXpsk0_25519_ChaChaPoly_BLAKE2s`. `snow` 0.10 supports it; verified end to end, including rejection of a wrong PSK at message 1.
- Initiator built from the PSK and the expected fingerprint, with no remote static. After reading message 2 it takes `get_remote_static()` and the 32-byte payload (the KEM key digest), recomputes the fingerprint, and returns `ChannelError::Handshake` on a mismatch before the handshake reports finished.
- Responder writes the KEM key digest as the message 2 payload. Message 2 becomes 128 bytes.
- The check lives in `bliti-core`, so the CLI and the web client (through wasm) share it.

### Rejected, for the record

Base45; an upper-case URL; the bare payload without a prefix; levels M and Q; `NKpsk0` (49-byte payload, v7); `NNpsk0` (breaks device authentication); `NXpsk2` (device replies to strangers with its static key); hybrid PQ now (`snow`'s `hfs` is round-3 Kyber in C, `clatter` is unaudited); a physical size on the SVG.

## Build

- [ ] Key schedule: 16-byte token, PSK, handle by derivation, device KEM key, KEM key digest, fingerprint (KEY)
  - [ ] Add `ml-kem` with `cargo add`; confirm `FromSeed` and its wasm build
  - [ ] ACVP keyGen vectors test for ML-KEM-768
  - [ ] Known-answer test pinning the whole chain from a fixed root
- [ ] Versions: split `VERSION` into payload version and version marker across `qr.rs`, `advertisement.rs`, `bliti-web`, the CLI and the wire-compat crate (VER)
- [ ] QR payload and code: 35-byte payload, `BLITI:` text, explicit v5 level H segment, single reader, fourteen-group rendering (QR)
- [ ] Handshake: `NXpsk0`, message 2 payload, fingerprint check in `Handshake` (CHN)
  - [ ] Device side (`bliti` session/identity) derives the PSK and KEM key digest
  - [ ] CLI `connect`/`configure` take a code through the new reader
- [ ] Web client (WEB)
  - [ ] `bliti-web` `QrCode`: fingerprint and PSK in place of the public key and token; local names as a list
  - [ ] `client.js` chooser filters on every local name
  - [ ] `App.jsx`: remove the fragment path (`location.hash`, `replaceState`) and its reload handling
  - [ ] Export file name and remembered-device label still use the last group, now within the fingerprint
  - [x] Scanner asks the camera for up to 3840 × 2160 and continuous focus (`scanner.js`), with a test
- [ ] README and CLI help: the link and fragment descriptions in `README.md` (lines 101, 114) and the `--help` text in `main.rs`
- [ ] Remove trial scaffolding: `?scan-only` in `client.js`
- [ ] `cargo fmt`, `just test`, `just test-web`
