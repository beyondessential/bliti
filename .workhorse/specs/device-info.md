---
id: NFO
---

# Device information

A device reports what board it is, what it is running, and how it is doing, as the facts and readings below.

This spec is the catalogue: which entries a device reports and what each is about.
What a reader does with them is its own; [VIEW](device-view.md) specifies what ours does.

## Borrowed terms

| term | meaning |
| --- | --- |
| state of charge | How full a cell is, as a proportion of its capacity. Not whether it is charging or discharging, which is its direction of travel. |
| overlay | A virtual network built across whatever physical links a device has, over which a fleet is reached. |
| throttling | A platform limiting its own processor, typically for heat or for supply voltage. |

## The topic

A device MUST send its facts and readings on the topic `default`.

## Facts and readings

A device reports what it knows about itself as `fact` and `reading` messages, within the envelope of [MSG](messages.md).

Both are ordinary feature message types: either end may send them, and an end with nothing to do about one received treats it as the no-op [MSG](messages.md) requires.

A `fact` is something true about the device.
A `reading` is a measurement, whose history is worth keeping.

The two are one shape, and what separates them is which catalogue names them.

| member | type | required | meaning |
| --- | --- | --- | --- |
| `at` | number | yes | milliseconds since the sender booted, when it was taken |
| `fact` or `measurement` | string | yes | what it is, named against the catalogue below; `fact` on a `fact`, `measurement` on a `reading` |
| `traits` | object | yes | what it is about, as below |
| `kind` | string | yes | what the value is |
| `unit` | string | no | the unit the value is in |
| `value` | any | no | the value |

A receiver MUST treat `at` as meaningful only relative to other `at` values from the same sender.

A device MUST send one message per fact or reading, at whatever cadence suits what it reports.

> [!NOTE]
> Times are measured from boot rather than from an epoch because a device in the field may have no set clock and no way to reach one.
> The two catalogues are separate and may name the same thing: a fact and a reading of one name are different things.

### What a message is about

`traits` says what a fact or reading is about.

A trait either distinguishes what a measurement was taken on, or describes it.

A distinguishing trait MUST be a dimension the sender could aggregate across: one that slices its catalogue entry into instances which can meaningfully be summed, ranked or compared.

A descriptive trait says something about the measurement without separating it from anything else the same measurement is taken on.

A trait's value MAY be of any JSON type, and MUST be an object where the trait has more than one thing to say.

A member that only qualifies another trait MUST sit inside that trait rather than beside it.

Where the sender holds a piece of information a trait could carry, it MUST send it rather than leave it implied.

> [!NOTE]
> Two measurements that merely resemble each other are two catalogue entries rather than one entry and a trait, because re-combining their instances would mean nothing.

### Telling entries apart

A device MUST emit entries a reader can tell apart: two entries that are about different things MUST differ somewhere in what is sent.

A device MUST NOT rely on the order entries arrive in to distinguish them.

> [!NOTE]
> How a reader groups entries, and what it treats as one thing measured over time, is the reader's own business. The device's obligation is only that what it sends is distinguished well enough for a reader to do that unambiguously.
> Two addresses on one interface meet this through their `kind`, which tells an `ipv4` from an `ipv6` without a trait to separate them.

### Status

The `status` trait says how the datum stands: whether there is a value, and where the measurement has a notion of being in difficulty, what it says about the thing measured.

| `is` | meaning | `value` |
| --- | --- | --- |
| `passed` | the measurement was taken and what it measures is well | present |
| `warning` | what it measures is degraded, but not gravely | present |
| `failed` | what it measures is unwell | present |
| `skipped` | a precondition was not met, so nothing was measured | absent |
| `broken` | the measurement was attempted and errored | absent |

Every fact and reading MUST carry the `status` trait.

An entry MUST carry `value` where `is` is `passed`, `warning` or `failed`, and MUST NOT carry it where `is` is `skipped` or `broken`.

`status` MUST carry `reason` where `is` is anything but `passed`, and MUST NOT carry it where `is` is `passed`.

`reason` is free text, in the sender's own words, saying what happened.

