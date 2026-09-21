---
id: NFO
---

# Device information

A device reports what board it is, what it is running, and how it is doing, as the facts and readings of [MSG](messages.md).

This spec is the catalogue: which entries a device reports, what each is about, and which of them an application renders specially.

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
A `reading` is a measurement, which may be in difficulty and whose history is worth keeping.

The two are one shape.
What separates them is which catalogue names them, and that only a `reading` may be in difficulty.

| member | type | required | meaning |
| --- | --- | --- | --- |
| `at` | number | yes | milliseconds since the sender booted, when it was taken |
| `fact` or `measurement` | string | yes | what it is, named against the catalogue below; `fact` on a `fact`, `measurement` on a `reading` |
| `traits` | object | no | what it is about, as below |
| `kind` | string | yes | what the value is |
| `unit` | string | no | the unit the value is in |
| `value` | any | no | the value |
| `error` | string | no | why there is none |

Either MUST carry `value` or `error`, and MUST NOT carry both.

Whether something is in difficulty, and the scale it is drawn against, are traits.

A `fact` MUST NOT carry the `state` trait.

A message carrying `error` MUST carry the `state` trait, with `is` of `fault`.

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

### What identifies a series

A fact or reading's identity MUST be its catalogue name together with its traits.

A receiver MUST treat every trait as part of that identity, except one it knows to be descriptive.

A receiver MUST treat a trait it does not recognise as part of that identity.

> [!NOTE]
> Identity is the receiver's to compute, and two receivers at different versions may reach different answers about the same data. That is deliberate: a sender newer than its receiver may add a trait that splits one series into several, and a receiver that left unrecognised traits out of identity would merge them and draw one series that is wrong. Treating an unknown trait as distinguishing leaves it two series it cannot fully tell apart, which is degraded and true, and it stops splitting them when it learns better.

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

An application MAY draw a `fraction` against its scale.

An application MUST NOT draw a `quantity` against a scale unless its `limits` trait gives it one.

A receiver that does not recognise a `kind` MUST render the stringification of `value`, followed by `unit` where there is one.

A `unit` MUST be named in full, and MUST NOT be abbreviated.

A receiver MUST choose for itself how to write a unit and at what magnitude to show a value.

A sender MUST round a numeric `value` to at most four decimal places.

> [!NOTE]
> An abbreviated unit is presentation, and an ambiguous one is worse than none: `B/s` and `bps` differ by a factor of eight and are routinely written for each other.
> Rounding nearly halves what the values cost compressed, and no reading this protocol carries is meaningful past four places.

## What a device reports

A device MUST report every entry below that its hardware and operating system can answer for.

Where the hardware an entry measures is not fitted, a device MUST omit the entry entirely rather than report it absent.

Where the hardware is fitted and the measurement cannot be taken, a device MUST send the entry carrying `error`.

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
| `temperature` | `quantity`, `celsius` | `sensor` | each temperature sensor |
| `fan-speed` | `quantity`, `revolutions/minute` | `fan` | fan speed |
| `power-source` | `text` | — | where the device's power is coming from |
| `battery-charge` | `fraction` | — | state of charge |
| `battery-voltage` | `quantity`, `volts` | — | cell voltage |
| `battery-direction` | `text` | — | the cell's direction of travel |

### Traits

| trait | members | what it names |
| --- | --- | --- |
| `interface` | `name`, `route`, `overlay` | a network interface; `route` is `default` on the one carrying the default route, and `overlay` names the overlay where it is one |
| `direction` | — | `in` or `out` |
| `filesystem` | `mount`, `device`, `role` | a filesystem; `role` is `boot` on a boot partition |
| `sensor` | — | which temperature sensor, of which `cpu` is the processor core |
| `fan` | — | which fan |
| `state` | `is`, `reason` | that the reading is in difficulty: `is` is `warn` or `fault`, and `reason` names what the trouble is |
| `limits` | — | marks on the reading's scale, each an object with `at` (number) and `label` (string) |

