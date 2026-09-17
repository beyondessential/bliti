---
id: SYS
---

# Device system information

A device reports what it knows about itself: what board it is, what it is running, and how it is doing.
The application displays that to an operator standing in front of it, who has no other way to reach the device.

This spec defines the readings a device reports, the format it reports them in, the topic they are subscribed to, and what the application does with them.
It inherits the envelope of [MSG](messages.md) entire and restates none of it.

The reading format is wire contract.
A client written from this spec alone renders every reading a conforming device sends, including readings that did not exist when the client was written.

## A reading

A reading is a JSON object.
Every message in this spec carries readings and nothing else of substance.

| member | type | required | meaning |
| --- | --- | --- | --- |
| `name` | string | yes | stable identifier, lower case with hyphens |
| `label` | string | yes | what to call the reading in an interface |
| `value` | object | no | the headline value, shaped as below |
| `detail` | array | no | further values, each an object with `label` and `value` |
| `note` | string | no | plain prose about what the reading means |
| `state` | string | yes | `ok`, `warn` or `fault` |
| `limits` | array | no | marks on the reading's scale, each an object with `at` (number) and `label` (string) |
| `group` | string | no | name tying this reading to others for display |
| `direction` | string | no | `in` or `out`, for a reading that measures flow |
| `graph` | boolean | yes | whether the reading's history is worth drawing |
| `error` | string | no | why the reading could not be taken |

`state` and `graph` are always sent. A reading carries no rule a reader must know before it can be read: where a member is absent, nothing is what that means.

A reading MUST carry either `value` or `error`.
A reading carrying `error` MUST have `state` of `fault` and MUST NOT carry `value`.

`name` identifies the reading across versions and MUST NOT be reused for a different quantity.
`label` is prose and MAY change between versions.

## A value

A value is a JSON object carrying `kind` and the members that kind requires.

| `kind` | members | meaning |
| --- | --- | --- |
| `fraction` | `number` | a number from 0 to 1 inclusive, a proportion of a whole |
| `quantity` | `number`, `unit`, optionally `max` | a measurement, `unit` a string, `max` the top of its scale |
| `duration` | `seconds` | an elapsed time |
| `text` | `text` | a string with no numeric meaning |

A receiver that does not recognise a `kind` treats the value as unreadable and the reading as carrying none, and renders the reading's `label` alone.

`fraction` and a `quantity` carrying `max` have a scale, and a client MAY draw one against it.
A `quantity` without `max` has no ceiling and MUST NOT be drawn against one.

## Static readings

The device sends `system-identity` on its reporting stream as soon as it has named itself, and again whenever what it reports changes.

| member | type | meaning |
| --- | --- | --- |
| `readings` | array | readings that do not change while the device runs, or change rarely |

The device's own software name and version are not repeated here; they are carried by `device-hello` in [MSG](messages.md).

## Live readings

Live readings are carried on the topic `system`.

On receiving the subscription the device sends the newest value of every reading it holds as a `system-sample`, then `system-history`, then a `system-sample` at its own cadence for as long as the stream is open.

The readings come before the window so that everything an operator is waiting for appears at once and the graphs fill in behind it. The other way round leaves the view empty for as long as the window takes to cross the link, which is the part of it nobody is waiting on.

`system-sample`:

| member | type | meaning |
| --- | --- | --- |
| `at` | number | milliseconds since the device booted, when the sample was taken |
| `readings` | array | the readings, as above |

`system-history`:

| member | type | meaning |
| --- | --- | --- |
| `series` | array | the past values, one entry per reading that has any |

Each series is an object:

| member | type | meaning |
| --- | --- | --- |
| `name` | string | the reading these are the past values of |
| `points` | array | each point as a two-element array of the time it was taken and the value then, in ascending order of time |

The window carries numbers and nothing else. A reading's description reaches a client with the live
samples, and repeating it against every past point costs far more than the numbers do: the whole
window is one message sent the instant a client subscribes, and the link it crosses is BLE.

A device MAY send fewer points than it holds, spread across the window, where it holds more than a
graph can draw. It MUST keep the newest, so the end of a graph is where the reading is rather than
wherever the spread fell.

A sample need not carry every reading, and the set MAY differ between samples.

A reading whose value is not a number has no series, because there is no graph to draw.

Times are measured from boot rather than from an epoch, because a device in the field may have no set clock and no way to reach one.
A client MUST treat `at` as meaningful only relative to other `at` values from the same device.

## Sampling and history

The device samples into a buffer holding about five minutes at full resolution, discarding oldest first and never coarsening.

Sampling begins when the device starts and begins again when a session opens.
It stops after thirty minutes during which no session has been open.

A `system-history` sent on subscription carries whatever the buffer holds, which MAY be empty.

Readings update at a rate suited to what they measure: those that move quickly often enough to read as live, those that move slowly every few seconds.
The device chooses these rates and a client MUST NOT assume a fixed interval between samples.

## The readings a device reports

A device reports every reading below that its hardware and operating system can answer for.

