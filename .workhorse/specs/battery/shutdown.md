---
id: LOW
---

# Low-battery shutdown

A device running on the cell of a backup supply it manages powers itself off before the cell runs out.

## The floor

The floor is a cell voltage, read under the device's own load, below which the device does not go on running from its cell.

The floor MUST be 2.8 V.

> [!NOTE]
> The X120x boards carry the device until the cell is within tens of millivolts of its rated minimum, and their cells carry no protection of their own, so stopping short of that is the device's to do.
> The board goes on drawing from the cell once the device is off, and that drain eats into the margin left above the rated minimum while external power stays away.

## Shutting down

A device MUST watch for a low battery only on a backup supply whose gauge and signal of [NFO](../device-info.md) it reads itself, and MUST leave a battery its operating system reports to the operating system's own power management.

> [!NOTE]
> A machine whose battery comes from its operating system, such as a laptop running bliti in development, already has power management of its own, which decides when that machine sleeps or shuts down.

A device MUST power off once external power has been absent, as the backup supply's signal of [NFO](../device-info.md) gives it, and the cell has read below the floor at every reading, for sixty seconds.

A device MUST read the cell at least every ten seconds while external power is absent.

A reading at or above the floor, a reading that could not be taken, or external power returning MUST start the sixty seconds again.

A device MUST watch for this whether or not it is sampling under [NFO](../device-info.md).

A device MUST NOT begin a shutdown within two minutes of its system starting.

> [!NOTE]
> A device near the floor sags below it for a moment whenever it is busy, and the sixty seconds ride that out.
> The two minutes are for someone at a device just turned on with a low cell, to plug it in before it turns itself off again.

A device MUST power off as [CTL](../control/power.md) carries out an accepted `power-off`, sending `going-away` with `low-battery` as its cause.

Once it has sent `going-away`, a device MUST carry the shutdown through, whether or not external power returns.

A device that has accepted an act under [CTL](../control/power.md) MUST carry that act out, and MUST NOT begin a shutdown here as well.

A device MUST report on its standard error that it is powering off for low battery, with the cell voltage, and how long external power has been absent where it saw it go.

A device that cannot power off its system MUST report on its standard error, each time the floor has been held for sixty seconds, that it cannot.
