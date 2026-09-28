---
id: CSCR
---

# Control screen

The screen an operator changes what a device is doing from: its network, through the screen of [NSCR](../network/screen.md), and its power, through the acts of [CTL](overview.md), by our application.

[CTL](overview.md) binds every implementation.
This spec says what ours makes of it, and binds nothing else that controls a bliti device.

## What it holds

The application MUST carry the Control screen's title where the device view carries its own, as [VIEW](../device-view.md) has it, with the way back to the device view beside it.

The application MUST offer the network configuration screen of [NSCR](../network/screen.md) from the Control screen, and MUST return the operator to the Control screen when they leave it.

The application MUST open a control stream when the operator opens the Control screen, and MUST close it when they leave.

The application MUST offer the acts the device listed, and no others, in the order restart, reboot, power off.

The application MUST leave out the power section until the device has listed its acts, and where it lists none.

> [!NOTE]
> A device older than the application skips `control` as it skips anything else it does not recognise, and never answers. Leaving the section out until an answer arrives is what makes that device look like one that offers no acts, rather than one the application is forever waiting on.

## Asking for an act

The application MUST ask the operator to confirm an act before asking the device for it.

Where the device reports `network-configuration` as `provisional`, as in [NFO](../device-info.md), the confirmation MUST say that the network settings the device is trying are not saved and will be lost.

Where the application holds a configuration session open with edits not applied, as [NSCR](../network/screen.md) has it, the confirmation MUST say that those edits will be lost.

The confirmation of a power off MUST say that the device stays off until it is turned on at the device.

The application MUST render a `refused` reason as the device wrote it.

What the application does once an act is accepted is specified in [WEB](../web-app.md).

> [!NOTE]
> Every act ends every session, so a proposal being tried reverts under [CFG](../network/session.md) whoever proposed it. The device reports that a proposal is running to every operator, which is what lets the one asking for the act be warned about another's.

## Wording

The application MUST supply its own wording for every act, and MUST NOT put the vocabulary of [CTL](overview.md) in front of an operator.

The application MUST name `restart` as restarting bliti.
