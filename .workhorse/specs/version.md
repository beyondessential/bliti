---
id: VER
---

# Versions

Two numbers version bliti: the [payload version](overview.md#payload-version) of a QR code, and the [version marker](overview.md#version-marker) of the protocol a device speaks.
A client and a device MUST NOT act on any version other than these.

## Where each is carried

The payload version MUST be carried in the QR payload of [QR](qr-code.md).

The version marker MUST be carried in the advertisement of [ADV](discovery.md).

## What each covers

The payload version covers everything between a board ID and a QR code, and between a QR code and the values a client takes from it:

- the layout of the QR payload of [QR](qr-code.md)
- everything specified in [KEY](key-schedule.md)

The version marker covers everything two ends must agree on before or during a session:

- the payload version it reads
- the layout of the advertised payload of [ADV](discovery.md) after the marker itself
- the handshake, framing, transport, compression and streams of [CHN](channel.md)
- the message encoding and envelope of [MSG](messages.md)

Each version marker MUST read exactly one payload version.

A change to anything the payload version covers is a new payload version, and so also a new version marker.
A change to anything else the version marker covers is a new version marker reading the same payload version.
A change that leaves all of them identical MUST NOT move either number.

The text around the payload in the QR code is a carrier rather than payload, and changing it MUST NOT move either number.

> [!NOTE]
> Moving the payload version orphans every QR code already fixed to an enclosure, so it moves only when something it covers has actually changed. Moving the version marker alone orphans none: a QR code is valid under every version marker that reads its payload version.
> The version marker is the only version signal that exists before a connection does, which is why it covers the layers above the key schedule as well as the key schedule itself. Were it to cover only the inputs that change the secret, two peers running incompatible handshakes would recognise each other and fail with nothing to tell an operator.

## The versions defined

The payload of [QR](qr-code.md), under the key schedule of [KEY](key-schedule.md), MUST carry payload version 1.

The protocol these specifications describe MUST be advertised as version marker 1, which reads payload version 1.

## Acting on the versions

A client holding a QR code MUST consider every version marker it implements that reads the code's payload version.
Where there is none, it MUST go no further, and MUST report the QR code as being at a payload version it does not support.

A client MUST read the advertised marker before it compares a handle, and MUST compare a handle only for a marker it is considering.

Where the client does not implement the advertised marker it MUST go no further, and SHOULD report a device present at a version it does not support.

> [!NOTE]
> No shared secret could be computed under a version the client does not implement, and nothing it said afterwards would be understood.
> The advertised name carries the marker, so no two markers produce a matching name, and reading the marker separates an unsupported device from one the client cannot hear at all.
> The report is a SHOULD because not every client hears every advertisement. A browser's chooser filtered on the names of [WEB](web-app.md) passes over a device at any other version, so the web application never sees one to report.

## Nothing else gates behaviour

A client and a device MUST NOT withhold or refuse any message type, member or feature on the grounds of the software the other end reported running.

The software each end runs carries its own version, exchanged and displayed under [MSG](messages.md).

## Supporting more than one version

A client holds a QR code, reads the payload version marked on it, and derives once under that payload version.

A device holds no QR code and cannot know which payload version was printed for it.
A device that supports more than one payload version MUST derive under each.

A device MUST advertise one version marker at a time, and by default MUST advertise the highest version marker it supports.
