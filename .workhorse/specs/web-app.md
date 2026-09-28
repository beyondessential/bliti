---
id: WEB
---

# Web application

The web application is a client that runs in a browser: it reads a QR code, finds the device the code belongs to, and opens a channel to it.

## Borrowed terms

| term | meaning |
| --- | --- |
| secure context | A browsing context a browser considers safe enough to expose powerful features to. An `https://` origin satisfies it, as does `http://localhost`. |
| chooser | The picker a browser may present in place of the advertisements themselves, from which a person selects the one device a page may talk to. |
| fragment | The part of a URL after `#`, which a browser resolves locally and does not send to a server. |

## Reading a QR code

The application MUST accept a payload by either path: following the link, which opens the application with the payload already in the fragment, or capturing the code with the camera while the application is already open.

The application MUST treat a payload identically however it arrived.

The application MUST report a payload it cannot parse and a payload at an unsupported version as the distinct conditions they are.

> [!NOTE]
> The link is the path for a device scanned with a generic phone camera, by an operator with nothing installed. The camera is the path for provisioning several devices in one session, where returning through the link each time would mean leaving and re-entering the application.

## Exporting the QR code

The application MUST offer a QR code it has read for download as the SVG image of [QR](qr-code.md).

The downloaded file MUST be named after the last group of the human-readable rendering, as `bliti-` followed by that group and `.svg`.

> [!NOTE]
> This is how a scuffed code is reprinted with only a phone to hand: the payload is read from what remains of the code, or typed from the rendering beside it, and printed again.
> The last group of the rendering falls wholly within the device static public key, so the name gives away nothing secret, and it matches the end of the rendering printed beside the code.

## Finding the device

The application MUST compute the local name of [ADV](discovery.md) that the device whose QR code it has read advertises, at the version marked on that QR code.

Before offering the chooser, the application MUST show the operator that name, and that it should be the only device listed.

Where the browser offers a chooser rather than the advertisements themselves, the application MUST filter that chooser by the service UUID of [ADV](discovery.md) and by that exact local name.

The application MUST match the device picked against the QR code as [ADV](discovery.md) specifies before it sends that device anything.

Where the chooser closes without a device picked, the application MUST tell the operator that, if their device was not listed, it may be off or out of range, and MUST let them open the chooser again.

> [!NOTE]
> Filtering the chooser is what puts the device whose QR code was read in front of the operator, rather than every bliti device in range, and showing the name first tells them what to expect there.
> The browser does not say why a chooser closed with nothing picked, so the application cannot tell an empty list from an operator who dismissed it.

## Opening the channel

The application MUST run the handshake of [CHN](channel.md) with the presence token read from the payload.

The application MUST NOT run the memory-hard derivation of [KEY](key-schedule.md).

> [!NOTE]
> A client reads the token from the payload rather than deriving it, so nothing in a client needs the argon2id parameters or the memory they ask for. The handle a client does compute is a fast hash.

## When the device goes away

The application MUST treat `accepted` for an act it asked for as it treats `going-away` for that act, as [CTL](control/overview.md) specifies both.

On either, the application MUST say which act the device is carrying out, and MUST keep the QR code it read.

After a `restart` or a `reboot`, the application MUST reconnect to the same device on its own once the channel has closed, without offering the chooser, and MUST go on trying until the channel is open again.
While it tries, the application MUST say that it is waiting for the device to come back, and MUST let the operator stop.
The application MUST stop trying once a device that has not come back is unlikely to, and MUST then say that the device has not come back and offer to find it again.
Where the browser cannot reach the device again without the chooser, the application MUST offer to find it again.
Once the channel is open again, the application MUST show the device view.

After a `power-off`, the application MUST say that the device has been turned off and is turned on at the device, MUST offer to find it again, and MUST NOT reconnect on its own.

> [!NOTE]
> The browser keeps the device the operator picked, so reaching it again needs no chooser and no tap, provided it comes back under the same Bluetooth address. One that comes back under another is a device the browser has not been given, and only the chooser can give it.
> A client told of a restart or reboot reconnects whether or not it asked for the act, so every operator watching a device is watching it again once it is back.

## Installation and offline use

The application MUST be served from the origin the QR code encodes, as [QR](qr-code.md) specifies.

The application MUST run without being installed first, and MUST remain usable offline once it has been loaded.

The application MUST also be installable, such that a browser offers to add it to the device's home screen.

> [!NOTE]
> Running uninstalled is what lets whoever is standing in front of a device provision it.
> Working offline is what makes a phone that has opened the application before useful at a site with no connectivity, and it costs nothing, because the only transport to a device is the BLE channel of [CHN](channel.md).

## Delivery

The bundle MUST be built with a gzip, a brotli, and a zstd encoding beside each file those encodings make meaningfully smaller.

The origin MUST serve whichever of those encodings the browser accepts, and MUST serve the file itself where the browser accepts none.

> [!NOTE]
> The bundle is built once and downloaded by every phone that provisions a device, often on the connection a site has rather than one it would choose. Encoding at build time affords settings far too slow to run per request, and leaves the origin nothing to do but choose between them.
> All three are written because the choice belongs to the browser asking: brotli is the smallest of them on this bundle, zstd decodes fastest, and gzip is understood by everything.
> The wasm module is the bulk of what is downloaded, so it is the file this matters most for.

## Secure context

The application MUST be served from an origin that constitutes a secure context.

> [!NOTE]
> Neither the camera nor Bluetooth is available to a page without one.
