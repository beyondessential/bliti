---
id: BLI-MSG
---

# Application messages

The application messages of [BLI-CHN](channel.md) are carried in a common envelope, whatever the feature.
This spec describes that envelope: how the two ends name themselves, how each tolerates what it does not recognise, and how live data is subscribed to rather than pushed.
Every feature carried over the channel is framed this way, so a feature spec describes the messages it adds and inherits the rest from here.

## Both ends name themselves

The device reports its own software version among the data it sends on connect, and the client displays it, because "what is this thing running" is a question an operator in front of a device has.

The client names itself and its version to the device, which records it, so it can be known what is actually in the field talking to these devices.

Neither end changes what it does based on what the other reported.
The exchange is for the operator reading a screen and for the record a device keeps; nothing in either end branches on the other's identity or version.

## Version skew is ordinary

A device runs software months behind the web application, because the application is served fresh each time and the device is not.
An installed client inverts this, running against a device that has since been updated.
Both are the normal case, not a fault, and neither may reduce the operator to an error message: the whole point of the system is that someone standing in front of a device can reach it.

Nothing in the protocol refuses to proceed on the grounds of the other end's version.
There is no version gate anywhere in the conversation.

## Unknown messages and fields are skipped

Each end ignores a message whose type it does not recognise, and ignores fields it does not recognise within a message it does, and carries on in both cases.
This holds in both directions: a device passes over what a client sends that it does not know, and a client passes over what a device sends that it does not know.

This is what lets one end gain a message type or a field while the other end has never heard of it, with no coordinated release between them.
An unrecognised message is not reported back as an error and does not close the channel; it is simply skipped.

## Push and subscribe

The data a device sends divides into what it sends once and what it sends continuously.

Its identity and its software version are pushed once on connect, unsolicited, as the device speaking first over the channel.

Live data flows only while a client is subscribed to it.
A client subscribes to begin receiving live updates and unsubscribes to stop, rather than being pushed at unconditionally.

A client drops its subscription while the operator is not looking, following the browser's page-visibility signal, and restores it when they return.
This is what keeps a phone in a pocket from pulling a stream of samples over the BLE link it can no longer be read on.

What a subscription covers, and what live data a given feature carries, is described by that feature's spec; this spec describes only that live data is reached by subscribing and released by unsubscribing.
