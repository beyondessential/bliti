---
id: DEV
---

# Device operation

## Reporting

A device MUST report failures and identity problems on its standard error.

> [!NOTE]
> These conditions leave a device unreachable over the channel, so there is no client to tell.
> They surface where the device is, and an operator reads them by reaching the device directly.

A device with a backup supply's signal, as in [NFO](device-info.md) "Power source and battery", MUST report on its standard error whether external power reaches the backup supply when it starts, and each time that changes, with the cell voltage and state of charge at the time.
While external power is absent, it MUST also report the cell voltage and state of charge each time the voltage has fallen a further step, with steps fine enough to reconstruct the discharge.
Each report while external power is absent MUST carry how long it has been absent, where the device saw it go.
A device MUST make these reports whether or not it is sampling.

> [!NOTE]
> These let an operator read a run on battery back from the device's log after it has died: when external power went, how the cell fell, and the voltage the device last ran at.
> Sampling stops when nobody has been connected for a while, and a power cut seldom has an audience.
> A cell that is not falling, as on a device fed around its backup supply, adds nothing to the log.

## Memory for the derivation

A device MUST establish that the full argon2id memory parameter of [KEY](key-schedule.md) is available before beginning that derivation, and MUST report that it is not rather than begin.

> [!NOTE]
> The derivation needs its memory at once, and the operating system kills a process asking for more than there is rather than failing the allocation.
> A device that began without checking would terminate part-way through with nothing reported.