A device MUST report `warning` or `failed` only where the measurement has a notion of being in difficulty.

> [!NOTE]
> The status is the datum's and not the device's: `passed` against a throughput reading says the figure is sound, not that the link is quiet. That is what lets every entry carry a status while a busy link stays no kind of warning.
> `skipped` and `broken` both leave the value absent and both say nothing about the device, but they are different things to whoever is looking: one is a measurement this platform cannot make, the other is one that should have worked and did not.
> The vocabulary is the one BES software already reports checks in, so an operator meets the same five words here as elsewhere.
> `reason` is free text because the useful part of a failure is the part nobody anticipated: a path, a permission, an errno. A code would carry the half that was foreseen and drop the half worth reading.

### Values

`kind` names what the value is, and its vocabulary is open.

This catalogue uses:

| `kind` | value | unit |
| --- | --- | --- |
| `text` | a string with no numeric meaning | none |
| `fraction` | a number from 0 to 1 inclusive, a proportion of a whole | none |
| `quantity` | a measurement | required |
| `duration` | an elapsed time | `seconds` |
| `datetime` | an instant, as [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339) | none |
| `ipv4`, `ipv6` | an internet address | none |

A `fraction` carries its own scale.

A `quantity` carries none, and a reader has one for it only where its `limits` trait gives it one or another entry in the catalogue is its total.

A `unit` MUST be named in full, and MUST NOT be abbreviated.

A sender MUST round a numeric `value` to at most four decimal places.

> [!NOTE]
> An abbreviated unit is presentation, and an ambiguous one is worse than none: `B/s` and `bps` differ by a factor of eight and are routinely written for each other. How a reader writes the unit, and at what magnitude it shows the value, is the reader's.
> Rounding nearly halves what the values cost compressed, and no reading this protocol carries is meaningful past four places.

## What a device reports

A device MUST report every entry below that its hardware and operating system can answer for.

Where the hardware an entry measures is not fitted, a device MUST omit the entry entirely rather than report it absent.

Where the hardware is fitted and the measurement errored, a device MUST report the entry as `broken`.

Where the hardware is fitted and a precondition for measuring it was not met, a device MUST report the entry as `skipped`.

> [!NOTE]
> A platform that cannot answer for something, or a privilege the device does not hold, is a measurement never attempted rather than one that failed, and an operator chasing a blank tile is served by knowing which.

### Facts

| `fact` | kind | traits | what it reports |
| --- | --- | --- | --- |
| `hostname` | `text` | — | the name the system answers to, from the kernel |
| `board` | `text` | — | the board's model, falling back to the machine's vendor and product |
| `board-revision` | `text` | — | the board's revision, or the machine's version |
| `os` | `text` | — | the operating system and its version |
| `kernel` | `text` | — | the kernel version |
| `last-boot` | `datetime` | — | the instant the device booted |
| `network-address` | `ipv4`, `ipv6` | `interface` | one entry per address held |
| `wireless-network` | `text` | `security`, `channel` | the wireless network the device is joined to |
| `hotspot` | `text` | `channel` | the network the device's hotspot advertises |
| `cpu-frequency-max` | `quantity`, `hertz` | — | the speed the processor is capable of |
| `memory-total` | `quantity`, `bytes` | — | memory fitted |
| `filesystem-total` | `quantity`, `bytes` | `filesystem` | the size of each filesystem |

### Readings

| `measurement` | kind | traits | what it reports |
| --- | --- | --- | --- |
| `cpu-usage` | `fraction` | — | processor in use across all cores |
| `cpu-frequency` | `quantity`, `hertz` | — | the speed the processor is running at |
| `memory-usage` | `fraction` | — | memory in use |
| `filesystem-usage` | `fraction` | `filesystem` | how full each filesystem is |
| `network-throughput` | `quantity`, `bytes/second` | `interface`, `direction` | throughput per interface and direction |
| `hotspot-clients` | `quantity`, `clients` | — | how many clients are joined to the hotspot |
| `temperature` | `quantity`, `celsius` | `sensor` | each temperature sensor |
| `fan-speed` | `quantity`, `revolutions/minute` | `fan` | fan speed |
| `power-source` | `text` | — | where the device's power is coming from |
| `battery-charge` | `fraction` | `battery` | state of charge |
| `battery-voltage` | `quantity`, `volts` | `battery` | cell voltage |
| `battery-direction` | `text` | `battery` | the cell's direction of travel |

