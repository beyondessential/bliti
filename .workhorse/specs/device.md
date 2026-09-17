---
id: DEV
---

# Device operation

A device runs as a daemon on hardware with no screen and no input.
What it cannot say over the channel, it says where it is.

## Reporting

A device MUST report failures and identity problems on its standard error.

> [!NOTE]
> These conditions leave a device unreachable over the channel, so there is no client to tell.
> They surface where the device is, and an operator reads them by reaching the device directly.
