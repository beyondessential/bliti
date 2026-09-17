---
id: BLI-MSG
---

# Application messages

The channel of [BLI-CHN](channel.md) carries application messages inside a common envelope, whatever the feature.
This spec is that envelope: how a message is delimited and shaped, which stream carries what, how the two ends name themselves, what each does with what it does not recognise, and how live data is subscribed to.
A feature spec describes the message types it adds and inherits everything here.

Everything in this spec is wire contract.
An independently written client that follows it interoperates with a device that follows it, and the two keep interoperating as each gains messages the other has never heard of.

## How a message is delimited

A message is a JSON object encoded as UTF-8, prefixed with its length as four bytes, big-endian, giving the number of bytes of JSON that follow.
There is no trailing newline and no padding, and the next message's prefix begins at the byte after the last byte of JSON.

This delimiting sits inside the framing of [BLI-CHN](channel.md) and is not the same thing.
That framing delimits Noise messages on the link; this delimits application messages within one yamux stream.
The two use the same encoding at different layers, and a receiver that conflates them reads nonsense.

A message is at most 128 kibibytes of JSON.
A receiver sent a longer one MUST treat it as a fault.

The ceiling is set by the link rather than by JSON.
The channel of [BLI-CHN](channel.md) runs over BLE, where a message is carried as a stream of small notifications, and one large enough to take thousands of them denies the connection to everything else for as long as it takes.
A feature with more to say than this sends it in several messages.

## How a message is shaped

Every message is a JSON object carrying a `type` member whose value is a string naming the message type.
Type names are lower case, with words separated by hyphens.

Every other member sits alongside `type` in the same object rather than nested under a payload member.

## Member names, and which members are critical

A member name is made of lower case letters, digits and hyphens, and is written either wholly in lower case or wholly in upper case.
The two forms name the same member: names are matched without regard to case, so `topic` and `TOPIC` are one member and not two.
A name in mixed case names nothing, and a message carrying one is malformed, as is a message carrying the same name twice whatever the case of each.

The case is what marks a member critical, after the convention X.509 and JWT use for extensions and header parameters.

An upper case name marks the member **critical**.
A receiver that does not recognise it must not act on the object carrying it; acting on the rest would mean acting on a partial reading of something the sender has said cannot be partially read.

A lower case name marks the member ignorable.
A receiver that does not recognise it passes over it and reads the rest.

The convention applies to member names alone and never to values: a `topic` of `system` and a `topic` of `SYSTEM` are different topics.
It holds for every object in a message at any depth.

## Which stream carries what

A stream's role is established by which end opened it and by the first message on it, never by its identifier.
yamux allocates identifiers, and neither end attaches meaning to a particular number.

There are three roles.

The **device's reporting stream** is opened by the device as soon as the handshake completes, without being asked.
Its first message is `device-hello`.
Every later message on it is data the device sends once and sends again when what it reports changes.

The **client's control stream** is opened by the client as soon as the handshake completes.
Its first message is `client-hello`.
Every later message on it is something a feature gives a client to send.

A **subscription stream** is opened by the client, one per subscription, as described below.

Each end opens its own stream and sends its first message without waiting for the other, so neither blocks on the other and the order in which the two arrive carries no meaning.

Streams are bidirectional, so a device may reply on the stream a client opened.

## Both ends name themselves

`client-hello` and `device-hello` each carry two members beyond `type`:

| member | type | meaning |
| --- | --- | --- |
| `name` | string | what the software calls itself |
| `version` | string | the version it is at |

Both are opaque.

The client SHOULD display the device's `name` and `version`.
The device MUST log the client's.

## Version skew is ordinary

A device runs software months behind the web application, because the application is served fresh each time and the device is not.
An installed client inverts this, running against a device that has since been updated.
Both are the normal case rather than a fault, and neither may reduce the operator to an error message: the point of the system is that someone standing in front of a broken device can reach it.

The base protocol version of [BLI](overview.md) is the only version anything acts on, and it is settled from the advertisement before a channel exists.
Once a channel is open nothing in it is gated: no message type, no member and no feature is withheld or refused on the grounds of what software the other end reported running.
No message exists by which one end tells the other it is unwilling to continue.

## What the base protocol guarantees

Both ends are at the same base protocol version before a channel exists, so each may hold the other to this spec.

Every message is therefore valid UTF-8, is valid JSON, is a JSON object, and carries a `type` member whose value is a string.
Every member name in it is well formed and appears once.
A message whose type the receiver recognises carries the members that type required when it was defined, each as the JSON type given for it.

A message that breaks any of this is not a version difference, because a version difference cannot produce one.
It is a fault in the peer, and it MUST be reported rather than passed over.