### Traits

| trait | members | what it names |
| --- | --- | --- |
| `interface` | `name`, `route`, `overlay` | a network interface; `route` is `default` on the one carrying the default route, and `overlay` names the overlay where it is one |
| `security` | — | how a wireless link is secured |
| `channel` | `number`, `band`, `width` | the channel a wireless link is on; `band` is named as [HOT](network/hotspot.md) names bands, and `width` is in megahertz |
| `direction` | — | `in` or `out` |
| `filesystem` | `mount`, `device`, `role` | a filesystem; `role` is `boot` on a boot partition |
| `sensor` | — | which temperature sensor, of which `cpu` is the processor core |
| `fan` | — | which fan |
| `battery` | `name`, `serial`, `model`, `vendor` | which battery, where a device holds more than one; `name` tells them apart and the rest describe the cell |
| `status` | `is`, `reason` | how the datum stands, as above |
| `limits` | — | marks on the reading's scale, each an object with `at` (number) and `label` (string) |

`route`, `overlay`, `security`, `channel`, a `battery`'s `serial`, `model` and `vendor`, `status` and `limits` are descriptive.
Every other trait distinguishes.

> [!NOTE]
> The default route moves between interfaces, and a reading goes in and out of difficulty constantly. Neither says anything about which thing is being measured, so neither separates one instance of a measurement from another.

## Storage

A device MUST report every filesystem backed by a block device, and MUST NOT report virtual filesystems.

A device MUST report one filesystem per block device, choosing the shortest mount path where several mount the same device.

A device MUST mark a boot partition with the `boot` role.

> [!NOTE]
> Boot partitions are small, written once when the device is imaged, and sit near full for the device's whole life. Marking them is what lets a reader leave them out of a headline without knowing the mount paths a distribution happens to use.

## Network

A device MUST report physical interfaces, wired and wireless, and the overlay the fleet is reached over.

A device MUST NOT report loopback or other virtual interfaces.

A device MUST report throughput as one reading per interface and direction, and MUST NOT aggregate across either.

A device MUST report the wireless network it is joined to, and MUST omit that entry where it is joined to none.

A device MUST report its hotspot and the clients joined to it, and MUST omit both entries where it runs no hotspot.

A device MUST report the channel of its hotspot and of its wireless link both.

What a device joins, and the hotspot it runs, are configured under [NET](network/overview.md).

> [!NOTE]
> An aggregate is a sum a reader can take, and one taken on the device is a figure it cannot break down.

## Power source and battery

`power-source` MUST report one of three values:

| value | meaning |
| --- | --- |
| `via-backup` | external power reaches the device through the backup supply: the ordinary state, and the only one in which a power cut is survived |
| `battery` | external power is absent and the backup supply is carrying the device |
| `bypassing-backup` | external power reaches the device directly, and the backup supply is idle |

A device MUST distinguish the three by a hardware signal reporting whether external power reaches the backup supply, together with the movement of the cell voltage: present is the first, absent with the voltage drifting down is the second, and absent with the voltage entirely static is the third.

A device whose board exposes no such hardware signal MUST omit `power-source` rather than guess at it.

A device MUST report `bypassing-backup` as `warning`, with a reason saying the backup supply is being bypassed.

A device MUST NOT assert `bypassing-backup` until the voltage has been watched long enough to tell drifting from static, and MUST report `battery` until then.

`battery-direction` MUST report one of three values:

| value | meaning |
| --- | --- |
| `charging` | the cell is taking charge |
| `discharging` | the cell is carrying the device |
| `idle` | the cell is doing neither |

