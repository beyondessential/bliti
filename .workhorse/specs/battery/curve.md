---
id: CRV
---

# Battery curve

The curves of [CHG](charge.md) as a document an operator can read out of a device, load into one, or reset, over the channel or at the device.

## The curve document

A curve document is a JSON object:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `discharging` | object | yes | the discharging curve |
| `charging` | object | no | the charging curve, where the device holds one |

Each curve is a JSON object:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `points` | array | yes | the curve, each point an array of the cell voltage in volts and the charge at it |
| `learnt-from` | number | yes | how many runs or charges the device has refined the curve from, 0 for a curve as a build carries it |
| `error` | number | yes | the curve's error, as [CHG](charge.md) holds it |
| `duration` | number | yes | the curve's duration in seconds, as [CHG](charge.md) holds it |

`points` MUST hold at least two points, ordered by rising voltage, with no two at one voltage.

A point's charge MUST be from 0 to 1 inclusive, on the scale of [CHG](charge.md), and MUST NOT fall as the voltage rises.

The discharging curve's first point MUST be at charge 0 and at or below the floor of [LOW](shutdown.md), and its last point MUST be at charge 1.

`learnt-from` MUST be a whole number, `error` MUST be from 0 to 1 inclusive, and `duration` MUST be greater than 0.

A sender MUST round every number in a curve document to at most four decimal places.

A device MUST refuse to load a document that breaks any of the above, with a reason saying what is wrong.

> [!NOTE]
> A curve is a set of points rather than a formula so that one learnt on a device can be read, compared against another, and loaded into a device carrying a different cell as it stands.

## Loading and resetting

A device MUST replace both of its curves with those of a document it loads, holding no charging curve where the document carries none, and MUST go on refining them under [CHG](charge.md) from there.

A device MUST reset by returning to the discharging curve its build carries and holding no charging curve.

## The curve stream

The curve stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

A client MUST open a curve stream by opening a stream whose first message is `curve`.

A device MUST answer `curve` with `curves`, carrying as `document` the curve document in force for the backup supply it manages, and carrying no `document` where it manages none.

Alongside `document`, `curves` MUST carry as `lasts` how long a full charge lasts, and as `recharge` how long a full recharge takes where the device holds a charging curve, each as [CHG](charge.md) gives it, and each an object:

| member | type | required | meaning |
| --- | --- | --- | --- |
| `duration` | number | yes | the time, in seconds |
| `margin` | number | yes | how far either way the time may be off, in seconds |

A client MUST ask to load a document by sending `load` on the curve stream, carrying the document as `document`, and MUST ask to reset by sending `reset`.

A device MUST answer every `load` and `reset` exactly once, with `accepted` where it has done what was asked, and otherwise with `refused`.

A device that manages no backup supply MUST refuse every `load` and `reset`.

`refused` MUST carry `reason`, in the device's own words, saying why.

A device MUST send `curves` on every open curve stream each time its curve document changes, whether by a load, a reset or a refinement.

A device MUST serve any number of curve streams at once, on one channel or across several.

A device MUST log every `load` and `reset` asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

> [!NOTE]
> A device that has answered `curve` has shown it knows the stream, so `load` and `reset` need no critical member to keep an older device from passing over them.

## At the device

A device's command line MUST offer to write the curve document in force to its standard output, to load one from a file or from its standard input, and to reset.

A load or reset from the command line MUST take effect as one over a curve stream does, in a running daemon without restarting it.

A device MUST log every load and reset made from its command line, and MUST report a refusal's reason on its standard error.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `curve` | client | a curve stream, as the first message | nothing beyond `type` |
| `curves` | device | the curve stream | `document` and `lasts`, where the device manages a backup supply; `recharge`, where it also holds a charging curve |
| `load` | client | the curve stream | `document` |
| `reset` | client | the curve stream | nothing beyond `type` |
| `accepted` | device | the curve stream | nothing beyond `type` |
| `refused` | device | the curve stream | `reason` |
