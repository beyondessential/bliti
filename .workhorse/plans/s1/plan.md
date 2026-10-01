# Shrink the QR code to fit the side of the case

The QR code goes from a v10 level H code (40 mm at 0.7 mm modules) to a v5 level H code (18.5 mm at 0.5 mm modules), by cutting the payload from 65 to 35 bytes, dropping the URL for a `BLITI:` prefix, and moving the handshake to `NXpsk0` so the code carries a key fingerprint rather than the key.
The reasoning, sizes, print-trial results and rejected options are in the working doc at `.workhorse/working-docs/s1/working-doc.md`; this plan keeps the decisions and the build.

## Tech notes

### Payload and code

- Payload: payload version (1 byte), presence token (16), key fingerprint (18). 35 bytes is 56 base32 characters with no padding bits; the last four lie within the fingerprint.
- Code text: `BLITI:` + the 56 characters, as one alphanumeric segment, at level H. Built with the `qrcode` crate's `Bits` (`push_alphanumeric_data`, `push_terminator`, `QrCode::with_bits`) at the smallest version that takes it, which is v5 (354 of 368 data bits). The crate's automatic segmentation is content-dependent and can land a payload a version up, so it is not used.
- Reading: one reader for the code text, whether scanned or pasted. Drop everything through the last `:`, strip whitespace and dashes, upper-case, map `0` to `O`, `1` to `I` and `8` to `B` (optional hardening, non-normative in QR), base32-decode, then check the first byte as the payload version before the length. The URL and fragment forms go.
- SVG stays unitless.

### Versions

- The single `VERSION` constant splits into a payload version (in the QR payload) and a version marker (in the advertisement), both 1. Marker 1 reads payload version 1. A device advertises the highest marker it supports.
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

- [x] Key schedule: 16-byte token, PSK, handle by derivation, device KEM key, KEM key digest, fingerprint (KEY)
  - [x] Add `ml-kem` with `cargo add`; confirm `FromSeed` and its wasm build
  - [x] ACVP keyGen vectors test for ML-KEM-768
  - [x] Known-answer test pinning the whole chain from a fixed root
- [x] Versions: split `VERSION` into payload version and version marker across `qr.rs`, `advertisement.rs`, `bliti-web`, the CLI and the wire-compat crate (VER)
- [x] QR payload and code: 35-byte payload, `BLITI:` text, explicit v5 level H segment, single reader (any prefix, `0`/`1`/`8` mapping, version byte first) (QR)
  - [x] Remove the human-readable rendering: `HUMAN_GROUP` and its reader in `qr.rs`, the CLI's printed rendering, and its tests
- [x] Handshake: `NXpsk0`, message 2 payload, fingerprint check in `Handshake` (CHN)
  - [x] Device side (`bliti` session/identity) derives the PSK and KEM key digest
  - [x] CLI `connect`/`configure` take a code through the new reader
- [x] Web client (WEB)
  - [x] `bliti-web` `QrCode`: fingerprint and PSK in place of the public key and token; local names as a list
  - [x] `client.js` chooser filters on every local name
  - [x] `App.jsx`: remove the fragment path (`location.hash`, `replaceState`) and its reload handling
  - [x] Export file name and remembered-device label use the last four characters of the encoded payload (`remembered.js` `lastGroup`, `App.jsx`); remembered devices kept as the code text rather than the rendering
  - [x] Scanner asks the camera for up to 3840 × 2160 and continuous focus (`scanner.js`), with a test
- [x] README and CLI help: the keyed-hash diagram, `NKpsk0` description, and link and fragment descriptions in `README.md` (lines 19, 34, 101, 114) and the `--help` text in `main.rs`
- [x] Remove trial scaffolding: `?scan-only` in `client.js`
- [x] `cargo fmt`, `just test`, `just test-web`

## Implementation notes

- Versions live in `bliti-core`'s `version` module: the payload version, the version marker, and the table of markers this build implements with the payload version each reads. `advertisement::Advertised::heard_by` reads an advertisement against a code (marker first, then the handle under a marker considered), and `advertisement::local_names` gives the names for a code; the CLI's `scan` and `connect` and the web client all go through these.
- `DeviceKeys` carries the token, the PSK, the static key and the KEM key digest. The KEM key itself is derived only to take its digest; nothing holds it.
- The responder refuses a first message carrying a payload, since CHN fixes it empty.
- The known-answer values were checked against an independent implementation (Python `blake3` and `kyber-py`'s ML-KEM) before being pinned. The ACVP vectors are the 25 ML-KEM-768 keyGen cases of NIST's ACVP-Server, pinned by commit in the data file's `source`.
- `ml-kem` adds nothing to the browser bundle: the wasm module built from this branch is the same size as `main`'s, since no client path generates a KEM key.
- The wire-compat baseline still points at a revision before this card. Its oracle compares messages only, which this card does not change, so the baseline does not need to move for it.