A device that measures a battery through a backup supply's own gauge MUST derive `battery-direction` from the movement of the cell voltage, and MUST keep it consistent with `power-source` where that reading exists: `battery` gives `discharging`, `bypassing-backup` gives `idle`, and `via-backup` gives `charging` or `idle` as the cell is taking charge or is full.

A device that reads a battery reported by its operating system MUST take `battery-direction` from the battery's own charging state as the operating system gives it: taking charge is `charging`, carrying the device is `discharging`, and doing neither is `idle`.

Where a device derives `battery-direction` from the cell voltage, it MUST report it as `skipped` until the voltage has been watched long enough to establish it.

Where a device takes `battery-direction` from its operating system and the operating system reports the battery but cannot say which of the three holds, it MUST report `battery-direction` as `skipped`.

A device MUST report `battery-charge`, `battery-voltage` and `battery-direction` once for each battery that powers it, whether fitted inside the device or an external supply carrying it, told apart by the `battery` trait.

A device MUST NOT report a battery that powers a peripheral attached to the device rather than the device itself.

A device MUST name each battery in its `battery` trait, and MUST carry the battery's serial, model and vendor where it holds them.

A device that reports a battery through a backup supply it manages itself MUST supply that battery's name, and MUST name a battery fitted inside the device `built-in`.

A device MUST name each battery it reads from its operating system by the model the operating system reports, and MUST instead use the name the operating system knows the battery as where it reports no model or where two batteries would otherwise share a name.

A device that reads its batteries from its operating system MUST omit `power-source`, since an operating system's report of an external supply cannot tell a device fed through that supply from one fed around it.

Where a battery is fitted but no cell voltage can be read for it, a device MUST report `battery-voltage` as `skipped`, with a reason that no voltage is available.

Where a device has both a backup supply's signal and the movement of the cell voltage, and they disagree, it MUST report `power-source` as the hardware gives it, and MUST report `battery-charge` as `warning`, with a reason saying the cell's direction of travel disagrees with the power source.

> [!NOTE]
> A device fed directly has a charged battery, a backup supply that answers, and no protection at all: removing its power halts it immediately rather than switching it to the battery. Nothing about this is visible from outside the case, and it is the state an operator reaches by plugging into the more obvious of the two inputs.
> The distinction is in the movement and not the level. A cell under load and an idle cell rest at the same voltage at different charges, while an idle cell's voltage does not move at all and a cell carrying the device drifts down continuously.
> State of charge is not the signal here: it does not begin to move until long after the voltage has.
> A device-managed backup supply names its own cell, since nothing else reports one for it, and naming it for sitting inside the case is what tells it from an external supply the same device might also report.
> A backup supply's gauge does not report the cell's direction of travel, so a device working from one derives the direction from the voltage; an operating system reports it directly. Either way the direction is reported plainly and only its absence needs a reason, since a standing note that it was derived would say the same thing on every device for its whole life.

## Temperature and processor speed

`temperature` on the `cpu` sensor MUST carry the board's own declared thresholds as its `limits` trait.

`temperature` MUST be `warning` or `failed` only where the board is in difficulty, not merely warm.

A device MUST report `cpu-frequency` as `warning`, with a reason saying the platform is limiting the processor, where the platform reports that it is.

A device MUST NOT infer throttling from `cpu-frequency` being below `cpu-frequency-max`.

> [!NOTE]
> Carrying the board's own thresholds draws the reading against what the board means by hot, rather than against an invented scale.
> Ordinary idle scaling lowers the frequency, so a device that inferred throttling from the two figures would report it on a device that is simply not busy.

## Sampling

A device MUST begin sampling when it starts and again when a session opens, and MUST stop after thirty minutes during which no session has been open.

A device MUST hold enough recent history of a reading it derives another from to make that derivation.

A device MUST NOT hold readings to send later.

A device MUST choose each reading's update rate to suit what it measures, and a reader MUST NOT assume a fixed interval between readings.

> [!NOTE]
> Sampling before a session opens is what gives the device something current to send the moment one does, and what gives the cell voltage the history its direction of travel is derived from.
> Nothing is kept for replay: a reader that comes back is sent what is current rather than what accumulated while it was away, as [MSG](messages.md) requires.
