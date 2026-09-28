---
id: CTL
---

# Device control

A client asks a device to restart its software, reboot, or power off, through a control stream, and the device tells every client connected to it before it goes.

The control stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## The acts

| act | effect |
| --- | --- |
| `restart` | stops its bliti software and starts it again, leaving the rest of the system running |
| `reboot` | restarts the whole system |
| `power-off` | shuts the system down and stays off |

A device MUST list an act only where it can carry it out.

> [!NOTE]
> A device whose software nothing starts again once it stops cannot restart, and one without the privilege to shut its system down can neither reboot nor power off.

## The exchange

A client MUST open a control stream by opening a stream whose first message is `control`.

A device MUST answer `control` with `acts`, carrying as `acts` an array of the acts it can carry out.

A client MUST ask for an act by sending `act` on the control stream, carrying as `ACT` a critical member naming the act.

A device MUST answer every `act` exactly once, with `accepted` where it will carry the act out, and otherwise with `refused`.

A device MUST refuse an act it did not list, and every act asked for once it has accepted one, until it has failed to carry that one out.

`refused` MUST carry `reason`, in the device's own words, saying why.

A device MUST serve any number of control streams at once, on one channel or across several.

> [!NOTE]
> An act is a request to do something that cannot be taken back, which is the case [MSG](../messages.md) reserves critical members for: a device that acted on an `act` whose selector it had not read would do something it was not asked to do.
> Several operators may each have a control stream open. The first act accepted is the one carried out, and the refusal each later one receives says so.

## Going away

Once it has accepted an act, a device MUST send `going-away`, carrying as `act` the act accepted, on the `default` feed of every channel where that feed is open, the channel of the client that asked included.

A device MUST end every connection once it has sent `going-away` on each, and MUST then carry the act out.

A device MUST end every connection and carry the act out even where it cannot send `going-away` on some channel.

A device MUST log every act asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

A device MUST log an act it accepted and then failed to carry out, with the reason it failed.

> [!NOTE]
> `going-away` lets every operator watching a device tell a device that is restarting from one that has dropped, and wait for it rather than go looking for a fault.
> A client that has declined `default` is not being looked at, so its channel ends as any other channel ends.
> Ending every connection before acting makes the channel close the same way whichever act follows, rather than on however the system happens to wind down.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `control` | client | a control stream, as the first message | nothing beyond `type` |
| `acts` | device | the control stream | `acts` |
| `act` | client | the control stream | `ACT` |
| `accepted` | device | the control stream | nothing beyond `type` |
| `refused` | device | the control stream | `reason` |
| `going-away` | device | the `default` feed | `act` |
