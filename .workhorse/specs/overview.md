---
id: BLI
---

# bliti device provisioning

bliti provisions headless devices over Bluetooth Low Energy, anchored to a QR code carried on the outside of the device.

## Requirements notation

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, NOT RECOMMENDED, MAY and OPTIONAL in bliti's specifications are to be interpreted as described in BCP 14 ([RFC 2119](https://www.rfc-editor.org/rfc/rfc2119), [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174)) when, and only when, they appear in all capitals, as here.

## External documents

| document | what it covers |
| --- | --- |
| [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174) | the requirement keywords above |
| [The Noise Protocol Framework](https://noiseprotocol.org/noise.html), revision 34 | the handshake of [CHN](channel.md) |
| [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification-6-3/) | GATT and the Attribute Protocol, Volume 3 Parts G and F, under [CHN](channel.md) |
| [yamux](https://github.com/hashicorp/yamux/blob/master/spec.md) | the streams of [CHN](channel.md) |
| [Core Specification Supplement](https://www.bluetooth.com/specifications/specs/core-specification-supplement/) | the advertising data types of [ADV](discovery.md) |
| [RFC 9106](https://www.rfc-editor.org/rfc/rfc9106) | the argon2id derivation of [KEY](key-schedule.md) |
| [the BLAKE3 specification](https://github.com/BLAKE3-team/BLAKE3-specs) | the keyed hash of [KEY](key-schedule.md) |
| [RFC 4648](https://www.rfc-editor.org/rfc/rfc4648) | the base32 rendering of [QR](qr-code.md) and [ADV](discovery.md) |
| [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259) | the JSON of [MSG](messages.md) |
| ISO/IEC 18004 | the QR code symbology of [QR](qr-code.md) |
| ISO 3166-1 | the country codes of the regulatory domain in [NET](network/overview.md) |
| IEEE 802.11 | the wireless networks a device joins and the access point it runs, under [WLAN](network/wireless.md) and [HOT](network/hotspot.md) |

## Terminology

### Client

The software an operator uses to provision a device.

### Device

The headless machine bliti provisions.

### Operator

The person physically at a device, using a client to reach it.

### Generator

The tool that turns a board ID into a QR code for printing.
Defined in [QR](qr-code.md).

### Board ID

The identifier a board's own firmware provides.
Defined in [BID](board-id.md).

### Presence token

The credential carried in a device's QR code.
Defined in [KEY](key-schedule.md).

### Advertised handle

The value a device broadcasts.
Defined in [KEY](key-schedule.md).

### Rotation salt

The short random value a device advertises alongside its handle.
Defined in [ADV](discovery.md).

### Version marker

The number identifying the version of the protocol a device speaks.
Defined in [VER](version.md).

### Configuration document

The declarative description of a device's network that a client reads and writes.
Defined in [NET](network/overview.md).

### Candidate

One way a device might attach to a network.
Defined in [LINK](network/attachment.md).
