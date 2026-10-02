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

Client and device MUST run a Noise `NXpsk0` handshake, as specified in [The Noise Protocol Framework](https://noiseprotocol.org/noise.html) revision 34, with the client as initiator and the device as responder.

The Noise protocol name MUST be `Noise_NXpsk0_25519_ChaChaPoly_BLAKE2s`.

The responder's static key MUST be the device static key of [KEY](key-schedule.md), which the device holds and sends in the handshake.

The pre-shared key MUST be the pre-shared key of [KEY](key-schedule.md), at PSK position zero.

The first message MUST carry an empty payload.
The second message MUST carry the KEM key digest of [KEY](key-schedule.md) as its payload, 32 bytes.

Having read the second message, the client MUST compute the key fingerprint of [KEY](key-schedule.md) from the static key and the digest the device sent, and MUST compare it against the key fingerprint in the QR code.
Where the two differ, the client MUST fail the handshake, and MUST NOT send anything further.
The client MUST report that failure as it reports any other failed handshake.

The handshake gives the session forward secrecy.

The security properties this upholds, and their limits, are specified in [SEC](security.md).

> [!NOTE]
> The two credentials authenticate different things. The static key proves the device holds something derived from its own board ID, which nothing in the QR code yields. The pre-shared key proves the client read that device's code.
> With the pre-shared key at position zero, a client without it fails at the first message, so the device runs no Diffie-Hellman for it and never sends it the static key.
> The static key and the digest travel encrypted, so a listener without the pre-shared key sees neither.

## Send rate

A device MUST NOT put more than 40 KiB of notification payload on the air in any one-second window, across all its channels.

A device that reaches the ceiling MUST hold the remainder until the window allows it, and MUST NOT discard it.

> [!NOTE]
> The ceiling counts payload bytes rather than notifications because the link spends air time, which the notification count does not track. A peer may coalesce many notifications into one ATT protocol data unit, so at the same count a device can occupy the air anywhere from one unit to dozens depending on how large each notification is and on whether the peer coalesces at all, neither of which the device is told. Bytes track the air time the device is asking for, across peers that differ in both.
> Past roughly 60 KiB a second a peer stops being reliable rather than merely slow: delivery becomes erratic, notifications begin to go missing, and the connection is eventually lost to a supervision timeout. Where that happens varies between connections at a fixed rate, so the ceiling sits far enough below it to stay out of that region rather than close enough to be efficient near it. Below it a link runs steadily for as long as it is asked to.
> The ceiling does not bind in ordinary operation. A peer slower than this paces the device by taking notifications more slowly, and the device clears its backlog at whatever rate that peer allows. The ceiling only matters for a peer fast enough to be driven into the unreliable region, which is the only case where sending harder ends the session instead of filling it.
> A device with a backlog therefore clears it more slowly, because a slow reading beats a dropped session.

## Transport

The channel MUST run over GATT, under the service UUID of [ADV](discovery.md), using these characteristics:

| characteristic | UUID | direction |
| --- | --- | --- |
| client transmit | `973bed6f-f4f9-4cae-b237-1b51701a77f5` | written by the client, carrying bytes to the device |
| slot allocation | `7f0c7b93-87a1-40d3-abcb-f8381c30824f` | read by the client, naming its device transmit slot |
| device transmit, slot 0 to 7 | `a7aabad6-3fc2-4c9b-953b-03a70a193e00` to `a7aabad6-3fc2-4c9b-953b-03a70a193e07` | notified on by the device, carrying bytes to the client |

GATT is specified in Volume 3, Part G of the [Bluetooth Core Specification](https://www.bluetooth.com/specifications/specs/core-specification-6-3/), and the Attribute Protocol beneath it, including the ATT_MTU negotiation referred to below, in Volume 3, Part F.
The device MUST accept both a write with response and a write without response on the client transmit characteristic, and a client MAY use either.

The device MUST offer eight device transmit characteristics, the slots, each UUID ending in its slot number.
A read of the slot allocation characteristic MUST answer one byte, the number of the slot given to the reading client, or an empty value where no slot is free.
A client MUST read the slot allocation characteristic before subscribing, and MUST subscribe only to the slot it was given.
Subscribing to its slot opens a client's channel.

Each direction is a stream of bytes.
The sender MUST chunk it into writes or notifications whose payload is at most the negotiated ATT_MTU less the three-byte ATT header.
The receiver MUST concatenate the chunks in the order they arrive and read messages out of the result; a chunk boundary is not a message boundary.

Within that byte stream, each Noise message, handshake or transport, MUST be prefixed with its length as two bytes, big-endian, giving the number of bytes that follow.
A Noise message is at most 65535 bytes, including its 16-byte authentication tag.

> [!NOTE]
> A two-byte prefix expresses exactly the range a Noise message can occupy, so a receiver cannot be asked to buffer more than the maximum and needs no rule refusing one.
> The prefix also lets a message exceed the negotiated ATT_MTU.

## Several clients at once

A device MUST serve a channel to each client connected to it, each independently of the others.
The data a client writes MUST reach only that client's channel, and the data the device sends on a channel MUST reach only that channel's client.
A client connecting, failing its handshake, or leaving MUST NOT end or disturb another client's channel.

The device MUST give each connected client a slot no other connected client holds, and MUST give a client that reads again while connected the slot it already holds.
A client MUST hold its slot for as long as it is connected.
The device MUST NOT open a channel on a slot for a client it was not given to.
A device that ends a channel while its client is still connected MUST end that connection.

A client given no slot MUST tell the operator that the device is serving as many clients as it can.

> [!NOTE]
> Up to eight operators can watch one device at once. Configuring it is exclusive, as [CFG](network/session.md) specifies.
> A slot per client is what keeps one client leaving from reaching another's channel. A host stack may end the notifications of a different subscriber to the same characteristic when a client that is not bonded disconnects, which a device cannot undo and which no characteristic shared between clients would survive.

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

A receiver that cannot decompress the bytes it receives MUST treat it as a fault in the peer and MUST close the connection.

The receiver MUST report it: a device by logging it, a client by telling the operator that the connection to the device has failed.

A receiver whose transport ends before the compressed stream it carries does MUST close the connection, and MUST NOT treat the exchange as complete.

> [!NOTE]
> Compression is unconditional, so there is nothing to negotiate, nothing to carry in a hello, and no uncompressed path. The marker of [VER](version.md) covers this section, and both ends are at the same marker before a channel exists.
> One context per direction, rather than one per stream or one per message, sees the redundancy that lives across messages rather than within one: every reading a device sends repeats the catalogue names, trait names, units and state strings of the one before it. The cost is that a receiver cannot skip an unknown message without decompressing it, since the context must stay fed, and it has to decompress to learn the type in any case.
> Flushing at each message boundary satisfies the rule above, and deferring lets a burst compress as one run.
> The context is shared by every stream and is unrecoverable once it has diverged, which is why a decompression failure ends the connection rather than the one stream, unlike the message faults of [MSG](messages.md). A conforming peer cannot produce one: the compressed bytes sit inside the Noise transport, so neither corruption on the link nor an observer can reach them.
> A transport that ends mid-stream is a different thing from one whose bytes will not decompress, and is nobody's fault: a client that walks out of range ends a connection exactly that way. It must not read as a complete exchange, since a receiver holds part of a block it can never finish.

## Streams

Above the compression, the byte stream MUST carry [yamux](https://github.com/hashicorp/yamux/blob/master/spec.md), with the client as the yamux client and the device as the yamux server.
Closing one stream MUST leave the other streams and the connection alive.

> [!NOTE]
> Making the client the yamux client puts the two ends' stream identifiers in disjoint spaces, so they cannot collide.

## Messages

The streams above carry application messages, as specified in [MSG](messages.md).

## When the channel closes

A client MUST report a channel that has closed, and SHOULD offer to open it again.

A device MUST, as it starts, end every connection made to it before it started, since it holds a channel for none of them.
It MUST offer the service before it ends them.

A device that stops MUST stop advertising and end every connection before it withdraws the service.

> [!NOTE]
> A channel closes with nothing having gone wrong, when the operator walks out of range or the device restarts, as readily as it closes on a fault.
> An operator is served by knowing the view has stopped either way, and by a way back to it short of reading the code again.
> A connection can outlive the device's own restart, and a client holding one would otherwise wait on a channel nothing answers, with no sign that it has gone.
> A client connected as the service is withdrawn is told the device's attributes have changed, and keeps a record of the device without the service. A connection left over from a device that went away without stopping has seen exactly that, and offering the service before ending it tells the client the service is back.
