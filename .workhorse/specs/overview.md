---
id: BLI
---

# bliti device provisioning

bliti provisions headless devices over Bluetooth Low Energy, anchored to a QR code carried on the outside of the device.
A device advertises an opaque handle, and a client that has scanned that device's QR code, and only such a client, can recognise it among the advertisements it hears, authenticate to it, and open a two-way channel.

The QR code stands in for the button press or on-screen code that other provisioning protocols use to establish that the operator is physically present, because the devices bliti targets have neither a button nor a screen.
bliti is not an implementation of Improv Wi-Fi and does not interoperate with it.

bliti ships as its own daemon and its own QR code generator, separate from the other tools in this repository.

## Requirements notation

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT, RECOMMENDED, NOT RECOMMENDED, MAY and OPTIONAL in bliti's specifications are to be interpreted as described in BCP 14 ([RFC 2119](https://www.rfc-editor.org/rfc/rfc2119), [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174)) when, and only when, they appear in all capitals, as here.

## External documents

The specifications bliti builds on, each cited where it bears on a requirement.

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

## The chain

Every value in the system descends from an identifier the board's own firmware provides, under constants that are public.
The board ID is read from firmware as specified in [BLI-BID](board-id.md), and both derivations that follow from it are specified in [BLI-KEY](key-schedule.md).
The QR code that carries the presence secret to an operator is specified in [BLI-STK](sticker.md).

A client scans the QR code, recomputes the handle, and matches it against what it hears, as specified in [BLI-ADV](discovery.md).
Client and device then authenticate to each other and open a channel, as specified in [BLI-CHN](channel.md), and exchange application messages within the envelope specified in [BLI-MSG](messages.md).
The client that does this in a browser is specified in [BLI-WEB](web-app.md).
What a device reports about itself within that envelope, and what an application does with it, is specified in [BLI-SYS](system-info.md).
What the chain guarantees, and where those guarantees stop, are specified in [SEC](security.md).
The version every value and every layer is fixed under is specified in [VER](version.md).
How a device reports what it cannot send over the channel is specified in [DEV](device.md).

There is no fleet key and no authoritative per-device record.
The whole chain is reproducible from the board alone, at manufacture or at any time after, so a QR code can be reproduced from the device itself rather than from a record of what was issued.
Anything a device stores about its own identity is a cache it can rebuild.

## How the layers sit

Each layer depends only on the one beneath it carrying bytes reliably and in order.

| layer | what it provides |
| --- | --- |
| BLE GATT | reliable, ordered bytes |
| framing | message boundaries across the negotiated attribute size |
| Noise `NNpsk0` | mutual authentication, encryption, a session key |
| yamux | either end opens streams without coordinating identifiers |
| JSON | application messages |
| message envelope | length-delimited JSON: naming, skipping unknowns, subscription streams |

Replacing the bottom layer with another BLE transport changes nothing above it, and the choice can differ per client while the layers above stay identical.

