---
id: SHARE
---

# Sharing the hotspot

The Share hotspot screen is where an operator turns a device's hotspot on and off, and reads or changes the password other devices join it with.
It is a simple network screen of [NSET](setup.md), whose terms it uses, and whose session and way of applying a change it shares.

## The switch

The application MUST title the screen Share hotspot, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Network screen beside it.

The application MUST present the hotspot as a single switch letting other devices join, on while the hotspot is on.

The application MUST propose turning the hotspot on or off, as [NSET](setup.md#moving-between-states) has it, as soon as the operator turns the switch.

The application MUST NOT let the operator turn the switch while a change is being applied.

While the hotspot is on, the application MUST show beneath the switch how many devices are joined, from `hotspot-clients` of [NFO](../device-info.md).

While the hotspot is off and wifi is on, the application MUST say beneath the switch that turning it on leaves the wireless network, naming it, and that the device then uses its cable, or goes offline where no wired interface reports `wired-carrier` `true`.

Where a change to the hotspot fails, the application MUST show the switch as the configuration the device holds has it, with the failure's `reason` as the device wrote it beneath the switch.

> [!NOTE]
> The hotspot runs beside the cable, so a device sharing a cabled uplink keeps it.

## The details

The application MUST show the hotspot's SSID and passphrase beneath the switch, whether the hotspot is on or off.

The application MUST show the SSID as text.

The application MUST hide the passphrase until the operator asks to see it, and MUST let them hide it again.

The application MUST let the operator change the passphrase in place, and MUST propose the hotspot with the new passphrase, on or off as it is, when they save it.

The application MUST let the operator cancel a change to the passphrase before saving it, keeping the one the configuration holds.

While the hotspot is on and the operator is changing the passphrase, the application MUST say beneath the field that devices already joined need to join again with the new password.

The application MUST check that a new passphrase has from eight to sixty-three characters before proposing it, and MUST say beneath the field where it does not.

Edits are not applied on this screen while the operator has changed the passphrase and not saved it.

The application MUST show beneath the SSID and passphrase, whether the hotspot is on or off, the QR code a phone joins the hotspot by, encoded as [VIEW](../device-view.md#joining-the-hotspot) encodes it, for the SSID and passphrase the configuration holds.

> [!NOTE]
> The SSID is changed under Advanced, in [NSCR](screen.md), with the rest of the hotspot's settings.

## A hotspot not yet set up

Where the configuration carries no hotspot, the application MUST show as its SSID the device's `hostname`, or the name [STAT](../status-screen.md#the-landing) titles the device by where it reports none, and as its passphrase one the application generates.

The application MUST propose the hotspot with that SSID and passphrase the first time the operator turns it on or saves a passphrase.

The application MUST keep the passphrase it generates for as long as the session is open.

The application MUST generate a passphrase of from eight to sixty-three characters, of words and digits that are easy to read aloud.

> [!NOTE]
> [HOT](hotspot.md) has a device supply no SSID or passphrase of its own, so a hotspot set up here takes the values the application proposes.
