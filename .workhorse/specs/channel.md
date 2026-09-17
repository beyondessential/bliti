---
id: BLI-CHN
---

# Authenticated channel

Once a client has matched a device by the handle in [BLI-ADV](discovery.md), the two authenticate to each other and open a channel carrying application messages.

Everything this spec states about the wire is contract.
An independently written client that follows it interoperates with a device that follows it.

## Authentication

Client and device MUST run a Noise `NNpsk0` handshake, with the client as initiator and the device as responder.
The Noise protocol name MUST be `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`.
The pre-shared key MUST be the 32-byte sticker secret of [BLI-KEY](key-schedule.md), at PSK position zero, used exactly as [BLI-KEY](key-schedule.md) produces it with no further derivation.

The handshake gives the session forward secrecy.

> **Note.** Both ends bring only ephemeral keys, so completing the handshake proves in both directions that each end holds the sticker secret, which is what "this is the device whose sticker I scanned" and "you scanned my sticker" both reduce to.
> A replayed or spoofed advertisement yields no session, because an attacker cannot complete the handshake behind it.
> Forward secrecy means that recovering a sticker secret later does not decrypt a recorded session, though an eavesdropper who recorded a handshake can attempt the same offline search against the transcript as against an advertisement, at the derivation cost of [BLI-KEY](key-schedule.md).
> The board ID is not verified directly and cannot be, because it is absent from the QR payload and the derivation does not run backwards; possession of the sticker secret is the proof, and is equivalent, because deriving the secret requires the board ID.

## Send rate

A device MUST NOT send more than 100 KiB, nor more than 200 notifications, in any one-second window.
The two ceilings are independent, and a device MUST satisfy both.

> **Note.** The link is shared with everything else the session is doing, including the client's own writes.
> A device with a backlog clears it more slowly rather than taking the connection down: a slow reading beats a dropped session.

## Transport

The channel MUST run over GATT, under the service UUID of [BLI-ADV](discovery.md), using two characteristics:

| characteristic | UUID | direction |
| --- | --- | --- |
| client transmit | `973bed6f-f4f9-4cae-b237-1b51701a77f5` | written by the client, carrying bytes to the device |
| device transmit | `a7aabad6-3fc2-4c9b-953b-03a70a193ec4` | notified on by the device, carrying bytes to the client |

Each direction is a stream of bytes.
The sender MUST chunk it into writes or notifications no larger than the negotiated attribute size.
The receiver MUST concatenate what arrives in the order it arrives and read messages out of the result; a chunk boundary is not a message boundary.

Within that byte stream, each Noise message, handshake or transport, MUST be prefixed with its length as four bytes, big-endian, giving the number of bytes that follow.
A Noise message is at most 65535 bytes, including its 16-byte authentication tag.
A receiver MUST reject a length prefix claiming more than 65535 bytes rather than buffering against it.

> **Note.** The length prefix is what lets a message exceed the negotiated attribute size.

## The device is a peripheral only

The device MUST act only as a GATT server, and MUST NOT act as a GATT client against the client that connects to it.
The device MUST NOT initiate pairing, and the channel MUST NOT depend on the link being encrypted or the peer being bonded.
Where the host's Bluetooth stack would resolve the connecting client's attributes or initiate pairing by default, it MUST be configured not to, as a prerequisite for running a device.

> **Note.** No client this protocol serves can pair: a browser cannot drive pairing at all, and the sticker stands in for it.
> A stack that pairs meets a client attribute whose read requires an encrypted link, asks to pair to satisfy it, and drops the link partway through a session that was otherwise working.
> All authentication and secrecy comes from the handshake above.

## Streams

Above the handshake, the encrypted byte stream MUST carry yamux, with the client as the yamux client and the device as the yamux server.
Both ends MUST use yamux's default settings, including its 256 KiB initial receive window.
Closing one stream MUST leave the other streams and the connection alive.

> **Note.** Making the client the yamux client puts the two ends' stream identifiers in disjoint spaces, so they cannot collide.

## Messages

Application messages are JSON, carried on the streams above.
How a message is delimited and encoded, which streams carry which messages, how the two ends name themselves, how each skips what it does not recognise, and how live data is subscribed to, are specified in [BLI-MSG](messages.md).
