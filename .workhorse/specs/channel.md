---
id: BLI-CHN
---

# Authenticated channel

Once a client has matched a device by the handle in [BLI-ADV](discovery.md), the two authenticate to each other and open a channel carrying application messages.

Everything this spec states about the wire is contract: an independently written client that follows it interoperates with a device that follows it.

## Authentication

Client and device run a Noise `NNpsk0` handshake with the sticker secret of [BLI-KEY](key-schedule.md) as the pre-shared key.

The protocol name is `Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s`: X25519 for the ephemeral exchange, ChaCha20-Poly1305 for the cipher, and BLAKE2s for the hash.
The pre-shared key is at position zero, mixed in before the first handshake message, and is the 32-byte sticker secret used exactly as [BLI-KEY](key-schedule.md) produces it with no further derivation.
The client is the initiator and the device the responder.
`NNpsk0` is a two-message pattern: the initiator writes the first message, the responder reads it and writes the second, the initiator reads that, and both then move into transport mode.

Both ends bring only ephemeral keys, and all authentication comes from the pre-shared secret.
This proves in both directions that each end holds the sticker secret, which is what "this is the device whose sticker I scanned" and "you scanned my sticker" both reduce to.
Neither a replayed advertisement nor a spoofed one yields a session, because an attacker cannot complete the handshake behind it.

The handshake produces a fresh session key and gives the session forward secrecy, so recovering a sticker secret later does not decrypt a recorded session.

A device's board ID is not verified directly, and cannot be: it is absent from the QR payload and the derivation does not run backwards.
Possession of the sticker secret is the proof, and it is equivalent, because deriving the secret requires the board ID.

The sticker secret is a full-width value rather than a short code a person types, which is why no password-authenticated key exchange is used: the resistance of the secret to guessing comes from the derivation in [BLI-KEY](key-schedule.md).

An eavesdropper who records a handshake can attempt the same offline search against the transcript as against an advertisement, and the same derivation cost and the same limits apply.

## How fast a device may send

A device sends no more than about a hundred kibibytes, and no more than about two hundred notifications, in any second.

Both ceilings exist because the link is shared with everything else the session is doing, including the client's own messages and the notifications that carry them.
A device with a backlog takes longer to clear it rather than taking the connection down, which is the outcome worth having: a slow reading beats a dropped session.

## Transport

The channel runs over GATT, under the service UUID of [BLI-ADV](discovery.md), using two characteristics:

| characteristic | UUID | direction |
| --- | --- | --- |
| client transmit | `973bed6f-f4f9-4cae-b237-1b51701a77f5` | written by the client, carrying bytes to the device |
| device transmit | `a7aabad6-3fc2-4c9b-953b-03a70a193ec4` | notified on by the device, carrying bytes to the client |

Each direction is a stream of bytes, chunked by the sender into writes or notifications no larger than the negotiated attribute size, and reassembled by the receiver into the byte stream the sender wrote.
A chunk boundary carries no meaning: a receiver concatenates what arrives in the order it arrives and reads messages out of the result.

Within that byte stream, each Noise message, handshake or transport, is prefixed with its length as four bytes, big-endian, giving the number of bytes that follow.
This is what lets a message exceed the attribute size.
A Noise message is at most 65535 bytes including its 16-byte authentication tag, so a receiver refuses a prefix claiming more than that rather than buffering without bound.

GATT is the transport every client platform can reach, including browsers, which have no other.

## The device is a peripheral only

The device acts only as a GATT server, and never as a GATT client against the client that connects to it.

A Bluetooth stack that resolves the connecting client's attributes in turn will meet one whose read requires an encrypted link, and ask to pair in order to read it.
No client this protocol serves can pair: a browser cannot drive pairing at all, and the sticker is what stands in for it.
The pairing attempt is therefore refused, and the device drops the link partway through a session that was otherwise working.

The device likewise never initiates pairing, and the channel never depends on the link being encrypted or the peer being bonded.
All of the protocol's authentication and secrecy comes from the handshake above.

Where the Bluetooth stack does this by default, turning it off is a prerequisite for running a device, alongside the stack itself.

## Streams

Above the handshake, the encrypted byte stream carries yamux, which is what lets either end open streams without coordinating identifiers with the other end and without asking permission, and lets several be in flight at once.

The client is the yamux client and the device the yamux server, which is what puts the two ends' stream identifiers in disjoint spaces so they cannot collide.
Both ends run yamux's own defaults, including its 256 KiB initial receive window.
Flow control is carried on the wire as window updates, so neither end has to be told the other's settings, and an implementation that follows the yamux specification interoperates without further agreement.

Closing one stream leaves the other streams and the connection itself alive.

This is what lets a device send without being asked, rather than only answering requests.
State that changes while a client is connected is sent as it happens rather than waiting to be polled for.

## Messages

Application messages are JSON, carried on the streams above.

The volumes involved are small, every client platform reads JSON without a library, and a conversation can be read directly while developing.

How a message is delimited and encoded, which streams carry which messages, how the two ends name themselves, how each skips what it does not recognise, and how live data is subscribed to, are specified in [BLI-MSG](messages.md).
