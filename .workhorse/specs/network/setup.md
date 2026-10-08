---
id: NSET
---

# Network setup

The Network screen is where a facility operator puts the device on the same network as the phones and laptops that use it, by one of three ways: a cable, a wireless network the device joins, or the device's own hotspot.

The three are offered as one at a time, though the configuration of [NET](overview.md) can hold several at once.
[NSCR](screen.md), reached under Advanced, is where they are combined.
Joining a wireless network is specified by [WIFI](wifi.md) and the hotspot by [SHARE](share.md).

## The three ways

The application MUST title the screen Network, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Status screen of [STAT](../status-screen.md) beside it.

The application MUST say, above the three ways, that phones and laptops must be on the same network as the device.

The application MUST offer the three ways in this order, each opening a screen of its own:

| way | opens |
| --- | --- |
| plug in a cable | the cable screen below |
| join the clinic's wifi | the screen of [WIFI](wifi.md) |
| share the device's hotspot | the screen of [SHARE](share.md) |

The application MUST show against each way the state it is in now: against the cable, whether a wired interface has carrier; against wifi, the wireless network the device is joined to, from `wireless-network`; against the hotspot, whether it is on.

The application MUST mark one way as in use, taking the first of these that holds: the hotspot, where it is on; wifi, where the device is joined to a wireless network; the cable, where a wired interface carries the `default` route.
Where none holds, the application MUST mark none.

The application MUST set the way in use apart from the other two with a check and a highlight, and MUST show against it that it is connected, beside the network joined where it is wifi.

## Using one turns the others off

The application MUST, where the operator puts one way into use, turn off the others that the configuration in force has on, in the same proposal:

- joining a wireless network turns the hotspot off;
- turning the hotspot on turns off every wireless candidate and every wired attachment whose `enabled` is true;
- using the cable turns every wired attachment on, and turns off the hotspot and every wireless candidate whose `enabled` is true.

The application MUST turn things off by setting `enabled` false, as [LINK](attachment.md) and [HOT](hotspot.md) keep a candidate or hotspot that is off with its settings, and MUST NOT remove them.

The application MUST NOT propose from these screens a configuration whose hotspot is on beside a wired attachment whose `enabled` is true.

The application MUST otherwise leave every wired attachment as the configuration in force holds it.

## Changes are kept

The application MUST propose each change made from the screens of this spec, [WIFI](wifi.md) and [SHARE](share.md) through the session of [CFG](session.md), and MUST confirm it as soon as the device answers with `applied`, whether or not the operator is still on the screen it was made from.

The application MUST NOT ask the operator to keep a change.

The application MUST close the session once a change is confirmed or has failed.

> [!NOTE]
> Each change here is undone by making the opposite one: turning the hotspot off brings back the connections it took the place of, and a network joined is left by choosing another way.
> The channel is Bluetooth, which no network change drops, so a change that leaves the device offline can be undone from where the operator stands.

## The cable

The application MUST ask the operator, on the cable screen, to plug a network cable into the device's network port from the clinic's router or a wall socket, and MUST say it is waiting for one until a wired interface has carrier.

The application MUST return the operator to the Status screen of [STAT](../status-screen.md) on its own once a wired attachment carries the `default` route, where the network row says the device is connected by cable.

The application MUST put the cable into use, as above, only once a wired interface has carrier, and MUST NOT do so while the operator is waiting for a cable.
Where every wired attachment is already on, the application MUST wait further, until a wired attachment carries the `default` route, so a cable that gives no connection never turns the others off.
Where the hotspot has turned the wired attachments off, the proposal turns them on, and the device verifies the cable as [CFG](session.md) verifies any candidate a proposal changes.

Where a wired interface has carrier and no wired attachment carries the `default` route, the application MUST say that the cable is plugged in and the network it reaches did not give the device a connection.
