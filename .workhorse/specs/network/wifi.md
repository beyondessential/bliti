---
id: WIFI
---

# Joining wifi

The Join wifi screen is where an operator joins a device to one wireless network.
It is a simple network screen of [NSET](setup.md), whose terms it uses, and whose session and way of applying a change it shares.

## The screen

The application MUST title the screen Join wifi, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Network screen beside it.

The application MUST show the operator where they are in joining, as three steps: the network, its password, and connecting.

The application MUST open the screen on the network step.

While the cable is on, the application MUST say on the network step that joining stops using the cable.

While the hotspot is on, the application MUST say on the network step that joining turns the hotspot off.

Edits are not applied on this screen while a network name or password the operator has typed has not been proposed.

## Choosing a network

The application MUST ask the device to scan, as [CFG](session.md#acting-now) has it, on opening the network step, and MUST show that it is looking for networks until the device answers.

The application MUST list the networks the scan heard by SSID, showing for each the strongest signal among its access points, and leaving out access points with no SSID.

The application MUST mark in the list the network the device is joined to, from `wireless-network` of [NFO](../device-info.md), with a check and a highlight, saying it is connected.

The application MUST offer to scan again, and MUST say where a scan heard no network.

The application MUST let the operator pick a network the device can join with a passphrase, as [WLAN](wireless.md) has it, and MUST then go to the password step.

Where the operator picks a network secured by `enterprise`, the application MUST say that the network needs settings kept under Advanced, MUST offer to open [NSCR](screen.md), and MUST offer to choose another network.

The application MUST show a network the device cannot join by any kind it offers, saying that the device cannot join it, and MUST NOT let the operator pick it.

The application MUST let the operator name a network the list does not show, and MUST then go to the password step.

> [!NOTE]
> A network absent from the list is commonly one that hides its name, and naming it is how an operator joins it.

## The password

The application MUST ask on the password step for the network's passphrase alone, naming the network.

The application MUST let the operator show and hide the passphrase as they type it.

Where the operator picks the network wifi already names, the application MUST fill in the passphrase wifi holds.

The application MUST check that the passphrase has from eight to sixty-three characters before proposing it, and MUST say beneath the field where it does not.

## Connecting

The application MUST propose joining the network when the operator asks to connect, as [NSET](setup.md#moving-between-states) has it.

The application MUST replace wifi in place, keeping its position in the ordering of [LINK](attachment.md), with a candidate carrying:

| member | value |
| --- | --- |
| `label` | the network's SSID |
| `ssid` | the network's SSID |
| `security` | the kind below, with the passphrase |
| `hidden` | `true` for a network the operator named, `false` otherwise, where the device offers `hidden` |
| `enabled` | `true` |
| `verify` | `true` |

Where the configuration carries no `wireless` candidate, the application MUST add the candidate at the end of the ordering.

The application MUST secure a network picked from the list by `psk-sae` where its access points advertise both `psk` and `sae`, and otherwise by `sae` where they advertise it, and otherwise by `psk`, taking the first of these the device offers.

The application MUST secure a network the operator named by the first of `psk-sae`, `psk` and `sae` the device offers.

While the device verifies the proposal, the application MUST say that it is connecting to the network, naming it.

Where joining is applied, the application MUST return the operator to the Status screen of [STAT](../status-screen.md) on its own.

> [!NOTE]
> The replaced network's settings go with it, so the configuration never carries a second wireless candidate and stays within the simple states.

## When joining fails

Where the failure's `at` names the passphrase, the application MUST return to the password step, MUST mark the field, and MUST say that the password did not work.

Where the failure stopped at `carrier`, the application MUST say that the device could not reach the network, naming it, and that moving the device closer to the network's router may help.
While it says so, the application MUST return the operator to the Status screen on its own once the device reports the candidate as `default-route`.

Otherwise, the application MUST say that the device could not join the network, naming it, with the failure's `reason` as the device wrote it.

The application MUST offer, beside any failure, to choose another network, and beside one not at the passphrase, to change the passphrase.

> [!NOTE]
> A wireless candidate that stops at `carrier` is out of range, as [CFG](session.md#the-state-of-each-candidate) has it.
> The device keeps the configuration that failed and joins the network once it hears it, as [LINK](attachment.md#selection-is-continuous) has it, so the screen can wait for that rather than ask the operator to try again.
