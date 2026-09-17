---
id: QR
---

# QR code

A device's QR code carries the [presence token](overview.md#presence-token) and the [version marker](overview.md#version-marker) on the outside of the device.

## Borrowed terms

| term | meaning |
| --- | --- |
| QR code | The two-dimensional bar code symbology defined in ISO/IEC 18004. |
| error correction level | One of the four Reed-Solomon strengths that symbology offers, L, M, Q and H in increasing order, each trading data capacity for tolerance of damage. |
| base32 | The encoding of [RFC 4648](https://www.rfc-editor.org/rfc/rfc4648), over the alphabet A to Z followed by 2 to 7. |
| fragment | The part of a URL after `#`, which a client resolves locally and does not send to a server. |

## Payload

The payload MUST be 33 bytes: the version marker, followed by the 32 bytes of the presence token.

The payload MUST be encoded as base32 without padding, giving 53 characters.

> [!NOTE]
> RFC 4648 pads by default and leaves it to a referencing specification to say when padding is omitted.

## The URL

The QR code MUST encode the URL `https://bliti.tamanu.app/` with the payload as its fragment.

The URL MUST be lower case.

A client that is already open MAY read the code with its own camera rather than follow the link, as [WEB](web-app.md) specifies. Both paths yield the same payload.

> [!NOTE]
> A generic phone camera opens the page, so a device is reachable without installing anything first, and the fragment never leaves the device that scanned it.
> A native application claims a link by matching scheme and host literally, which is what fixes the case.

## Printing

A QR code SHOULD be produced at error correction level H.

A human-readable rendering of the payload SHOULD be printed alongside the code, in the same characters as the fragment.

> [!NOTE]
> Level H tolerates the most damage of the four, which is what a code fixed to an enclosure needs.
> The rendering is what keeps a device reachable once the code itself is scuffed.
> Base32 draws a coarser code than mixed-case text carrying the same payload would, because the symbology spends fewer bits on upper-case letters and digits, and a coarser code is what a phone camera reads off an enclosure.

## Generation

A QR code MUST be generated from a board ID.

A generator MAY read that board ID from the board in front of it, or from a list gathered beforehand.

A generator MUST produce the same payload for a given board every time.

Generation MUST be refused where the board offers no usable source, as [BID](board-id.md) specifies.

> [!NOTE]
> Whether a list can be gathered before the boards are to hand depends on which source wins the precedence in [BID](board-id.md). A platform serial can be known without the board present, while an Endorsement Key name or written one-time-programmable memory is readable only from the board itself.
> Because the payload for a board is fixed, no record of what was issued is kept or needed, and a damaged code is replaced by printing the same payload again, recovered from the code, from the rendering alongside it, or by deriving it from the board once more.
