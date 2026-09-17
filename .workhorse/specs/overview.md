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
| [The Noise Protocol Framework](https://noiseprotocol.org/noise.html), revision 34 | the handshake of [BLI-CHN](channel.md) |
| [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification-6-3/) | GATT and the Attribute Protocol, Volume 3 Parts G and F, under [BLI-CHN](channel.md) |
| [yamux](https://github.com/hashicorp/yamux/blob/master/spec.md) | the streams of [BLI-CHN](channel.md) |

## Terminology

### Board ID

The identifier a board's own firmware provides.
Defined in [BLI-BID](board-id.md).

### Presence secret

The secret derived from the board ID and encoded in the device's QR code.
Defined in [BLI-KEY](key-schedule.md).

### Advertised handle

The value a device broadcasts so that a client holding its presence secret can recognise it.
Defined in [BLI-KEY](key-schedule.md).

### QR code

The machine-readable code carried on the outside of a device, encoding the presence secret and the version marker.
Defined in [BLI-STK](sticker.md).

### Version marker

The number identifying the version of the protocol a device speaks.
Defined in [VER](version.md).
