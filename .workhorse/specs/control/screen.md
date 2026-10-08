---
id: CSCR
---

# Power and battery screen

The screen an operator controls a device's power from, through the acts of [CTL](power.md), and its battery's curves, through [CRV](../battery/curve.md), by our application.

[CTL](power.md) and [CRV](../battery/curve.md) bind every implementation.
This spec says how ours applies them, and binds nothing else that controls a bliti device.

## What it holds

The application MUST title the screen Power and battery, and MUST carry that title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the Status screen of [STAT](../status-screen.md), which it is reached from under Advanced, beside it.

The application MUST hold the screen's sections in the order power, battery.

The application MUST open a power stream and a curve stream when the operator opens the screen, and MUST close both when they leave.

> [!NOTE]
> A device older than the application skips `power` and `curve` as it skips anything else it does not recognise, and never answers. Leaving a section out until an answer arrives makes that device look like one with nothing to offer there, rather than one the application is forever waiting on.

## Power

The application MUST offer the acts the device listed, and no others, in the order restart, reboot, power off.

The application MUST leave out the power section until the device has listed its acts, and where it lists none.

The application MUST ask the operator to confirm every act, each time, before asking the device for it.

The confirmation MUST name the act in the words of the act's button.

The application MUST show the confirmation beneath the act's row, in place of its button, leaving the other acts and sections in view, as the Status screen of [STAT](../status-screen.md) shows the confirmation of a restart.

Where the device reports `network-configuration` as `provisional`, as in [NFO](../device-info.md), the confirmation MUST also say that the network settings the device is trying are not saved and will be lost.

The confirmation of a power off MUST say that the device stays off until it is turned on at the device.

The application MUST render a `refused` reason as the device wrote it.

[WEB](../web-app.md) specifies how the application behaves once an act is accepted.

> [!NOTE]
> Every act ends every session, so a proposal being tried reverts under [CFG](../network/session.md) whoever proposed it. The device reports that a proposal is running to every operator, which lets the one asking for the act be warned about another's.

## Battery

The application MUST leave out the battery section until the device has answered `curve`, and where it answers with no document.

The application MUST say how long a full charge lasts, and how long a full recharge takes where the device gives one, each with its margin.

Beneath those, the application MUST say that the device learns its battery curve over time, and that this improves the figures, so the curve the section offers to export, import and reset is introduced where it is offered.

The application MUST offer to export the curve document as a file, to import one from a file, and to reset.

The application MUST ask the operator to confirm every import and reset, each time, before asking the device for it.

The confirmation of an import MUST say that what the device has learnt is replaced, and the confirmation of a reset MUST say that it is discarded.

The application MUST show the figures from the latest `curves` the device sent.

The application MUST render a `refused` reason as the device wrote it.

## Wording

The application MUST supply its own wording for every act and every curve, and MUST NOT put the vocabulary of [CTL](power.md) or [CRV](../battery/curve.md) in front of an operator.

The application MUST name `restart` as restarting bliti.
