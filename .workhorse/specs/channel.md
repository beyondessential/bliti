---
id: CHN
---

# Authenticated channel

Once a client has matched a device by its [advertised handle](overview.md#advertised-handle), as specified in [ADV](discovery.md), the two authenticate to each other and open a channel carrying application messages.

## Borrowed terms

| term | meaning |
| --- | --- |
| characteristic | A GATT attribute holding a value that a peer can read, write, or be notified of, with the operations it permits declared alongside it. |
| notification | A transfer the GATT server initiates to send a characteristic value to a client, which the client does not acknowledge. The acknowledged counterpart is an indication. |
| write, write without response | The two ATT operations by which a client sends a characteristic value. The device acknowledges a write; it does not acknowledge a write without response. |
| ATT_MTU | The largest ATT protocol data unit the two ends have negotiated for a connection. The payload of one operation is three bytes smaller. |
| pairing, bonding | The Security Manager procedures that encrypt a link and store keys for reconnecting to the same peer later. |
| peripheral, central | The GAP roles: a peripheral advertises and accepts connections, a central scans and initiates them. These are separate from the GATT server and client roles. |

## Authentication

Client and device MUST run a Noise `NKpsk0` handshake, as specified in [The Noise Protocol Framework](https://noiseprotocol.org/noise.html) revision 34, with the client as initiator and the device as responder.

The Noise protocol name MUST be `Noise_NKpsk0_25519_ChaChaPoly_BLAKE2s`.

The responder's static key, which `NKpsk0` requires the initiator to know in advance, MUST be the device static key of [KEY](key-schedule.md). The client MUST take its public half from the QR code, and the device MUST hold the private half.

The pre-shared key MUST be the 32-byte [presence token](overview.md#presence-token), at PSK position zero, used exactly as [KEY](key-schedule.md) produces it with no further derivation.

The handshake gives the session forward secrecy.

The security properties this upholds, and their limits, are specified in [SEC](security.md).

> [!NOTE]
> The two credentials authenticate different things. The static key proves the device holds something derived from its own board ID, which nothing in the QR code yields. The pre-shared key proves the client read that device's code.
> `NKpsk0` encrypts the client's first message to the device's static key, so a party without the private half cannot read it at all, rather than merely failing to prove itself.

## Send rate

A device MUST NOT send more than 200 notifications in any one-second window.

> [!NOTE]
> The ceiling counts notifications rather than bytes because it is the count that overruns a controller's buffers, and overrunning them takes the connection down rather than slowing it.
> A device with a backlog therefore clears it more slowly, because a slow reading beats a dropped session.

## Transport

The channel MUST run over GATT, under the service UUID of [ADV](discovery.md), using two characteristics:

| characteristic | UUID | direction |
| --- | --- | --- |
| client transmit | `973bed6f-f4f9-4cae-b237-1b51701a77f5` | written by the client, carrying bytes to the device |
| device transmit | `a7aabad6-3fc2-4c9b-953b-03a70a193ec4` | notified on by the device, carrying bytes to the client |

GATT is specified in Volume 3, Part G of the [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification-6-3/), and the Attribute Protocol beneath it, including the ATT_MTU negotiation referred to below, in Volume 3, Part F.
The device MUST accept both a write with response and a write without response on the client transmit characteristic, and a client MAY use either.

Each direction is a stream of bytes.
The sender MUST chunk it into writes or notifications whose payload is at most the negotiated ATT_MTU less the three-byte ATT header.
The receiver MUST concatenate what arrives in the order it arrives and read messages out of the result; a chunk boundary is not a message boundary.

Within that byte stream, each Noise message, handshake or transport, MUST be prefixed with its length as two bytes, big-endian, giving the number of bytes that follow.
A Noise message is at most 65535 bytes, including its 16-byte authentication tag.

> [!NOTE]
> A two-byte prefix expresses exactly the range a Noise message can occupy, so a receiver cannot be asked to buffer more than the maximum and needs no rule refusing one.
> The prefix is also what lets a message exceed the negotiated ATT_MTU.

## The device is a peripheral only

The device MUST act only as a GATT server, and MUST NOT act as a GATT client against the client that connects to it.
The device MUST NOT initiate pairing, and the channel MUST NOT depend on the link being encrypted or the peer being bonded.
Where the host's Bluetooth stack would resolve the connecting client's attributes or initiate pairing by default, it MUST be configured not to, as a prerequisite for running a device.

> [!NOTE]
> All authentication and secrecy come from the handshake above, so an encrypted link and a stored bond add nothing the protocol relies on.
> A stack that resolves the peer's attributes can meet one whose read requires an encrypted link, ask to pair to satisfy it, and drop the link partway through a session that was otherwise working.

## Compression

Above the handshake, the encrypted byte stream MUST carry one zlib stream of [RFC 1950](https://www.rfc-editor.org/rfc/rfc1950) in each direction.

Each direction's compression context MUST be established with the connection and MUST last as long as it.
A context MUST NOT be reset, and MUST NOT be established per stream or per message.

A context MUST NOT use a preset dictionary.

A sender MUST NOT leave a message it has finished writing unreadable by the receiver.
A sender MAY defer flushing while it has more to write.

A receiver that cannot decompress what arrives MUST treat it as a fault in the peer, MUST report it, and MUST close the connection.

> [!NOTE]
> Compression is unconditional, so there is nothing to negotiate, nothing to carry in a hello, and no uncompressed path. The marker of [VER](version.md) covers this section, and both ends are at the same marker before a channel exists.
> One context per direction, rather than one per stream or one per message, is what sees the redundancy that lives across messages rather than within one: every sample a device sends repeats the labels, units and state strings of the one before it. What it costs is that a receiver cannot skip an unknown message without decompressing it, since the context must stay fed, and it has to decompress to learn the type in any case.
> Flushing at each message boundary satisfies the rule above, and deferring is what lets a burst compress as one run.
> The context is shared by every stream and is unrecoverable once it has diverged, which is why a decompression failure ends the connection rather than the one stream, unlike the message faults of [MSG](messages.md). A conforming peer cannot produce one: the compressed bytes sit inside the Noise transport, so neither corruption on the link nor an observer can reach them.

## Streams

Above the compression, the byte stream MUST carry [yamux](https://github.com/hashicorp/yamux/blob/master/spec.md), with the client as the yamux client and the device as the yamux server.
Closing one stream MUST leave the other streams and the connection alive.

> [!NOTE]
> Making the client the yamux client puts the two ends' stream identifiers in disjoint spaces, so they cannot collide.

## Messages

The streams above carry application messages, as specified in [MSG](messages.md).
