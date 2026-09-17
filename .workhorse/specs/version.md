---
id: VER
---

# Base protocol version

A client and a device MUST NOT act on any version other than the [version marker](overview.md#version-marker).

## Where the marker is carried

The marker MUST be carried in the QR payload of [QR](qr-code.md) and in the advertisement of [ADV](discovery.md), and MUST be the same number in both.

## What the marker covers

The marker covers everything two ends must agree on before or during a session:

- the derivation constants, argon2id parameters, source precedence, input encoding, pinned Endorsement Key template and handle length of [KEY](key-schedule.md)
- the handshake, framing, transport and streams of [CHN](channel.md)
- the message encoding and envelope of [MSG](messages.md)

A change to any of these is a new version.
A change that leaves all of them identical MUST NOT move the marker.

The URL the QR code is carried in, and the human-readable rendering printed beneath it, are carriers rather than payload, and changing either MUST NOT move the marker.

> [!NOTE]
> Moving the marker orphans every QR code already fixed to an enclosure, so it moves only when something it covers has actually changed.
> The marker is the only version signal that exists before a connection does, which is why it covers the layers above the key schedule as well as the key schedule itself. Were it to cover only what changes the secret, two peers running incompatible handshakes would derive matching handles, recognise each other, and fail with nothing to tell an operator.

## Acting on the marker

A client MUST read the advertised marker before it recomputes a handle.

Where the client does not implement the advertised version it MUST report a device present at a version it does not support, and MUST go no further.

> [!NOTE]
> No shared secret could be computed under a version the client does not implement, and nothing it said afterwards would be understood.
> No two versions produce a matching handle, so reading the marker is what separates an unsupported device from one the client cannot hear at all.

## Nothing else gates behaviour

A client and a device MUST NOT withhold or refuse any message type, member or feature on the grounds of what software the other end reported running.

The software each end runs carries its own version, exchanged and displayed under [MSG](messages.md).

## Supporting more than one version

A client holds a QR code, reads the version marked on it, and derives once under that version.

A device holds no QR code and cannot know which version was printed for it.
A device that supports more than one version MUST derive under each, and MUST advertise one version at a time.
