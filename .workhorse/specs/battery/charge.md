---
id: CHG
---

# Battery charge

How a device estimates the charge left in the cell of a backup supply it manages, and the time until it is empty or full, which it reports as `battery-charge`, `battery-time-to-empty` and `battery-time-to-full` under [NFO](../device-info.md).

The estimate is how much of the cell's usable charge remains: 1 is a full cell, and 0 is the floor at which the device powers itself off under [LOW](shutdown.md).

## Curves

A curve maps the cell's voltage to its charge, for one direction of travel.

A device MUST hold a discharging curve, for the voltage under the device's own load while the cell carries it, and MAY hold a charging curve, for the voltage while the cell takes charge.

A curve's charge is on the scale of the discharging curve: 0 at its lowest point and 1 at full.

Every build MUST carry a discharging curve measured on an X120x backup board from a full cell until the board could no longer carry the device, the same for every such board.

A device MUST start from the curve its build carries, and MUST return to it when reset under [CRV](curve.md).

A device MUST report a charge taken from a curve as the share of the charge the discharging curve places between the floor and full, reporting 0 at or below the floor and 1 at or above full.

> [!NOTE]
> The backup board's gauge estimates charge against a generic model whose empty point sits well above where these boards stop, so it reads 0 for the last stretch of every run on battery while the device goes on running.
> The gauge cannot be tuned out of this, and no rescaling of its figure recovers a stretch it reads flat across, so the device works from the cell voltage itself.
> Scaling to the floor keeps 0 meaning the device is about to power itself off, whatever the floor is.

## What is reported

While external power is absent, a device MUST report the discharging curve's charge at the cell voltage.

While external power reaches the backup supply, a device MUST report the charging curve's charge at the cell voltage, where it holds a charging curve learnt from at least three charges and the voltage lies within what that curve covers.

Otherwise, while external power reaches the backup supply, a device MUST report the gauge's own state of charge, scaled so that the figure the gauge gives for a full cell reports as 1.

Once the backup supply has finished charging the cell, as the cell voltage shows, a device MUST report the cell as full, whatever a curve or the gauge gives, until the cell next carries the device.

> [!NOTE]
> The charge current holds the voltage above where it would rest, so a discharging curve reads high while the cell charges, and until it has seen enough charges of its own, the device has only the gauge's figure.
> The gauge seldom reaches its own full reading on a full cell, which is why its figure is scaled to its top and a finished charge is taken from the voltage.

## Learning

A device MUST refine its discharging curve from each run on battery that ends in a shutdown under [LOW](shutdown.md), and from no other run.

A device MUST take the time a run spent between two voltages as the charge the cell gave between them.

A run that began from a full cell MUST refine the whole curve, and a run that began below full MUST refine the curve only below the voltage it began at, starting from the charge the curve gave there.

A device MUST learn its charging curve from each charge that begins at a charge the device knows, as when external power returns to a device running on battery, and continues until the cell is full, and from no other charge.

A device MUST weigh recent runs and charges above older ones, so that the curves of a replaced cell take over from the old cell's within a few runs.

A device MUST keep its curves across restarts, reboots and loss of power, and MUST have recorded what a run taught it before the shutdown ending that run begins.

A device MUST report on its standard error each time it refines a curve, with how many runs or charges the curve has now been learnt from and its error.

> [!NOTE]
> The gauge measures voltage and nothing else, so time at the device's own draw stands in for charge.
> Only a run that reaches the floor says how far the cell had to go from each voltage it passed, and only a charge that starts from a known figure and finishes says how far it came.

## Accuracy

A device MUST hold, for each curve, its error: how far the charge the curve gives is expected to be off, as a share of a full cell.

Before refining a curve from a run or charge, a device MUST measure how far the curve's charge along the way differed from the charge the run or charge showed, and MUST weigh that into the curve's error as it weighs runs and charges into the curve itself.

Until a curve's error has been measured, a device MUST estimate it from how many runs or charges the curve has been learnt from, larger the fewer.

A device MUST hold, for each curve, its duration: how long the cell took to cover the curve's whole scale at the rate the device has seen it move, discharging under the device's own load or taking charge.

Where the charge on mains comes from the gauge, a device MUST take its error as that of a charging curve learnt from no charges.

A full charge lasts the time the discharging curve's duration gives between full and the floor, give or take the discharging curve's error over that time.

A full recharge takes the time the charging curve's duration gives between the floor and full, give or take the charging curve's error over that time.

## Time left

While the cell carries the device, a device MUST estimate how long until the charge reaches 0, from the charge and the rate it has recently fallen at.

While the cell takes charge, a device MUST estimate how long until it is full, from the charge and the rate it has recently risen at.

A device MUST give each estimate a margin either way, combining the error of the figure it was taken from, at the rate the charge is moving, with how much that rate has varied.

A device MUST report an estimate as skipped until it has watched the charge move long enough to have a rate, with a reason saying so.

> [!NOTE]
> The rate is the device's own recent one rather than a curve's duration, so a device that has just become busy says so in the time it reports.
