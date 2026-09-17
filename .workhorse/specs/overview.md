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

There is no fleet key and no authoritative per-device record.
The whole chain is reproducible from the board alone, at manufacture or at any time after, so a QR code can be reproduced from the device itself rather than from a record of what was issued.
Anything a device stores about its own identity is a cache it can rebuild.

## What the guarantees are

An operator who has scanned a device's QR code can pick it out of every device advertising nearby.

A listener on the BLE link learns neither the presence secret nor the contents of a session, and cannot turn a recorded advertisement into a session.

Reading the QR code does not yield the board ID, and does not yield it even to someone who knows the derivation constants.
The board ID therefore appears in no QR payload, in no advertisement, and nowhere else reachable without access to the device.

An observer who has not scanned the QR code cannot tell which device an advertisement belongs to.

## Where the guarantees stop

These limits are properties of the design rather than gaps in it, and the system is described accurately by stating them.

Anyone who has had access to a device, or who otherwise knows its board ID, can derive its presence secret and both impersonate it and connect to it.
The same is true of anyone holding a photograph of the QR code: the code is the credential.

An observer who has not scanned the QR code can still tell that some bliti device is present, because the service UUID is advertised in the clear so that clients can filter a scan on it.
Such an observer can also tell that two advertisements come from the same device whenever the adapter's address does not rotate, which is a property of how the host is configured rather than something bliti controls.
bliti claims only that such an observer cannot tell *which* device it is hearing.

Because the derivation constants are public, finding a device requires neither its QR code nor physical access to it, only its board ID, and board IDs can be searched for rather than known.
What stands against that search is the cost of the derivation and the size of the board ID's space, both specified in [BLI-KEY](key-schedule.md).
For boards whose only identifier is a short serial number, that margin is narrow, and those boards carry a weaker guarantee than boards with a hardware-backed identifier.

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

## The base protocol version

One version number spans the whole stack, and it is the only version anything in the system acts on.
It is carried in the QR payload of [BLI-STK](sticker.md) and in the advertisement of [BLI-ADV](discovery.md), and it is the same number in both.

It covers every layer in the table above: the derivations of [BLI-KEY](key-schedule.md), the handshake, transport and streams of [BLI-CHN](channel.md), and the fact that application messages are JSON in the envelope of [BLI-MSG](messages.md).

A client reads the advertised marker before it recomputes a handle.
Where it does not implement that version it reports a device present at a version it does not support, and goes no further, because no shared secret could be computed and nothing it said afterwards would be understood.

Above that version nothing is gated.
The software each end runs carries its own version, which is exchanged and displayed and never acted on, as specified in [BLI-MSG](messages.md).

## Reporting

A device reports failures and identity problems on its standard error.
These conditions leave a device unreachable over the channel, so they surface where the device is rather than to a client.