The receiver closes the stream the message arrived on, and leaves the connection and every other stream alive, so whatever else the peer can still say keeps arriving.
A message beyond the size ceiling above is a fault of the same kind and is handled the same way.

## How a message type grows

A later version of either end adds members to a type rather than removing or repurposing them, and the case it names them in says what a receiver that has never heard of them is to do.

A member added in lower case is one the message still means something without, and an older receiver passes over it and reads the rest.
A member added in upper case is one the message does not mean anything without, and an older receiver refuses the message and says so rather than acting on a reading the sender has told it is incomplete.

This is what removes the need for a type's members to be closed when it is defined, or for a version to be carried inside each type.
A sender that must be understood says so on the member itself, and a receiver that predates it finds out without either end comparing a version.

The members a type required when it was defined stay required and keep their meaning.
Removing one, or changing what one means, is a change to the base protocol version.

## A critical member the receiver does not know

A receiver that meets an unrecognised critical member does not act on the object carrying it.

Where that object is the message, the message is not processed.
Where it is nested within the message, the receiver treats that part as unusable and reads the rest, which is what lets a client show every reading in a report but one.

The receiver reports it where reports belong: the device logs it, and the client tells the operator that the device has said something this version of the application is too old to act on.

This is not a fault in the peer and is not treated as one.
The stream stays open and the connection is untouched, and the receiver goes on handling and displaying everything else it does understand.
A client behind a device is the ordinary case, and an operator is better served by most of a view, plainly marked as partial, than by none of it.

The `device-hello` and `client-hello` message types MUST NOT contain any critical member.

The `subscribe` message type MUST contain exactly one critical member, `TOPIC`, and MUST NOT contain any other.
A subscription is a request to be sent something, and a device that acted on one whose selector it had not read would send something other than what was asked for.
Marking the selector critical is what says so on the wire.

## What is not recognised is skipped

Everything else a receiver does not recognise it passes over, and carries on:

- a message whose `type` names a type the receiver does not recognise: the message is skipped whole
- an ignorable member the receiver does not recognise: the member is skipped and the rest of the object is read
- a value the receiver does not recognise where a feature spec says an unrecognised one is skipped, of which a `subscribe` for an unknown topic is the one this spec defines

Skipping is silent on the wire.
Nothing is sent in reply, the stream stays open, and the connection is untouched.
A receiver may record what it skipped.

A sender that needs a whole message not to be passed over in silence names its `type` member in upper case.
That makes the type itself critical, and a receiver that does not know the type reports it rather than skipping it.

A stream whose first message is skipped is a stream whose role the receiver never learns.
It stays open and carries nothing until the end that opened it closes it.
That is what a device older than the client looks like, and it fails nothing.

## Subscribing

What a device sends once, it pushes: its hello and its static data go on the reporting stream unasked.
What a device sends continuously flows only while a client is subscribed to it.

A client subscribes by opening a stream whose first message is `subscribe`, carrying one critical member beyond `type`:

| member | type | meaning |
| --- | --- | --- |
| `TOPIC` | string | what is being subscribed to |

The device sends that topic's data on that same stream, for as long as the stream is open.

A client unsubscribes by closing the stream it opened.

Closing ends the client's sending side, and the device reads end of stream on its own side.
That is the unsubscribe: on reading it the device stops sending the topic, closes its side in turn, and keeps nothing for that subscription.
A device that read end of stream and carried on sending would leave a client receiving data it has said it no longer wants, which is the case this whole mechanism exists to prevent.

A subscription ends whenever its stream ends, however it ends.
A stream closed gracefully, a stream reset, a client that goes away without a word, a dropped link and a closed connection are each the unsubscribe, and none is a fault either end reports.
The subscription lasts exactly as long as its stream, so there is no subscription a device must time out and no state the two ends can disagree about.

Data already in flight when the stream ends may still arrive.
A client discards it rather than treating it as a fault.

A client opens one stream per subscription, so subscribing to one topic and unsubscribing from another are independent of each other.

A device that does not recognise a topic skips the `subscribe` as it skips anything else it does not recognise, and sends nothing on the stream.
The client sees a subscription that yields no data, which is what a device older than the client looks like, and nothing fails.

A client drops its subscriptions when the page is hidden, and opens them again when it is shown.
This is what keeps a phone in a pocket from pulling samples over a link nobody is reading.

Topic names, and what a device sends on a subscription to each, are defined by the feature spec that owns them.

## The message types this spec defines

| type | sent by | on |
| --- | --- | --- |
| `client-hello` | client | its control stream, as the first message |
| `device-hello` | device | its reporting stream, as the first message |
| `subscribe` | client | a subscription stream, as the first message |

Every other message type belongs to the feature spec that defines it.
