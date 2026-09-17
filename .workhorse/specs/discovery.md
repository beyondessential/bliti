---
id: ADV
---

# Discovery and matching

A device advertises continuously, and a client that holds a QR code recomputes the expected [advertised handle](overview.md#advertised-handle) from it and matches that against what it hears.

## Borrowed terms

| term | meaning |
| --- | --- |
| advertisement | The packet a BLE peripheral broadcasts so that scanners learn it is there. |
| scan response | A second packet a peripheral sends only when a scanner asks for one. |
| AD type | One element of an advertisement or scan response, each defined in Part A of the [Core Specification Supplement](https://www.bluetooth.com/specifications/specs/core-specification-supplement/). |
| flags, local name, service UUID, service data | Four such AD types: the mandatory discoverability bits, the name a device calls itself, the services it offers, and arbitrary bytes keyed by one of those service UUIDs. |
| legacy advertising | Advertising as it was before extended advertising, offering 31 bytes for the advertisement and 31 for the scan response. |

## What is advertised

A device MUST advertise a 128-bit service UUID identifying it as speaking bliti, in the advertisement rather than in the scan response.

A device MUST carry its payload in the local name.

The payload MUST be 13 bytes: the eight-byte handle, the four-byte [rotation salt](overview.md#rotation-salt), then the one-byte [version marker](overview.md#version-marker), rendered as 21 characters of base32 without padding.

> [!NOTE]
> A scan filter is applied to advertisement data and never to the scan response, so a service UUID a client filters on has to sit in the advertisement.
> The payload rides in the local name because a host, not a device, decides which element goes in which packet. On a legacy controller the mandatory flags take three bytes and a 128-bit service UUID eighteen, leaving ten of the advertisement's 31, while service data keyed by that same UUID would need 31 of its own before it fit anywhere. A local name costs two bytes of element header rather than eighteen of repeated UUID, and is the one element a host will place in the scan response.
> Eight bytes of handle makes a collision between two devices at one site implausible. The handle is not secret, so carrying it in the clear costs nothing, and a client that can only filter by name prefix has the rendering to filter on.

## Matching

A client MUST read the version marker before recomputing a handle, as [VER](version.md) requires.

A client MUST recompute the handle from the QR code it holds together with the salt it observes, and compare that against what it read.

A client MUST match on the payload rather than on the peer's address.

A local name that is not a bliti payload MUST be passed over.

> [!NOTE]
> Matching on the payload means a client that is never shown the peer's address can still identify a device, and a device whose address rotates is still recognised.
> The cost to a client is one fast hash per advertisement heard, per QR code held.

## Rotation

A device SHOULD change its rotation salt every fifteen minutes, and MUST re-register its advertisement when it does.

A client MUST recompute against whatever salt it observes.

> [!NOTE]
> Were the salt fixed, a passive observer could follow a device by its handle alone, even though the handle reveals nothing about which device it is.
> Because a client recomputes against what it observes, nothing a client does depends on the rotation period.

## Advertising continuously

A device MUST advertise whenever it is running.

A device MUST NOT lock a peer out after failed handshakes.

A device MUST bound what it records about failed attempts.

> [!NOTE]
> Anyone in range can open a connection and begin a handshake that will fail. A lockout would let someone in range deny an operator their own device, which is worse than the attempts it would prevent, and unbounded recording would let them exhaust the device's storage instead.

## Address privacy

The privacy of the BLE address is a property of how the adapter is configured, which bliti neither sets nor depends on.

What an observer learns either way is specified in [SEC](security.md).
