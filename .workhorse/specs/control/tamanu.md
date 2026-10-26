---
id: TMU
---

# Tamanu control

A client asks a device to start Tamanu or to restart it, through a Tamanu stream, without the channel or any other stream closing.

The device's state and versions in Tamanu are reported by [NFO](../device-info.md) as `tamanu-state`, `tamanu-web-version` and `tamanu-facility-server-version`.

The Tamanu stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## The acts

| act | effect |
| --- | --- |
| `start` | brings Tamanu up, so that it is `running` as [NFO](../device-info.md) has it |
| `restart` | restarts Tamanu |

A device MUST list an act only where it can carry it out now, by the table below.

| act | listed while `tamanu-state` is |
| --- | --- |
| `start` | `stopped` |
| `restart` | `running` or `degraded` |

A device MUST list no act while `tamanu-state` is any other value, and where it does not report `tamanu-state` at all.

A device MUST refuse every act while it does not report `tamanu-state`.

A client MUST ignore an act in `tamanu-acts` that it does not recognise.

> [!NOTE]
> Which acts are listed follows from the state, so a client holds no table of its own saying what each state allows.
> The set of acts is open. A client that does not recognise an act has nothing to render for it and does not offer it, so a device can list an act a later client learns to word without breaking an earlier one.
> The channel is carried over [CHN](../channel.md) and not over Tamanu, so neither act can strand the operator who asks for it.

## The exchange

A client MUST open a Tamanu stream by opening a stream whose first message is `tamanu`.

A device MUST answer `tamanu` with `tamanu-acts`, carrying as `acts` an array of the acts it can carry out now.

A device MUST send `tamanu-acts` on every open Tamanu stream each time the acts it can carry out change.

A client MUST ask for an act by sending `tamanu-act` on the Tamanu stream, carrying as `ACT` a critical member naming the act, and the act's own parameters.

A device MUST answer every `tamanu-act` exactly once, with `cannot-act` of [MSG](../messages.md) where it will not carry the act out, and otherwise with `tamanu-accepted`.

A device MUST refuse an act it did not list, at `$['act']`.

A device MUST refuse an act asked for while another is under way, on any Tamanu stream, at `$`, with a reason saying an act is under way.

An act is under way from the moment the device accepts it until it has been carried out or has failed.

A device MUST serve any number of Tamanu streams at once, on one channel or across several.

A device MUST carry out an act it accepted once the stream that asked for it has ended, however it ends.

> [!NOTE]
> The selector is critical because each act changes what the device is running, as the `act` of [CTL](power.md) does.
> Every parameter is critical as well, because an act carried out without one it did not read would be a different act.
> Unlike a power act, neither act ends the channel, so there is no `going-away` to send.

## Carrying it out

A device MUST report `tamanu-state` as `starting` from accepting `start`, and as `restarting` from accepting `restart`, until the act has been carried out or has failed, whatever it observes of Tamanu in the meantime.

A device MUST report the state it observes as soon as the act has been carried out or has failed.

An act is carried out when the command that does it has ended without failing and `tamanu-state` has reached `running`.

A device MUST send `tamanu-failed`, carrying `reason`, where it has accepted an act and the command that does it fails or `tamanu-state` does not reach `running`, and MUST NOT send it otherwise.

`reason` MUST say how the command failed, where it did, and which services the act left not running or not as configured, where it left some.

A device MUST send `tamanu-failed` on the stream that asked for the act, and MUST NOT send it on any other.

A device MUST NOT wait indefinitely for an act to be carried out.

> [!NOTE]
> Tamanu may keep serving while a restart happens, so its state alone does not show one under way. The device says so itself with `starting` and `restarting`, on the `default` feed, where every client watching sees it and not only the one that asked.
> A restart begins from `running`, so a state of `running` after it says nothing of whether it happened. The command's own failure is what shows that one which never ran, or gave up part way, did not do what was asked.
> An act that fails part way leaves Tamanu as it stands, which may be some services restarted and some not. The state the device reports once the act ends says which, and `tamanu-failed` says why.

## Logging

A device MUST log every act asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

A device MUST log an act it accepted and did not carry out, with the reason it failed.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `tamanu` | client | a Tamanu stream, as the first message | nothing beyond `type` |
| `tamanu-acts` | device | the Tamanu stream | `acts` |
| `tamanu-act` | client | the Tamanu stream | `ACT` and the act's parameters |
| `tamanu-accepted` | device | the Tamanu stream | nothing beyond `type` |
| `tamanu-failed` | device | the Tamanu stream | `reason` |
| `cannot-act` | device | the Tamanu stream | as [MSG](../messages.md) has it |
