---
id: KEY
---

# Key schedule

One memory-hard derivation takes the [board ID](overview.md#board-id) of [BID](board-id.md) to a root, and everything else descends from that root cheaply: the [presence token](overview.md#presence-token) and [key fingerprint](overview.md#key-fingerprint) carried in the QR code, the pre-shared key and device keys of the handshake, and the [advertised handle](overview.md#advertised-handle) broadcast over BLE.

Every constant and context string below is public, and all are compiled into devices, generators and clients.

> [!NOTE]
> Publishing them weakens nothing, because no derivation runs backwards. They provide domain separation, so that a value from one step is not a valid value at another.

## Borrowed terms

| term | meaning |
| --- | --- |
| argon2id | The hybrid variant of the Argon2 memory-hard function, defined in [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106). |
| memory, passes, lanes | Argon2's cost parameters: how much memory a derivation fills, how many times it passes over that memory, and how many lanes it divides the work into. |
| key derivation | BLAKE3's `derive_key` mode, which takes a hardcoded context string and key material, defined in [the BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs). Its output is the 32-byte subkey unless a length is given, in which case it is that many bytes of the mode's extendable output. |
| X25519 | The Diffie-Hellman function on curve25519 defined in [RFC 7748](https://www.rfc-editor.org/rfc/rfc7748). |
| ML-KEM-768 | The key-encapsulation mechanism of [FIPS 203](https://csrc.nist.gov/pubs/fips/203/final) at its middle parameter set, whose key pair FIPS 203 derives deterministically from a 64-byte seed `d ‖ z`. |
| encapsulation key | The public half of an ML-KEM key pair, 1184 bytes at ML-KEM-768. |

## The root

The root MUST be derived with argon2id, version `0x13`, over:

| input | value |
| --- | --- |
| password | the source tag byte of [BID](board-id.md), followed by the raw bytes of the source value, most significant first |
| salt | `3edfe95ceb86fadd23d46a8734c7eb13` |
| memory | 2 GiB |
| passes | 1 |
| lanes | 2 |
| output | 32 bytes |

The password MUST carry the source value's raw bytes, never a text rendering of them.

Every parameter above is part of the derivation and MUST NOT be treated as a tuning choice.

An implementation MAY compute the lanes concurrently or in sequence, which does not change the result.

> [!NOTE]
> Changing any parameter produces a different root, and so a different token and fingerprint, orphaning every QR code already printed under the old ones.
> The parameters are memory-heavy rather than pass-heavy because memory bounds how many guesses an attacker runs at once, while passes only make each guess longer. They are chosen to be affordable on the slowest board in scope while remaining costly in bulk.
> A Raspberry Pi serial is read as characters, and deriving from those characters rather than from the eight bytes they denote produces a different token, permanently, once a code carrying it has been printed.
> The tag makes a value that is byte-identical across two kinds of source derive differently. Lengths are fixed per kind of source, so the tag leaves the input unambiguous without a length prefix.

The root MUST NOT be carried in a QR code, in an advertisement, or on the wire.

## Presence token

The presence token MUST be the first 16 bytes of the key derivation of the root under the context string `bliti presence token`.

## Pre-shared key

The pre-shared key MUST be the key derivation of the presence token under the context string `bliti pre-shared key`, 32 bytes.

> [!NOTE]
> The handshake of [CHN](channel.md) takes a 32-byte pre-shared key, and the QR code has room for a 16-byte token. Sixteen bytes is beyond guessing, online or offline.

## Device static key

The device static private key MUST be the key derivation of the root under the context string `bliti device static key`, clamped as [RFC 7748](https://www.rfc-editor.org/rfc/rfc7748) requires: the three least significant bits of the first byte cleared, the most significant bit of the last byte cleared, and the second most significant bit of the last byte set.

The device static public key MUST be the X25519 public key for that private key.

## Device KEM key

The device KEM key MUST be the ML-KEM-768 key pair generated from a seed of 64 bytes of the key derivation of the root under the context string `bliti device kem seed`, the first 32 bytes as `d` and the last 32 as `z`.

The KEM key digest MUST be the key derivation of the encapsulation key under the context string `bliti device kem key digest`.

> [!NOTE]
> The key fingerprint commits to the KEM key so that a QR code stays valid under any protocol version whose handshake authenticates the device by it, as [VER](version.md) allows. A QR code cannot be changed once it is fixed to an enclosure, and a handshake can.
> The fingerprint commits to the key FIPS 203 generates from the seed, exactly. Any departure from that generation produces a different fingerprint for every device.

## Key fingerprint

The key fingerprint MUST be the first 18 bytes of the key derivation, under the context string `bliti device key fingerprint`, of the device static public key followed by the KEM key digest.

> [!NOTE]
> The static key and the KEM key descend from the root, so one argon2id derivation per candidate board ID gates any search for either, and the fingerprint offers no shorter path to the board ID than the keys would.
> Deriving them through the root rather than cheaply from the board ID keeps that cost on the path. Were it cheap, someone holding a QR code could search the board ID space against the fingerprint in it and bypass the memory-hard step entirely, which [SEC](security.md) relies on being impossible.
> A holder of the QR code has the presence token, but the token does not invert to the root, so it does not yield the static key.
> Eighteen bytes puts a key matching a given fingerprint out of reach, and makes the payload of [QR](qr-code.md) a whole number of base32 groups.

## Advertised handle

The advertised handle MUST be the first eight bytes of the key derivation of the presence token under the context string `bliti advertised handle`.

> [!NOTE]
> This derivation is deliberately cheap. A client computes it for every QR code it holds, so a memory-hard function here would be felt before every scan.
> Eight bytes makes a collision between two devices at one site implausible, and fits the advertising budget of [ADV](discovery.md).
> The handle and the pre-shared key descend from the token under different context strings, so the one reveals nothing about the other.

## Deriving on the device

A device MUST derive its own root, and MUST derive its presence token, pre-shared key, device static key and device KEM key from it.

A device MUST cache the root together with the board ID it was derived from, the platform serial of the board it was derived on, and which kind of source won the precedence.

A device MUST establish whether that cache still holds by comparing those values against the board, and MUST run the derivation only where the cache is absent or the comparison fails.

On start a device MUST read the platform serial and probe which kinds of source are present, both cheap as [BID](board-id.md) specifies. Then:

- where the serial and the strongest kind present both match the cache, the cached root stands, and the device MUST NOT read a source value or run the memory-hard derivation
- where the strongest kind present is stronger than the cached one, the device MUST report a dead QR code as [BID](board-id.md) requires
- where the serial differs, the device MUST evaluate the precedence, read the winning source, and derive

> [!NOTE]
> Deriving on the device makes the chain reproducible from the board everywhere, rather than only on machines large enough to run the derivation comfortably.

## Versioning

Everything specified above is covered by the payload version of [VER](version.md): a change to any of it is a new payload version.
