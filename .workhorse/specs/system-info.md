---
id: SYS
---

# Device system information

A device reports what board it is, what it is running, and how it is doing, within the envelope of [MSG](messages.md).

## Borrowed terms

| term | meaning |
| --- | --- |
| state of charge | How full a cell is, as a proportion of its capacity. Not whether it is charging or discharging, which is its direction of travel. |
| overlay | A virtual network built across whatever physical links a device has, over which a fleet is reached. |
| throttling | A platform limiting its own processor, typically for heat or for supply voltage. |

## A reading

A reading MUST be a JSON object:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `name` | string | yes | stable identifier, lower case with hyphens |
| `label` | string | yes | what to call the reading in an interface |
| `state` | string | yes | `ok`, `warn` or `fault` |
| `graph` | boolean | yes | whether the reading's history is worth drawing |
| `value` | object | no | the headline value, shaped as below |
| `detail` | array | no | further values, each an object with `label` and `value` |
| `note` | string | no | plain prose about what the reading means |
| `limits` | array | no | marks on the reading's scale, each an object with `at` (number) and `label` (string) |
| `group` | string | no | name tying this reading to others for display |
| `direction` | string | no | `in` or `out`, for a reading that measures flow |
| `error` | string | no | why the reading could not be taken |

A reading MUST carry either `value` or `error`.

A reading carrying `error` MUST have `state` of `fault` and MUST NOT carry `value`.

A `name` MUST NOT be reused for a different quantity. A `label` MAY change between versions.

Where a member is absent, nothing is what that means.

> [!NOTE]
> `state` and `graph` are sent every time rather than defaulted, so a reading carries no rule a reader has to know before it can be read.
> `name` is what identifies a reading across versions; `label` is prose.

## A value

A value MUST be a JSON object carrying `kind` and the members that kind requires:

| `kind` | members | meaning |
| --- | --- | --- |
| `fraction` | `number` | a number from 0 to 1 inclusive, a proportion of a whole |
| `quantity` | `number`, `unit`, optionally `max` | a measurement, `unit` a string, `max` the top of its scale |
| `duration` | `seconds` | an elapsed time |
| `text` | `text` | a string with no numeric meaning |

A receiver that does not recognise a `kind` MUST treat the value as unreadable and the reading as carrying none, and MUST render the reading's `label` alone.

A client MAY draw a `fraction`, or a `quantity` carrying `max`, against its scale.

A client MUST NOT draw a `quantity` without `max` against a scale.

## Static readings

A device MUST send `system-identity` on its reporting stream as soon as it has named itself, and again whenever what it reports changes.

| member | type | meaning |
| --- | --- | --- |
| `readings` | array | readings that do not change while the device runs, or change rarely |

> [!NOTE]
> The device's own software name and version are not repeated here. They are carried by `device-hello` in [MSG](messages.md).

## Live readings

Live readings MUST be carried on the topic `system`.

On receiving the subscription a device MUST send the newest value of every reading it holds as a `system-sample`, then a `system-history`, then a `system-sample` at its own cadence for as long as the stream is open.

`system-sample`:

| member | type | meaning |
| --- | --- | --- |
| `at` | number | milliseconds since the device booted, when the sample was taken |
| `readings` | array | the readings, as above |

`system-history`:

| member | type | meaning |
| --- | --- | --- |
| `series` | array | the past values, one entry per reading that has any |

Each series MUST be an object:

| member | type | meaning |
| --- | --- | --- |
| `name` | string | the reading these are the past values of |
| `points` | array | each point a two-element array of the time it was taken and the value then, in ascending order of time |

A sample need not carry every reading, and the set MAY differ between samples.

A reading whose value is not a number has no series.

A device MAY send fewer points than it holds, spread across the window, and MUST keep the newest.

A client MUST treat `at` as meaningful only relative to other `at` values from the same device.

> [!NOTE]
> Times are measured from boot rather than from an epoch because a device in the field may have no set clock and no way to reach one.
> Keeping the newest point is what puts the end of a graph where the reading is, rather than wherever the spread fell.
> A series carries numbers and nothing else: a reading's description reaches a client with the live samples, and repeating it against every past point would cost far more than the numbers do over BLE.

## Sampling and history

A device MUST sample into a buffer holding about five minutes at full resolution, discarding oldest first and never coarsening.

A device MUST begin sampling when it starts and again when a session opens, and MUST stop after thirty minutes during which no session has been open.

A `system-history` sent on subscription MUST carry whatever the buffer holds, which MAY be empty.

A device MUST choose each reading's update rate to suit what it measures, and a client MUST NOT assume a fixed interval between samples.

## The readings a device reports

A device MUST report every reading below that its hardware and operating system can answer for.

Where the hardware a reading measures is not fitted, a device MUST omit the reading entirely rather than report it absent.

Where the hardware is fitted and the reading cannot be taken, a device MUST send the reading carrying `error`.

Static:

| `name` | what it reports |
| --- | --- |
| `hostname` | the name the system answers to, from the kernel |
| `board` | the board's model and revision, falling back to the machine's vendor, product and version |
| `os` | the operating system and its version |
| `address` | one reading per interface holding an address, grouped as `network` |

Live:

| `name` | value | notes |
| --- | --- | --- |
| `cpu` | `fraction` | processor in use across all cores |
| `memory` | `fraction` | memory in use, with total and used under `detail` |
| `disk` | `fraction` | how full the fullest filesystem is, with each behind it |
| `battery` | `fraction` | state of charge, with voltage and direction of travel under `detail` |
| `temperature` | `quantity` | the processor core, with any further sensors under `detail` |
| `power-source` | `text` | where the device's power is coming from |
| `throttling` | `text` | what is currently limiting the board |
| `fan` | `quantity` | fan speed |
| `uptime` | `duration` | time since boot |
| `network-in`, `network-out` | `quantity` | throughput summed across interfaces per direction, grouped as `network`, `direction` set accordingly |

