---
id: CSCR
---

# Power and battery screen

The screen an operator controls a device's power from, through the acts of [CTL](power.md), and reads its batteries from, through the facts and readings of [NFO](../device-info.md) and the curves of [CRV](../battery/curve.md), by our application.

[CTL](power.md), [NFO](../device-info.md) and [CRV](../battery/curve.md) bind every implementation.
This spec says how ours applies them, and binds nothing else that controls a bliti device.

## What it holds

The application MUST title the screen Power and battery, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Status screen of [STAT](../status-screen.md) beside it.

The application MUST hold the screen's sections in the order power, battery.

The application MUST open a power stream and a curve stream when the operator opens the screen, and MUST close both when they leave.

> [!NOTE]
> A device older than the application skips `power` and `curve` as it skips anything else it does not recognise, and never answers. Leaving a section out until an answer arrives makes that device look like one with nothing to offer there, rather than one the application is forever waiting on.

## Power

The application MUST offer the acts the device listed, and no others, in the order restart, reboot, power off.

The application MUST leave out the power section until the device has listed its acts, and where it lists none.

The application MUST ask the operator to confirm every act, each time, before asking the device for it.

The confirmation MUST name the act in the words of the act's button.

The application MUST show the confirmation beneath the act's row, in place of its button, leaving the other acts and sections in view.

The confirmation of a reboot MUST say that the device is unavailable while it reboots.

The confirmation of a power off MUST say that the device stays off until it is turned on at the device.

The application MUST render the reason of a `cannot-act` as the device wrote it.

[WEB](../web-app.md) specifies how the application behaves once an act is accepted.

## Battery

The application MUST show the battery section where the device reports a `battery-charge`, and MUST leave it out otherwise.

The application MUST show in it every battery the device reports a `battery-charge` for, each as a block of its own, ordered by `battery` name with the battery named `built-in` last.

Where the device reports more than one battery, the application MUST head each block with the battery's name as the device sent it.

The application MUST headline each block with the battery's `battery-charge`, with its `battery-direction` beside it, and MUST draw the charge against its scale beneath.

The application MUST show beneath the scale the battery's `battery-time-to-empty`, `battery-time-to-full` and `battery-voltage`, each where the device reports it, pairing each with its `battery-charge` by the `battery` trait.

The application MUST show `power-source`, where the device reports it, with the facts of the battery where there is one, and once above the blocks where there are several.

The application MUST show the latest of each entry the device sent, and MUST remove an entry when the device sends it as `ended`.

The application MUST colour each entry by its `status` as [VIEW](../device-view.md) colours a tile's face, MUST show the reason of any entry whose `status` is not `passed`, and MUST show an entry whose `status` is `skipped` or `broken` as having no value, distinguishing the two.

The application MUST show a reading's `margin` beside its value.

## Battery curve

The application MUST show the battery curve beneath the block of the battery named `built-in`, where the device reports one and has answered `curve` with a document, and MUST leave it out otherwise.

> [!NOTE]
> A device learns a curve only for the backup supply it manages, and the cell of that supply is the one it names `built-in`, as [NFO](../device-info.md) has it.
> Keeping that battery last keeps its curve beside it.

The application MUST say how long a full charge lasts, and how long a full recharge takes where the device gives one, each with its margin.

Beneath those, the application MUST say that the device learns its battery curve over time, and that this improves the figures, so the curve the section offers to export, import and reset is introduced where it is offered.

The application MUST offer to export the curve document as a file, to import one from a file, and to reset.

The application MUST ask the operator to confirm every import and reset, each time, before asking the device for it.

The confirmation of an import MUST say that what the device has learnt is replaced, and the confirmation of a reset MUST say that it is discarded.

The application MUST show the figures from the latest `curves` the device sent.

The application MUST render the reason of a `cannot-act` or `cannot-configure` as the device wrote it.

## Wording

The application MUST supply its own wording for every act and every curve, and MUST NOT put the vocabulary of [CTL](power.md) or [CRV](../battery/curve.md) in front of an operator.

The application MUST word each battery entry and each of its values as [VIEW](../device-view.md) words it.

The application MUST name `restart` as restarting bliti.
