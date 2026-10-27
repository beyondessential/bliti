---
id: NSET
---

# Network setup

The simple network screens are where a facility operator puts a device on the same network as the phones and laptops that use it: by cable, by a wireless network the device joins, or by the device's own hotspot.
They are the Network screen and the Cable screen of this spec, the Join wifi screen of [WIFI](wifi.md) and the Share hotspot screen of [SHARE](share.md).

The simple network screens edit the [configuration document](../overview.md#configuration-document) of [NET](overview.md) within the simple states below.
[NSCR](screen.md), reached under Advanced, edits anything the device supports.
Every restriction this spec places on a configuration binds the simple network screens alone.

## The simple states

This spec, [WIFI](wifi.md) and [SHARE](share.md) use these terms of a configuration:

| term | meaning |
| --- | --- |
| the cable | every `wired-dynamic` and `wired-static` [candidate](../overview.md#candidate) the configuration carries |
| wifi | the `wireless` candidate the configuration carries, where it carries exactly one |
| on | for the cable, carrying at least one candidate, every one with `enabled` true; for wifi and the hotspot, present with `enabled` true |
| off | for the cable, carrying at least one candidate, none with `enabled` true; for wifi and the hotspot, absent or with `enabled` false |

A configuration MUST be in one of the simple states where it matches a row of this table, and outside them otherwise:

| state | the cable | wifi | the hotspot |
| --- | --- | --- | --- |
| cable | on | off | off |
| cable and hotspot | on | off | on |
| wifi | off | on | off |

A configuration carrying more than one `wireless` candidate MUST be outside the simple states.

> [!NOTE]
> The uplink is the cable or wifi, and the hotspot is an addition to the cable alone. A hotspot with no cable plugged in is the cable and hotspot state.
> A device that has never saved a configuration holds the cable state, as [CFG](session.md) gives it.
> A configuration with both uplinks on is an ordering of [LINK](attachment.md), which only [NSCR](screen.md) presents.

## Moving between states

The application MUST put a way into use by proposing the configuration the device holds with these changes:

| the operator | the cable | wifi | the hotspot |
| --- | --- | --- | --- |
| joins a wireless network, as [WIFI](wifi.md) has it | off | the network joined, on | off |
| uses the cable, as the Cable screen has it | on | off | unchanged |
| turns the hotspot on, as [SHARE](share.md) has it | on | off | on |
| turns the hotspot off, as [SHARE](share.md) has it | unchanged | unchanged | off |

The application MUST turn a candidate or the hotspot off by setting its `enabled` false, and MUST NOT remove it.

The application MUST leave every other member of the configuration as the device holds it, except where [WIFI](wifi.md) replaces wifi and [SHARE](share.md) changes the hotspot's passphrase or sets up a hotspot for the first time.

The application MUST NOT propose, from a simple network screen, a configuration outside the simple states.

> [!NOTE]
> A candidate turned off keeps its settings, so a cable with a static address works again when the cable is next used.

## Outside the simple states

While the configuration the device holds is outside the simple states, the Network screen MUST say that the device's network is set up under Advanced, MUST offer the way to [NSCR](screen.md), and MUST NOT offer the cable, wifi or the hotspot.

> [!NOTE]
> A technician's configuration is one the simple network screens cannot express, so they leave it alone rather than overwrite it.

## The session

The application MUST open a configuration session of [CFG](session.md) when the operator asks to change or set up the network from the network row of [STAT](../status-screen.md).

The application MUST stay on the Status screen, showing on the network row that it is waiting on the device, until the session's first `state` arrives.

Where that `state` carries `busy` true, the application MUST close the session, MUST stay on the Status screen, and MUST say on the network row that someone else is changing the device's network, until the operator next asks to change or set it up or leaves the Status screen.

Otherwise, the application MUST show the Network screen, and MUST hold the session open across the simple network screens.

The application MUST close the session when the operator returns to the Status screen, by any way back.

The application MUST close the session before opening [NSCR](screen.md) from a simple network screen.

Where the application reconnects as [WEB](../web-app.md#while-the-page-is-hidden) has it while a simple network screen is open, it MUST open a session again, and MUST keep the screen and the operator's edits where that session is not busy.
Where it is busy, the application MUST return the operator to the Status screen, saying on the network row that someone else is changing the device's network.

Where the session ends while the channel stays open, the application MUST say so on the screen showing and offer to open it again.

> [!NOTE]
> The session is opened only when it is about to be used: a Status screen holding one idly would be the session holding the configuration, and every other operator's would be busy.
> [NSCR](screen.md) opens a session of its own, which would be busy behind the simple network screens' had they not closed theirs.

## Applying a change

The application MUST propose a change as soon as the operator makes it, as each simple network screen has it, and MUST NOT ask the operator to confirm or keep it.

The application MUST show that it is applying from proposing a change until the device reports its outcome as `applied` or `failed`, and MUST NOT offer to cancel it.

The application MUST then show the failure of a change that failed, as each simple network screen has it.

Where the device refuses a proposal, the application MUST show the refusal's `reason` as the device wrote it, and MUST keep the operator's edits.

Edits are not applied while the operator has entered something on a simple network screen that the application has not proposed, as [WIFI](wifi.md) and [SHARE](share.md) have it.

The application MUST ask the operator to confirm leaving a simple network screen while edits are not applied, by its way back or by the browser's, saying that they will be lost.

The application MUST have the browser warn before the page is closed, reloaded or navigated away from while edits are not applied.

> [!NOTE]
> A device saves a configuration that fails as it does one that works, as [CFG](session.md) has it, so leaving after a change has been proposed loses nothing, and a change is undone by making the opposite one.
> The channel runs over Bluetooth, as [CHN](../channel.md) has it, so no network change drops the operator's connection to the device.
> A channel that closes drops edits not applied without asking, as after any closed channel.

## The Network screen

The application MUST title the screen Network, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Status screen beside it.

The application MUST say, above the ways, that phones and laptops must be on the same network as the device.

The application MUST offer the three ways in this order, each opening a screen of its own:

| way | titled | opens |
| --- | --- | --- |
| the cable | Cable | the Cable screen |
| wifi | Wifi | the Join wifi screen of [WIFI](wifi.md) |
| the hotspot | Iti hotspot | the Share hotspot screen of [SHARE](share.md) |

The application MUST mark each way that is on, with a check and a highlight.

The application MUST show against each way how it stands now, from [NFO](../device-info.md), counting a way as in use as [STAT](../status-screen.md#the-network-row) counts it:

| way | is | shows |
| --- | --- | --- |
| the cable | in use | connected |
| the cable | on, with a wired interface's `wired-carrier` `true` and not in use | plugged in, no connection |
| the cable | off, with a wired interface's `wired-carrier` `true` | plugged in |
| the cable | otherwise | no cable |
| wifi | in use | connected, with the network's name |
| wifi | on and not in use | not connected |
| wifi | off | off |
| the hotspot | on | on, with how many devices are joined from `hotspot-clients` where it carries a value |
| the hotspot | off | off |

## The Cable screen

The application MUST title the screen Cable, and MUST carry that title where the device view carries its own, with the way back to the Network screen beside it.

While no wired interface reports `wired-carrier` `true`, the application MUST ask the operator to plug a network cable into the device's network port from the site's router or a wall socket, and MUST say that it is waiting for a cable.

While the cable is off, the application MUST use the cable, as [Moving between states](#moving-between-states) has it, once a wired interface reports `wired-carrier` `true`, and MUST NOT use it before.

Where using the cable is applied, the application MUST return the operator to the Status screen on its own.

Where using the cable fails, the application MUST say that the cable is plugged in and the network it reaches did not give the device a connection, with the failure's `reason` as the device wrote it.

While the cable is on, the application MUST NOT propose anything from the Cable screen.

While the cable is on and in use, the application MUST say that the device is connected by cable, and where it was not in use when the screen opened, MUST return the operator to the Status screen on its own.

While the cable is on, a wired interface reports `wired-carrier` `true`, and the cable is not in use, the application MUST say that the cable is plugged in and the network it reaches has not given the device a connection.

> [!NOTE]
> A device on wifi has its wired candidates off, and `wired-carrier` is how the screen learns a cable is in before using it, as [NFO](../device-info.md) has it.
> Waiting for a cable before using it keeps the device on wifi until there is something to move to.
