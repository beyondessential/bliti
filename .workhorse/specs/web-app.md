---
id: WEB
---

# Web application

The web application is the client for [BLI](overview.md): it reads a QR code, finds the device the code belongs to, and opens a channel to it.
It runs in a browser without being installed first, which is what lets a device be provisioned by whoever is standing in front of it.

## Reading a QR code

A QR code reaches the application by two paths.

Following the link opens the application with the payload already in the fragment.
This is the path for a device scanned with a generic phone camera, by an operator with nothing installed.

Capturing the code with the camera reads it into an application that is already open.
This is the path for provisioning several devices in one session, where returning through the link for each one would mean leaving and re-entering the application every time.

Both paths yield the same payload, and the application treats it identically once read.

The fragment is read in the browser and is not sent to a server.

A payload the application cannot parse, and one carrying a version the application does not support, are each reported as what they are.

## Finding the device

The application scans for the service UUID of [ADV](discovery.md), and matches the handle it recomputes from the QR code against the advertisements it hears.

Where the browser offers a chooser rather than the advertisements themselves, the application filters that chooser by the local name, so that the device whose QR code was read is the one presented.

## Opening the channel

The application runs the handshake of [CHN](channel.md) with the presence token, and carries messages over the channel that handshake establishes.

The application computes the handle, which is a fast hash, and does not run the memory-hard derivation of [KEY](key-schedule.md).
The presence token is read from the payload rather than derived, so nothing in the client needs the argon2id parameters or the memory they ask for.

Within the channel the application exchanges messages under the envelope of [MSG](messages.md): it names itself to the device and displays the version the device reports, subscribes to live data only while the operator is looking, and skips anything it does not recognise.

## Installable and available offline

The application is served from a hosted origin, which is the one the QR code encodes, as specified in [QR](qr-code.md).
It needs no installation to run, so a device can be provisioned by whoever is standing in front of it.

The application can also be installed, and once it has been loaded it works offline, so a phone that has opened it before is useful at a site with no connectivity.
The only transport to a device is the BLE channel of [CHN](channel.md); there is no device-side server and no second path, so a client reaches a device the same way whether it was installed or freshly loaded.

A cached or installed application is itself a source of the version skew [MSG](messages.md) absorbs, an older client meeting a device that has since been updated.

## Secure context

The application requires a secure context, because neither the camera nor Bluetooth is available without one.
An `https://` origin satisfies this, as does `http://localhost` when the application is served locally.

## Where a feature's client half is specified

A spec for a feature carried over the channel describes what the application does with that feature under a heading of its own, rather than in this spec.
This spec covers reading a QR code, finding a device, and opening a channel, which every feature shares.
