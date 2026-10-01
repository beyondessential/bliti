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

The application MUST treat a payload identically however it arrived, including one it held from before, as [Remembering devices](#remembering-devices) and [After a reload](#after-a-reload) have it.

The application MUST report a payload it cannot parse and a payload at an unsupported version as the distinct conditions they are.

Once it has taken the fragment, whether or not it could parse it, the application MUST remove the fragment from its address.

> [!NOTE]
> The link is the path for a device scanned with a generic phone camera, by an operator with nothing installed. The camera is the path for provisioning several devices in one session, where returning through the link each time would mean leaving and re-entering the application.
> Removing the fragment lets a reload come back to the payload the page held rather than to the link it was opened with, and keeps the token out of an address copied from the page.

## Exporting the QR code

The application MUST offer a QR code it has read for download as the SVG image of [QR](qr-code.md).

The downloaded file MUST be named after the last group of the human-readable rendering, as `bliti-` followed by that group and `.svg`.

> [!NOTE]
> This is how a scuffed code is reprinted with only a phone to hand: the payload is read from the remains of the code, or typed from the rendering beside it, and printed again.
> The last group of the rendering falls wholly within the device static public key, so the name gives away nothing secret, and it matches the end of the rendering printed beside the code.

## Finding the device

The application MUST compute the local name of [ADV](discovery.md) that the device whose QR code it has read advertises, at the version marked on that QR code.

Before offering the chooser, the application MUST show the operator that name, and that it should be the only device listed.

Where the browser offers a chooser rather than the advertisements themselves, the application MUST filter that chooser by that exact local name alone, and MUST ask for access to the service of [ADV](discovery.md) alongside the filter.

The application MUST match the device picked against the QR code as [ADV](discovery.md) specifies before it sends that device anything.

Where the chooser closes without a device picked, the application MUST tell the operator that, if their device was not listed, it may be off or out of range, and MUST let them open the chooser again.

> [!NOTE]
> Filtering the chooser puts the device whose QR code was read in front of the operator, rather than every bliti device in range, and showing the name first tells them which device to expect there.
> A chooser matches a filter against the host's record of a device, not against what the device advertises. A host that has once resolved a device's services reports those in place of the services it hears advertised, so a record from a time the device was not offering its service would hide it from a chooser filtered on that service, and only a connection could correct the record. The name alone already narrows the chooser to the one device.
> The browser does not say why a chooser closed with nothing picked, so the application cannot tell an empty list from an operator who dismissed it.

## Remembering devices

Once a channel to a device has opened, the application MUST remember that device, by keeping its payload together with the `hostname` it last reported, as [NFO](device-info.md) has it.

The application MUST keep the devices it remembers for the life of the tab, so that a reload or a restored tab keeps them and closing the tab forgets them.

The application MUST remember at most three devices, forgetting the one whose channel opened least recently to make room for another.

The application MUST forget a remembered device whose payload it can no longer read, as after an update that drops the payload's version.

Where it holds no payload, the application MUST list the devices it remembers beneath the means of reading a QR code, the one whose channel opened most recently first.
Each MUST be listed by its hostname and the last group of the human-readable rendering of [QR](qr-code.md), or by that group alone where the device has reported no hostname.

Choosing a remembered device MUST hold its payload as though it had just been read, and the application MUST then find the device as [Finding the device](#finding-the-device) specifies, the operator opening the chooser as for any other payload.

Wherever the application shows a payload it holds that is a remembered device's, it MUST show that device's hostname with it.

> [!NOTE]
> A device is remembered only once a channel to it has opened, so a mistyped code, or one for a device never reached, stays off the list.
> The chooser is opened by the operator every time, because a browser offers it only in answer to a gesture. Remembering a device saves reading its code again, not picking it in the chooser.

## After a reload

Where the page is reloaded while a channel is open, or while the device carries out an act as [When the device goes away](#when-the-device-goes-away) has it, the application MUST come back holding that device's payload, ready for the operator to find it again.

Where the page is reloaded otherwise, or where it can no longer read that device's payload, the application MUST come back holding no payload.

> [!NOTE]
> A reload ends the channel, and any attempt to reach a device coming back ends with it. The payload is kept, so finding the device again is one pick in the chooser.

## Opening the channel

The application MUST run the handshake of [CHN](channel.md) with the presence token read from the payload.

The application MUST NOT run the memory-hard derivation of [KEY](key-schedule.md).

> [!NOTE]
> A client reads the token from the payload rather than deriving it, so nothing in a client needs the argon2id parameters or the memory they ask for. The handle a client does compute is a fast hash.

## When the device goes away

The application MUST treat `accepted` for an act it asked for as it treats `going-away` for that act, as [CTL](control/power.md) specifies both.

On either, the application MUST say that the act is under way, as restarting bliti, rebooting or shutting down, and MUST keep the QR code it read.
On a `going-away` whose cause is `low-battery`, the application MUST also say that the device is shutting down because its battery is low.
While it says so, the application MUST keep the device view's title and the header naming the device, as [VIEW](device-view.md) has them, MUST show the act under way in place of the tiles, and MUST offer to disconnect beside the title.
The application MUST NOT say that an act has completed.

After a `restart` or a `reboot`, the application MUST reconnect to the same device on its own once the channel has closed, without offering the chooser, and MUST go on trying until the channel is open again.
While it tries, the application MUST go on saying that the act is under way.
Disconnecting MUST stop it trying.
Once the channel is open again, the application MUST stop saying so and show the device view.
The application MUST stop trying once a device that has not come back is unlikely to, and MUST then say that the device has not come back and offer to find it again.
Where the browser cannot reach the device again without the chooser, the application MUST offer to find it again.

After a `power-off`, the application MUST go on saying that the act is under way for a moment once the channel has closed, long enough to be read, unless the operator disconnects first, and MUST then stop saying so and offer to find the device again, as after any closed channel.
The application MUST NOT reconnect on its own after a `power-off`.

> [!NOTE]
> The browser keeps the device the operator picked, so reaching it again needs no chooser and no tap, provided it comes back under the same Bluetooth address. One that comes back under another is a device the browser has not been given, and only the chooser can give it.
> An act accepted is not an act done: the device may fail to carry it out after it has said it is going, and nothing is left connected to say so. Saying only that the act is under way stays true either way.
> A client told of a restart or reboot reconnects whether or not it asked for the act, so every operator watching a device is watching it again once it is back.

## Installation and offline use

The application MUST be served from the origin the QR code encodes, as [QR](qr-code.md) specifies.

The application MUST run without being installed first.

Once loaded, the application MUST remain usable offline for as long as the page stays open.

The application MUST open offline once it is ready offline, holding everything it needs to load with no connection.

Until it is ready offline, the application MUST say that it is not, beside its title on the screen for reading a QR code, and MUST stop saying so once it is.

Where it could not become ready offline, the application MUST try again once the browser has a connection.

The application MUST also be installable, such that a browser offers to add it to the device's home screen.

> [!NOTE]
> Running uninstalled lets anyone standing in front of a device provision it.
> Working offline makes a phone that has opened the application before useful at a site with no connectivity, and it costs nothing, because the only transport to a device is the BLE channel of [CHN](channel.md).
> A browser offers to install the application before it is ready offline, so an operator who installs it and leaves coverage at once has an application that will not open until it is back. Saying so while it is not ready is what tells them to wait.

## Delivery

The bundle MUST be built with a gzip, a brotli, and a zstd encoding beside each file those encodings make meaningfully smaller.

The origin MUST serve whichever of those encodings the browser accepts, and MUST serve the file itself where the browser accepts none.

> [!NOTE]
> The bundle is built once and downloaded by every phone that provisions a device, often on the connection a site has rather than one it would choose. Encoding at build time affords settings far too slow to run per request, and leaves the origin nothing to do but choose between them.
> All three are written because the choice belongs to the browser asking: brotli is the smallest of them on this bundle, zstd decodes fastest, and gzip is understood by everything.
> The wasm module is the bulk of the download, so it is the file this matters most for.

## Secure context

The application MUST be served from an origin that constitutes a secure context.

> [!NOTE]
> Neither the camera nor Bluetooth is available to a page without one.