A device MUST carry the `state` trait only where the measurement has a notion of being in difficulty, and MUST NOT carry it to say that nothing is wrong.

An application MUST treat `route`, `overlay`, `state` and `limits` as descriptive.

> [!NOTE]
> The default route moves between interfaces, and a reading goes in and out of difficulty constantly. An application that let either distinguish would start a new series each time, forking a graph for a reason that has nothing to do with what it measures.
> Throughput has no notion of being in difficulty: a link is not doing badly by being busy. Sending `ok` against it would be answering a question the measurement does not ask.

## Storage

A device MUST report every filesystem backed by a block device, and MUST NOT report virtual filesystems.

A device MUST report one filesystem per block device, choosing the shortest mount path where several mount the same device.

A device MUST mark a boot partition with the `boot` role.

> [!NOTE]
> Boot partitions are small, written once when the device is imaged, and sit near full for the device's whole life. Marking them is what lets an application leave them out of a headline without knowing the mount paths a distribution happens to use.
> Filesystem use does not move fast enough over a five-minute window for a graph to say anything, which is why an application does not draw one.

## Network

A device MUST report physical interfaces, wired and wireless, and the overlay the fleet is reached over.

A device MUST NOT report loopback or other virtual interfaces.

A device MUST report throughput as one reading per interface and direction, and MUST NOT aggregate across either.

> [!NOTE]
> An aggregate is a sum an application can take, and one taken on the device is a figure it cannot break down.

## Power source and battery

`power-source` MUST report one of three states:

| state | meaning |
| --- | --- |
| external power through the backup supply | the ordinary state, and the only one in which a power cut is survived |
| the battery | external power is absent and the backup supply is carrying the device |
| external power bypassing the backup supply | the device is fed directly and the backup supply is idle |

A device MUST distinguish the three by a hardware signal reporting whether external power reaches the backup supply, together with the movement of the cell voltage: present is the first, absent with the voltage drifting down is the second, and absent with the voltage entirely static is the third.

A device whose board exposes no such hardware signal MUST omit `power-source` rather than guess at it.

A device MUST report the third state with a `state` trait of `warn` and a reason of `backup-bypassed`.

A device MUST NOT assert the third state until the voltage has been watched long enough to tell drifting from static, and MUST report the second until then.

A device MUST take `battery-direction` from `power-source` where that reading exists, and MUST derive it from the movement of the cell voltage otherwise.

A device that derives the direction MUST say so with a `state` trait reason of `derived`, and MUST NOT report a derived direction it does not yet have enough history to establish.

Where a device has both signals and they disagree, it MUST report `power-source` as the hardware gives it, and MUST give `battery-charge` a `state` trait of `warn` with a reason of `against-source`.

> [!NOTE]
> A device fed directly has a charged battery, a backup supply that answers, and no protection at all: removing its power halts it immediately rather than switching it to the battery. Nothing about this is visible from outside the case, and it is the state an operator reaches by plugging into the more obvious of the two inputs.
> The distinction is in the movement and not the level. A cell under load and an idle cell rest at the same voltage at different charges, while an idle cell's voltage does not move at all and a cell carrying the device drifts down continuously.
> State of charge is not the signal here: it does not begin to move until long after the voltage has.

## Temperature and processor speed

`temperature` on the `cpu` sensor MUST carry the board's own declared thresholds as its `limits` trait.

`temperature` MUST be `warn` or `fault` only where the board is in difficulty, not merely warm.

A device MUST give `cpu-frequency` a `state` trait of `warn` with a reason of `throttled` where the platform reports that it is limiting the processor.

A device MUST NOT infer throttling from `cpu-frequency` being below `cpu-frequency-max`.

> [!NOTE]
> Carrying the board's own thresholds draws the reading against what the board means by hot, rather than against an invented scale.
> Ordinary idle scaling lowers the frequency, so a device that inferred throttling from the two figures would report it on a device that is simply not busy.

