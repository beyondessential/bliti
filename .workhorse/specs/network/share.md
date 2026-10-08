---
id: SHARE
---

# Sharing the hotspot

The Share hotspot screen is where an operator turns the device's hotspot on and off and reads or changes the password other devices join it with: the hotspot of [HOT](hotspot.md), presented as a phone presents its own.

It is one of the three ways of [NSET](setup.md), reached from the Network screen, and its changes are proposed and kept as [NSET](setup.md) has it.
[NSCR](screen.md), reached under Advanced, holds the rest of the hotspot's settings, its network name among them.

## Turning the hotspot on and off

The application MUST title the screen Share hotspot, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Network screen beside it.

The application MUST present the hotspot as a single switch letting other devices join, on where the configuration in force carries a hotspot whose `enabled` is true, and MUST let the operator turn it on and off by the switch alone.

The application MUST show beneath the switch, while it is on, how many devices are joined, from `hotspot-clients`.

Turning the hotspot on MUST set it `enabled` true and turn the wireless candidates and wired attachments off, as [NSET](setup.md) has it, so the hotspot never runs beside a cable.
Turning it off MUST set it `enabled` false, set every wired attachment `enabled` true, and set the wireless candidate [WIFI](wifi.md) presents `enabled` true, where there is one.

The application MUST say beneath the switch, while it is off and the device is connected, what turning it on stops using, the wireless network by name or the cable, and that the device would go offline.

## The details

The application MUST show the hotspot's network name and password beneath the switch, whether the switch is on or off.

The application MUST show the network name as text, leaving it to be changed in [NSCR](screen.md).

The application MUST hide the password until the operator asks to see it, and MUST let them hide it again.

The application MUST let the operator change the password in place, and MUST propose it when they save it, saying beneath the field that devices already joined will need to reconnect with the new password.

The application MUST check that a new password has at least eight characters before proposing it, and MUST say beneath the field where it does not.

The application MUST show beneath the network name and password, whether the switch is on or off, the QR code a phone joins the hotspot by, as [VIEW](../device-view.md) draws the same code.

## A hotspot not yet set up

Where the configuration in force carries no hotspot, the application MUST show as its network name the device's `hostname`, and as its password one the application generates, and MUST propose both when the operator first turns the hotspot on.

The application MUST generate a password of at least eight characters, from words and digits easy to read aloud.
