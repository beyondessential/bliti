---
id: SVC
---

# Service control

A client asks a device to restart one of the services it runs, through a service stream, without the channel or any other stream closing.

The service stream is a stream role beyond those of [MSG](../messages.md), established as [MSG](../messages.md) requires by which end opened the stream and by its first message.

## The services

A device restarts the services it reports under [NFO](../device-info.md), named as the `name` member of their `service` trait names them.

A device MUST offer to restart a service only where it can stop it and start it again without rebooting, and only where it reports that service under [NFO](../device-info.md).

> [!NOTE]
> Restarting a service is a local act on the device. The channel is carried over [CHN](../channel.md), which a service coming and going does not touch, so the operator watches the service return on the feed rather than losing the device as a power act of [CTL](power.md) loses it.

## The exchange

A client MUST open a service stream by opening a stream whose first message is `services`.

A device MUST answer `services` with `restartable`, carrying as `services` an array of the names of the services it can restart, each as its `service` trait names it.

A client MUST ask for a restart by sending `restart` on the service stream, carrying as `SERVICE` a critical member naming the service.

A device MUST answer every `restart` exactly once, with `restarting` where it will restart the service, and otherwise with `restart-refused`.

A device MUST refuse a restart of a service it did not list.

`restart-refused` MUST carry `reason`, in the device's own words, saying why.

A device MUST serve any number of service streams at once, on one channel or across several.

> [!NOTE]
> The selector is critical because a restart is an act a device asked for must have been read in full before it is carried out, as the `act` of [CTL](power.md) is, and unlike a subscription's unknown topic there is no safe way to carry out a restart of a service the device does not understand.

## Carrying it out

Once it has accepted a restart, a device MUST stop the named service and start it again, and MUST NOT close the channel or any stream to do so.

A device MUST reflect the restart in the service's `service` fact on the `default` feed, as [NFO](../device-info.md) has it, so a client watching the feed sees the service leave its running state and return to it.

A device MUST go on serving every other feature while a service restarts, and a service restarting MUST NOT stop the device answering on any stream.

A device MUST log every restart asked for, with whether it was accepted, and the `name` and `version` the asking client gave in its `hello`.

A device MUST log a restart it accepted and then could not carry out, with the reason it failed.

> [!NOTE]
> A restart accepted is not a restart done: the client says the service is restarting until the feed shows it running again, as it says an act is under way under [WEB](../web-app.md), because nothing on the service stream reports the service back up.

## Message types

| type | sent by | on | carries |
| --- | --- | --- | --- |
| `services` | client | a service stream, as the first message | nothing beyond `type` |
| `restartable` | device | the service stream | `services` |
| `restart` | client | the service stream | `SERVICE` |
| `restarting` | device | the service stream | nothing beyond `type` |
| `restart-refused` | device | the service stream | `reason` |
