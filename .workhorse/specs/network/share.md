---
id: SHARE
---

# Sharing wifi

The Share screen is where an operator turns the device's hotspot on and off and reads or changes the credentials other devices join it by: the hotspot of [HOT](hotspot.md) presented for a facility operator.

This screen edits the hotspot through the session of [CFG](session.md), as [NSCR](screen.md) does, and leaves the attachments as they are but for the one case below where sharing and the device's own wifi cannot both run.

## Turning sharing on and off

The application MUST present whether the hotspot is on, from whether the configuration in force carries a hotspot whose `enabled` is true, and MUST let the operator turn it on and off.

Turning sharing on MUST set the hotspot `enabled` true, and turning it off MUST set it false, each proposed through the session of [CFG](session.md) and kept on confirm as a join of [WIFI](wifi.md) is kept.

The application MUST show, while sharing is on, how many devices are joined, from `hotspot-clients`.

The application MUST require a network name and a passphrase before sharing can be turned on, and MUST NOT supply either itself, as [HOT](hotspot.md) forbids a device deriving them.

## Sharing and the device's own wifi

Where the device cannot run the hotspot beside the wireless network it is joined to, as [HOT](hotspot.md) has it for a radio that runs an access point and a wireless client only one at a time or only on one channel, the application MUST, on turning sharing on, also turn that wireless connection off in the same proposal.

The application MUST tell the operator, before it does so, that sharing turns the device's own wifi connection off, and whether the device stays online through the wired network or goes offline, drawn from whether a wired attachment carries the default route.

Where the device is online through the wired network, or its radio runs the hotspot and its wireless client at once, the application MUST turn sharing on without taking the wifi connection off.

> [!NOTE]
> The device has one radio, and one radio cannot both join a network and be the network. An operator sharing from a clinic on a wired uplink loses nothing; one sharing from a device on wifi loses its uplink, and is told which before it happens rather than after.

## The credentials

The application MUST show the hotspot's network name and passphrase, revealing the passphrase on request, and MUST show a QR code a phone joins the hotspot by, as [VIEW](../device-view.md) draws the same code.

The application MUST let the operator change the network name and the passphrase, proposed through the session of [CFG](session.md) and kept on confirm.

> [!NOTE]
> The passphrase is the one credential on the device meant to be read aloud and handed to a stranger, as [HOT](hotspot.md) has it, so the screen an operator shares from is where it is shown, rather than won from the readings under Advanced.