Where the hardware a reading measures is not fitted, the device MUST omit the reading entirely rather than report it absent.
Where the hardware is fitted and the reading cannot be taken, the device MUST send the reading carrying `error`.

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

A device reports how full its fullest filesystem is, with every filesystem behind it.
Several mount points on one block device count once, not once each.

A boot partition does not set the headline.
Those are small, written once when the device is imaged, and sit near full for its whole life, so one setting the headline would show every device as nearly out of space.

Filesystem use does not move fast enough for a graph to say anything, so the reading says its history is not worth drawing.

### Power source and battery

`power-source` reports where the device's power is coming from, as one of three states:

| state | meaning |
| --- | --- |
| external power through the backup supply | the ordinary state, and the only one in which a power cut is survived |
| the battery | external power is absent and the backup supply is carrying the device |
| external power bypassing the backup supply | the device is fed directly and the backup supply is idle |

The third state MUST be reported as `warn`, and its `note` MUST say that a power cut will stop the device without warning and that moving the supply to the backup's own input restores protection.

The third state MUST NOT be asserted until the voltage has been watched long enough to tell drifting from static. Until then the device reports the second.

A device fed directly has a charged battery, a backup supply that answers, and no protection at all: removing its power halts it immediately rather than switching it to the battery.
Nothing about this is visible from outside the case, and it is the state an operator reaches by plugging into the more obvious of the two inputs.

The battery headline is state of charge.
Voltage and the direction of travel go under `detail`.

`power-source` requires a hardware signal reporting whether external power reaches the backup supply. A device whose board exposes no such signal MUST omit the reading rather than guess at it, and its battery reading carries a derived direction alone.

The three states are distinguished by that signal together with the movement of the cell voltage: present is the first, absent with the voltage drifting down is the second, and absent with the voltage entirely static is the third.

The distinction is in the movement and not the level. A cell under load and an idle cell rest at the same voltage at different charges, so the level separates neither. An idle cell's voltage does not move at all, while a cell carrying the device drifts down continuously.

State of charge is not the signal here. It does not begin to move until long after the voltage has, and a device that waited for it would report the wrong state for the first minute of every outage.

Direction is taken from `power-source` where that reading exists, and is derived from the movement of the cell voltage across the buffered history otherwise.
A device that derives it MUST say so in `note`, and MUST NOT report a derived direction it does not yet have enough history to establish.

Where a device has both signals and they disagree, it reports `power-source` as the hardware gives it and sets the battery reading to `warn`, with a `note` saying the cell is moving against the reported source.
External power reported as present while the cell drains is a fault in the supply or in the board's own sensing, and an operator cannot see either from outside the case.

### Temperature and throttling

`limits` carries the board's own declared thresholds, so the reading is drawn against what the board means by hot rather than against an invented scale.

`temperature` is `warn` or `fault` only where the board is in difficulty, not merely warm.
Its `note` states that the figure is the processor core rather than the case or the ambient air, and gives the range that is ordinary under load.
This note is required: without it the number is routinely read as a fault on a device that is working correctly.

`throttling` reports the conditions the platform can establish: the supply voltage being low, and the processor running below the speed it is capable of.

### Network

A device reports physical interfaces, wired and wireless, and the overlay the fleet is reached over.
It MUST NOT report loopback, and MUST NOT report other virtual interfaces.

## What the application does with it

The application renders every reading it receives, whether or not it recognises the `name`.
Rendering from `label`, `value`, `detail`, `note`, `state` and `limits` alone is the floor, and is what lets a device that has gained a reading appear in an application that predates it.

Where the application recognises a `name` it MAY render that reading specially.
Recognition is never a precondition for display.

The application MUST NOT report a reading as missing.
It has no list of expected readings to compare against, and a reading the device never declared is not an absence it can observe.

A reading carrying `error` is shown as failing, with the reason.

### The face and the reveal

Each reading is shown as a tile.

The tile's face carries the reading's `label` and its headline value and nothing else.
A tile whose detail is open takes the full width, because figures and graphs are not read through half a column.
`detail`, `note`, `limits`, any scale drawn as a bar, and any history drawn as a graph are revealed when the operator taps the tile.

A reading whose `state` is `warn` or `fault` is coloured on the face.
The face gains no further element to carry that.

Readings sharing a `group` are shown as one tile.

### Graphs

Where the application holds history for a reading, and the reading does not say its history is not worth drawing, the reveal shows it as a graph.
The application holds history from `system-history` on subscribing, and extends it with each `system-sample`.

Two readings in one group carrying opposed `direction` values are drawn as a single graph mirrored about a shared time axis, one direction above it and the other below.
Each direction is scaled to its own peak, and each peak is stated beside it, along with which line it belongs to.
A graph with two lines and no way to tell them apart is not readable.

Samples are spaced by their `at` values rather than evenly.

### Subscribing

The application subscribes to `system` while the operator is looking at the device, and drops the subscription when the page is hidden, as [MSG](messages.md) requires.

History received on resubscribing covers the gap, so a graph is continuous across it.
