---
id: STAT
---

# Status screen

The Status screen is the screen an operator meets when a channel to a device opens: whether the things the device must be running are running, the way into the tasks an operator comes to do, and the way into everything else.

[VIEW](device-view.md) renders the whole catalogue of [NFO](device-info.md).
This screen renders the few entries that answer "is it working", drives the restarts of [SVC](control/services.md), and sends an operator to the network setup of [NSET](network/setup.md) and to [VIEW](device-view.md) for the rest.

## The landing

The application MUST show the Status screen when a channel to a device opens, and MUST return the operator to it from every screen reached from it.

The application MUST title the Status screen with the device's `hostname`, falling back to the last four characters of the encoded payload of [QR](qr-code.md) where the device reports none.

The application MUST carry beneath the title the device's software identity, from `board` with `board-revision`, `os` and `kernel`, as [VIEW](device-view.md) draws the same header.

The application MUST offer to disconnect beside the title.

> [!NOTE]
> The question an operator opens the application to answer is "is this working, and if not what do I press". The Status screen is that question answered before anything is tapped.

## What it is running

The application MUST show, beneath the title, a row for each thing that must be running, each reading at a glance as well or not well: well where the entry behind it is `passed`, and not well otherwise.

The application MUST render a row not well with the device's `reason` where the entry carries one.

The application MUST show the rows in this order:

| row | drawn from |
| --- | --- |
| whether the device is online | `network-address` and `network-configuration` |
| the overlay | the overlay client's `service` |
| Tamanu | the Tamanu services' `service` entries |
| anything else | every other `service` |

The application MUST show the device as online while it holds a `network-address` on the interface carrying the `default` route, and not online otherwise.

The application MUST say on the online row how the device is connected, by the way [NSET](network/setup.md) marks as in use: by cable, by wifi naming the network joined from `wireless-network`, or by its hotspot.
Where the hotspot is on, the application MUST also say how many clients are joined, from `hotspot-clients`, beside how the device reaches the internet.

The application MUST render the overlay client the device reports under the name `tailscale` as the overlay row, well while its `service` is `passed`, and MUST show the name the device is reached by from that entry's value.

The application MUST render the two Tamanu services the device reports, under the names `tamanu-web` and `tamanu-facility-server`, as one Tamanu row, well only while both are `passed`, and MUST show each service's version from its value.

The application MUST render a `service` it does not recognise as a row of its own, named and valued as [VIEW](device-view.md) renders an unrecognised fact, well while it is `passed`.

The application MUST supply its own wording for every row it recognises, and MUST NOT put a service's reported name in front of an operator where it recognises it.

> [!NOTE]
> The names the client recognises are the device's own, as the overlay's name is in [NFO](device-info.md): a name it knows it renders in its own words, and one it does not it renders generically, exactly as [VIEW](device-view.md) renders the rest of the catalogue. A device that one day reports another service appears as a plain row until a later client learns to word it.

## Restarting a service

The application MUST open a service stream of [SVC](control/services.md) when the Status screen opens, and MUST close it when the operator leaves, as [CSCR](control/screen.md) opens and closes its streams.

The application MUST offer to restart a row whose service or services the device lists as restartable, and MUST offer no restart on a row whose services it does not.

The application MUST restart the overlay on its own, and MUST restart the two Tamanu services together as one act, asking the device to restart each.

The application MUST ask the operator to confirm a restart before asking the device for it, naming what is restarted.

From asking until the feed shows the service running again, the application MUST say the service is restarting in place of its row's restart control, and MUST NOT say of its own account that the restart has completed.

The application MUST render a `restart-refused` reason as the device wrote it.

> [!NOTE]
> A restart does not end the channel, as [SVC](control/services.md) has it, so the operator stays on the Status screen and watches the row leave its well state and return. This is why a restart is offered here and a reboot is not: a reboot takes the device away, and belongs with the acts of [CTL](control/power.md) under Advanced.

## Changing the network

The application MUST offer on the online row to open the Network screen of [NSET](network/setup.md), in the place a restart is offered on the other rows, worded as changing the network while the device is online and as setting it up while it is not.

## Advanced

The application MUST gather, under a disclosure closed by default and headed Advanced, the way to the full readings of [VIEW](device-view.md), the network configuration editor of [NSCR](network/screen.md), and the power and battery controls of [CSCR](control/screen.md).

> [!NOTE]
> A facility operator never needs the disclosure and never meets it by accident; a field technician finds the full depth under Advanced, where they look for it.

## While a session or an act is open

The application MUST show on the Status screen the kept configuration session's bar of [NSCR](network/screen.md), where a session is kept open, offering to return to the screen it belongs to and to confirm or discard what it holds.

The application MUST show on the Status screen a device carrying out an act, as [WEB](web-app.md) specifies, in place of the status, the tasks and Advanced while the act is under way.