### Disk

A device MUST report how full its fullest filesystem is, with every filesystem under `detail`.

A device MUST count several mount points on one block device once.

A device MUST NOT let a boot partition set the headline.

The `disk` reading MUST say its history is not worth drawing.

> [!NOTE]
> Boot partitions are small, written once when the device is imaged, and sit near full for the device's whole life, so one setting the headline would show every device as nearly out of space.
> Filesystem use does not move fast enough for a graph to say anything.

### Power source and battery

`power-source` MUST report one of three states:

| state | meaning |
| --- | --- |
| external power through the backup supply | the ordinary state, and the only one in which a power cut is survived |
| the battery | external power is absent and the backup supply is carrying the device |
| external power bypassing the backup supply | the device is fed directly and the backup supply is idle |

A device MUST distinguish the three by a hardware signal reporting whether external power reaches the backup supply, together with the movement of the cell voltage: present is the first, absent with the voltage drifting down is the second, and absent with the voltage entirely static is the third.

A device whose board exposes no such hardware signal MUST omit `power-source` rather than guess at it.

A device MUST report the third state as `warn`, and its `note` MUST say that a power cut will stop the device without warning and that moving the supply to the backup's own input restores protection.

A device MUST NOT assert the third state until the voltage has been watched long enough to tell drifting from static, and MUST report the second until then.

The `battery` headline MUST be state of charge, with voltage and direction of travel under `detail`.

A device MUST take direction from `power-source` where that reading exists, and MUST derive it from the movement of the cell voltage across the buffered history otherwise.

A device that derives direction MUST say so in `note`, and MUST NOT report a derived direction it does not yet have enough history to establish.

Where a device has both signals and they disagree, it MUST report `power-source` as the hardware gives it, and MUST set the `battery` reading to `warn` with a `note` saying the cell is moving against the reported source.

> [!NOTE]
> A device fed directly has a charged battery, a backup supply that answers, and no protection at all: removing its power halts it immediately rather than switching it to the battery. Nothing about this is visible from outside the case, and it is the state an operator reaches by plugging into the more obvious of the two inputs.
> The distinction is in the movement and not the level. A cell under load and an idle cell rest at the same voltage at different charges, so the level separates neither, while an idle cell's voltage does not move at all and a cell carrying the device drifts down continuously.
> State of charge is not the signal here: it does not begin to move until long after the voltage has, and a device that waited for it would report the wrong state for the first minute of every outage.
> External power reported as present while the cell drains is a fault in the supply or in the board's own sensing, and an operator cannot see either from outside the case.

### Temperature and throttling

`temperature` MUST carry the board's own declared thresholds in `limits`.

`temperature` MUST be `warn` or `fault` only where the board is in difficulty, not merely warm.

The `temperature` `note` MUST state that the figure is the processor core rather than the case or the ambient air, and MUST give the range that is ordinary under load.

`throttling` MUST report the conditions the platform can establish: the supply voltage being low, and the processor running below the speed it is capable of.

> [!NOTE]
> Carrying the board's own thresholds draws the reading against what the board means by hot, rather than against an invented scale.
> Without the note, the number is routinely read as a fault on a device that is working correctly.

### Network

A device MUST report physical interfaces, wired and wireless, and the overlay the fleet is reached over.

A device MUST NOT report loopback or other virtual interfaces.

## What an application does with it

An application MUST render every reading it receives, whether or not it recognises the `name`.

An application MAY render a reading it recognises specially, and MUST NOT make recognition a precondition for display.

An application MUST NOT report a reading as missing.

An application MUST show a reading carrying `error` as failing, with the reason.

> [!NOTE]
> Rendering from `label`, `value`, `detail`, `note`, `state` and `limits` alone is the floor, and is what lets a device that has gained a reading appear in an application that predates it.
> An application has no list of expected readings to compare against, so a reading a device never declared is not an absence it can observe.

### The face and the reveal

An application MUST show each reading as a tile.

A tile's face MUST carry the reading's `label` and its headline value, and nothing else.

An application MUST reveal `detail`, `note`, `limits`, any scale drawn as a bar, and any history drawn as a graph when the operator opens the tile.

An application MUST colour the face of a reading whose `state` is `warn` or `fault`, and MUST NOT add a further element to the face to carry that.

An application MUST show readings sharing a `group` as one tile.

> [!NOTE]
> An open tile wants the full width, because figures and graphs are not read through half a column.

### Graphs

Where an application holds history for a reading whose `graph` is true, it MUST show that history as a graph in the reveal.

An application MUST hold history from `system-history` on subscribing, and MUST extend it with each `system-sample`.

An application MUST draw two readings in one group carrying opposed `direction` values as a single graph mirrored about a shared time axis, one direction above it and the other below.

An application MUST scale each direction to its own peak, and MUST state each peak beside its line together with which line it belongs to.

An application MUST space samples by their `at` values rather than evenly.

> [!NOTE]
> A graph with two lines and no way to tell them apart is not readable.

### Subscribing

An application MUST subscribe to `system` while the operator is looking at the device, and MUST drop the subscription when they are not, as [MSG](messages.md) requires.

> [!NOTE]
> History received on resubscribing covers the gap, so a graph is continuous across it.
