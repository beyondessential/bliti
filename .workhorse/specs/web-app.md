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

## Finding the device

The application MUST scan for the service UUID of [ADV](discovery.md), and MUST match on the recomputed handle as [ADV](discovery.md) specifies.

Where the browser offers a chooser rather than the advertisements themselves, the application MUST filter that chooser by the local name.

> [!NOTE]
> Filtering the chooser is what puts the device whose QR code was read in front of the operator, rather than every bliti device in range.

## Opening the channel

The application MUST run the handshake of [CHN](channel.md) with the presence token read from the payload.

The application MUST NOT run the memory-hard derivation of [KEY](key-schedule.md).

> [!NOTE]
> A client reads the token from the payload rather than deriving it, so nothing in a client needs the argon2id parameters or the memory they ask for. The handle a client does compute is a fast hash.

## Installation and offline use

The application MUST be served from the origin the QR code encodes, as [QR](qr-code.md) specifies.

The application MUST run without being installed first, and MUST remain usable offline once it has been loaded.

The application MUST also be installable, such that a browser offers to add it to the device's home screen.

> [!NOTE]
> Running uninstalled is what lets whoever is standing in front of a device provision it.
> Working offline is what makes a phone that has opened the application before useful at a site with no connectivity, and it costs nothing, because the only transport to a device is the BLE channel of [CHN](channel.md).

### Install criteria

A browser decides whether to offer the install itself, against criteria it does not publish as a contract, so the following are what the application declares in order to meet them.

The application MUST link a manifest declaring a name, a start URL within the served origin, a display mode of `fullscreen`, `standalone` or `minimal-ui`, and icons.

The declared icons MUST include a square PNG of at least 192 pixels and a square PNG of at least 512 pixels.

The application MUST register a service worker that serves the application's own assets.

The application MUST NOT declare a preference for a related native application.

> [!NOTE]
> The two icon sizes are the criterion most easily left unmet, because a manifest without them is still valid and still links, and the application still runs: the only symptom is that the browser never offers the install.

### Icons

Every asset the manifest references MUST remain available offline once the application has been loaded.

The application MUST declare a maskable icon, whose mark stays clear of the region a platform crops when it masks an icon to a shape of its own.

The application MUST declare a home-screen icon in its markup, for platforms that take one from there rather than from the manifest.

> [!NOTE]
> A platform that masks an icon crops whatever it is given, so a mark drawn to the edge of the tile loses its edges; a platform that ignores the manifest composites a transparent icon onto black.

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
