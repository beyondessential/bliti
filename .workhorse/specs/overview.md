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

The identifier the board's own firmware provides, from which every other value in the system descends.
It is not a secret, and any software on the device can read it.
Which sources qualify, and in what order of precedence, are specified in [BLI-BID](board-id.md).

### Presence secret

The 32-byte secret derived from the board ID, encoded in the device's QR code, and used as the pre-shared key that authenticates a channel.
Holding it stands as proof that the holder has read the QR code on the device itself.
Its derivation is specified in [BLI-KEY](key-schedule.md).

### Advertised handle

The value derived from the presence secret and the current rotation salt, which the device broadcasts continuously.
A client holding the presence secret recomputes it to recognise the device; an observer without the secret cannot tell which device it belongs to.
Its derivation is specified in [BLI-KEY](key-schedule.md), and its use in [BLI-ADV](discovery.md).

### QR code

The machine-readable code carried on the outside of a device, encoding the presence secret and the base protocol version.
What it encodes, and how one is produced, are specified in [BLI-STK](sticker.md).

### Version marker

The number identifying the version of the protocol a device speaks, carried both in its QR code and in its advertisement.
It is the only version any part of the system acts on.
What it covers, and how each end acts on it, are specified in [VER](version.md).