## Sampling

A device MUST sample into a buffer holding about five minutes at full resolution, discarding oldest first and never coarsening.

A device MUST begin sampling when it starts and again when a session opens, and MUST stop after thirty minutes during which no session has been open.

A device MUST choose each reading's update rate to suit what it measures, and an application MUST NOT assume a fixed interval between readings.

## What an application does with it

An application MUST render every fact and reading it receives, whether or not it recognises the catalogue name.

An application MUST NOT report a fact or reading as missing.

An application MUST show one carrying `error` as failing, with the reason.

> [!NOTE]
> An application has no list of expected entries to compare against, so one a device never sent is not an absence it can observe.

### What it recognises

An application MUST hold its own order, and MUST NOT take one from the order data arrives in.

An application MUST render in this order:

| position | from |
| --- | --- |
| a header naming the device | `hostname`, `board` with `board-revision`, `os` |
| tiles | `network-address`, `cpu-usage`, `memory-usage`, `filesystem-usage`, `network-throughput`, `temperature`, `cpu-frequency`, `fan-speed`, `power-source`, `battery-charge`, `last-boot` |
| appended | everything it does not recognise |

An application MUST NOT reorder by the `state` trait.

An application MUST supply its own wording for every catalogue name, trait, trait value and unit it recognises.

An application MUST render `last-boot` as an elapsed time.

> [!NOTE]
> A layout that rearranged while an operator was looking at it would cost the screen its familiarity, and a device with several marginal readings would reshuffle as they crossed back and forth. Trouble is found by colour instead.

### What it does not

An application MUST render a catalogue name it does not recognise from the name itself, with the values of its traits as a qualifier, its value drawn by its `kind`, and `unit` where there is one.

An application MUST render the traits of an entry it does not recognise, and MUST NOT drop them.

An application MUST render an unrecognised fact as a tile rather than in the header.

> [!NOTE]
> Two entries sharing a catalogue name and differing only in traits would otherwise appear as two tiles under one label with different values and nothing to tell them apart.

### The face and the reveal

An application MUST show each tile's face carrying a label and a headline value, and nothing else.

An application MUST reveal what is behind the headline, any scale drawn as a bar, and any history drawn as a graph, when the operator opens the tile.

An application MUST colour the face of an entry carrying a `state` trait, and MUST NOT add a further element to the face to carry that.

An application MUST show every reading in a reveal with its own limits, state reason and error reason.

> [!NOTE]
> An open tile wants the full width, because figures and graphs are not read through half a column.

### Aggregates

An application MUST headline `filesystem-usage` with the fullest filesystem whose `filesystem` trait does not carry the `boot` role, and MUST show each filesystem in the reveal.

An application MUST headline `network-throughput` with the sum of every direction and interface, and MUST show each interface in the reveal.

An application MUST headline `temperature` with the `cpu` sensor, and MUST show every sensor in the reveal.

An application MUST headline `network-address` with the address whose `interface` carries the `default` route, together with the one whose `interface` names an overlay, and MUST show every address in the reveal.

### Graphs

An application MUST hold a history for each `reading` it receives, keyed by identity as [MSG](messages.md) defines it, and MUST NOT hold one for a `fact`.

An application MUST show that history as a graph in the reveal.

An application MUST NOT draw a history for `filesystem-usage`.

An application MUST draw the two directions of one interface's `network-throughput` as a single graph mirrored about a shared time axis, one direction above it and the other below.

An application MUST scale each direction to its own peak, and MUST state each peak beside its line together with which line it belongs to.

An application MUST space readings by their `at` values rather than evenly.

> [!NOTE]
> Two directions of throughput routinely differ by an order of magnitude, and a shared scale flattens the quieter one to a line.

### Subscribing

An application MUST let the device's feed run while the operator is looking at the device.

An application SHOULD close the feed when they are not, and MUST subscribe to `default` to resume.
