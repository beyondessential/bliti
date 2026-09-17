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
