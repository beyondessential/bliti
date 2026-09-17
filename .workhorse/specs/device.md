---
id: DEV
---

# Device operation

## Reporting

A device MUST report failures and identity problems on its standard error.

> [!NOTE]
> These conditions leave a device unreachable over the channel, so there is no client to tell.
> They surface where the device is, and an operator reads them by reaching the device directly.

## Memory for the derivation

A device MUST establish that the full argon2id memory parameter of [KEY](key-schedule.md) is available before beginning that derivation, and MUST report that it is not rather than begin.

> [!NOTE]
> The derivation needs its memory at once, and the operating system kills a process asking for more than there is rather than failing the allocation.
> A device that began without checking would terminate part-way through with nothing reported.
