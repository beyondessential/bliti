---
id: QR
---

# QR code

A device's QR code carries its [presence token](overview.md#presence-token), [key fingerprint](overview.md#key-fingerprint) and [payload version](overview.md#payload-version).

## Borrowed terms

| term | meaning |
| --- | --- |
| QR code | The two-dimensional bar code symbology defined in ISO/IEC 18004. |
| error correction level | One of the four Reed-Solomon strengths that symbology offers, L, M, Q and H in increasing order, each trading data capacity for tolerance of damage. |
| symbol version | The size of a QR code, from 1 to 40, each a fixed number of modules a side. |
| alphanumeric mode | The symbology's encoding of the digits, the upper-case letters, space and `$ % * + - . / :`, which packs two characters into eleven bits. |
| base32 | The encoding of [RFC 4648](https://www.rfc-editor.org/rfc/rfc4648), over the alphabet A to Z followed by 2 to 7. |

## Payload

The payload MUST be 35 bytes: the payload version, then the 16 bytes of the presence token, then the 18 bytes of the key fingerprint of [KEY](key-schedule.md).

The payload MUST be encoded as base32 without padding, giving 56 characters.

> [!NOTE]
> RFC 4648 pads by default and leaves it to a referencing specification to say when padding is omitted. Thirty-five bytes is a whole number of base32 blocks, so there is nothing to pad.
> The fingerprint lets a client authenticate the device rather than merely share a secret with whoever holds one. It reveals nothing about the token or the board ID, so carrying it in the clear costs nothing.

## The code

The QR code MUST encode the text `BLITI:` followed by the encoded payload.

> [!NOTE]
> The code is best encoded as one segment in alphanumeric mode, at error correction level H, at the smallest symbol version that holds it, which is version 5.
> The prefix and base32 lie wholly within the alphanumeric set, which packs them tighter than any other mode, and one segment leaves an encoder's optimiser no choice, so every device's code is the same size.
> Level H tolerates the most damage of the four.

## Reading

A client MUST read a payload from the text of a QR code, or from text a person enters, such as the human-readable rendering.

A client MUST discard everything up to and including the last `:` in the text, where there is one, and MUST discard dashes and whitespace.

A client MUST accept the payload in any case, and MUST read `0` as `O` and `1` as `I`.

A client MUST read the payload version from the first byte of the decoded payload before it checks anything else about the payload.

A client MUST report text that does not hold a payload, and a payload at a payload version it does not support, as the distinct conditions they are.

> [!NOTE]
> One reading covers the code and the rendering, because a person typing or pasting either should not have to know which they hold.
> Base32 has no `0` or `1`, so reading them as the letters they resemble costs nothing.
> The payload version comes first so that a client can recognise a payload at a version it does not support, however that version lays out the bytes that follow.

## What a generator offers

A generator MUST offer the human-readable rendering of the payload: the 56 characters of the encoded payload in fourteen groups of four, separated by dashes.

A generator MUST offer the code as an SVG image.
The image MUST carry the code alone, with its quiet zone, dark modules on a light ground.
The image MUST NOT state a physical size.

> [!NOTE]
> The last group of the rendering lies wholly within the key fingerprint, so it can name a device without giving away anything secret.

## Generation

A QR code MUST be generated from a board ID.

A generator MAY read that board ID from the board in front of it, or from a list gathered beforehand.

A generator MUST produce the same payload for a given board every time.

Generation MUST be refused where the board offers no usable source, as [BID](board-id.md) specifies.

> [!NOTE]
> Whether a list can be gathered before the boards are to hand depends on which source wins the precedence in [BID](board-id.md). A platform serial can be known without the board present, while an Endorsement Key name or written one-time-programmable memory is readable only from the board itself.
> Because the payload for a board is fixed, no record of the codes issued is kept or needed, and a damaged code is replaced by printing the same payload again, recovered from the code, from the rendering, or by deriving it from the board once more.
