---
id: MSG
---

# Application messages

The channel of [CHN](channel.md) carries application messages inside a common envelope, whatever the feature. A feature spec adds message types and inherits everything here.

## Borrowed terms

| term | meaning |
| --- | --- |
| JSON | The interchange format of [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259), which requires UTF-8 for text exchanged between systems. |
| stream | A yamux stream within the channel, as [CHN](channel.md) specifies: either end opens one, and closing one leaves the others alive. |
| end of stream | What a reader observes once the other end has closed its sending side and no more data will arrive on it. |
| critical | A member a receiver must understand before acting on the object carrying it, after the convention X.509 extensions and JWT header parameters use. |

## How a message is delimited

A message MUST be a JSON object encoded as UTF-8, prefixed with its length as three bytes, big-endian, giving the number of bytes of JSON that follow.

A message MUST NOT carry a trailing newline or padding, and the next message's prefix MUST begin at the byte after the last byte of JSON.

> [!NOTE]
> This delimiting sits inside the framing of [CHN](channel.md) and is not the same thing: that framing delimits Noise messages on the link, while this delimits application messages within one stream. The two prefixes are of different widths, so code that reads one cannot be code that reads the other.
> A three-byte prefix expresses exactly what a receiver can be asked to buffer, so an over-long message is unrepresentable and no rule is needed to refuse one. The width is a bound rather than an invitation: a feature with bulk to send carries it as many messages on a stream of its own, which is what leaves the connection usable for everything else while it does.

## How a message is shaped

Every message MUST carry a `type` member whose value is a string naming the message type.

A type name MUST be lower case, with words separated by hyphens.

Every other member MUST sit alongside `type` in the same object, and MUST NOT be nested under a payload member.

## Member names, and which are critical

A member name MUST be made of lower case letters, digits and hyphens, and MUST be written either wholly in lower case or wholly in upper case.

Names MUST be matched without regard to case, so `topic` and `TOPIC` name one member and not two.

A message carrying a mixed-case name, or carrying the same name twice whatever the case of each, MUST be treated as malformed.

An upper case name marks the member critical. A receiver that does not recognise a critical member MUST NOT act on the object carrying it.

A lower case name marks the member ignorable. A receiver that does not recognise an ignorable member MUST pass over it and read the rest.

The convention applies to member names alone and never to values, and it holds for every object in a message at any depth.

> [!NOTE]
> A `topic` of `system` and a `topic` of `SYSTEM` are different topics.
> Acting on the rest of an object carrying an unrecognised critical member would mean acting on a partial reading of something the sender has said cannot be partially read.

## Which stream carries what

A stream's role MUST be established by which end opened it and by the first message on it, and never by its identifier.

There are three roles:

| role | opened by | first message | carries |
| --- | --- | --- | --- |
| reporting stream | the device, as soon as the handshake completes | `device-hello` | data the device sends once and again whenever it changes |
| control stream | the client, as soon as the handshake completes | `client-hello` | what a feature gives a client to send |
| subscription stream | the client, one per subscription | `subscribe` | the subscribed topic's data |

Each end MUST open its own stream and send its first message without waiting for the other.

> [!NOTE]
> yamux allocates identifiers and neither end attaches meaning to a particular number.
> Because neither end waits, the order in which the two first messages arrive carries no meaning. Streams are bidirectional, so a device may reply on the stream a client opened.

## Both ends name themselves

`client-hello` and `device-hello` MUST each carry two members beyond `type`:

| member | type | meaning |
| --- | --- | --- |
| `name` | string | what the software calls itself |
| `version` | string | the version it is at |

Both values are opaque.

A client SHOULD display the device's `name` and `version`. A device MUST log the client's.

`client-hello` and `device-hello` MUST NOT contain a critical member.

> [!NOTE]
> These values are the software version each end runs, which [VER](version.md) requires nothing to act on.

## What the base protocol guarantees

Both ends are at the same version marker before a channel exists, so each may hold the other to this spec.

Every message is therefore valid UTF-8, is valid JSON, is a JSON object, and carries a `type` member whose value is a string; every member name in it is well formed and appears once; and a message whose type the receiver recognises carries the members that type required when it was defined, each as the JSON type given for it.

A message that breaks any of that MUST be treated as a fault in the peer and MUST be reported rather than passed over.

On such a fault the receiver MUST close the stream the message arrived on, and MUST leave the connection and every other stream alive.

> [!NOTE]
> A version difference cannot produce a malformed message, which is why one is a fault rather than skew.
> Leaving the rest alive is what keeps whatever else the peer can still say arriving.

## How a message type grows

A later version of either end MUST add members to a type rather than removing or repurposing them.

The members a type required when it was defined MUST keep their meaning. Removing one, or changing what one means, is a change to the version marker of [VER](version.md).

> [!NOTE]
> The case a new member is named in says what a receiver that has never heard of it does: a lower case member is one the message still means something without, and an upper case member is one it does not.
> That is what removes the need for a type's members to be closed when it is defined, or for a version to be carried inside each type. A sender that must be understood says so on the member itself, and a receiver that predates it finds out without either end comparing a version.

## A critical member the receiver does not know

Where the object carrying an unrecognised critical member is the message, the receiver MUST NOT process the message.

Where it is nested within the message, the receiver MUST treat that part as unusable and read the rest.

The receiver MUST report it: a device by logging it, a client by telling the operator that the device has said something this version of the application is too old to act on.

The receiver MUST NOT treat this as a fault in the peer, MUST leave the stream open, and MUST go on handling everything else it understands.

> [!NOTE]
> Reading the rest is what lets a client show every reading in a report but one.
> A client behind a device is the ordinary case, and an operator is better served by most of a view, plainly marked as partial, than by none of it.

## What is not recognised is skipped

A receiver MUST pass over and carry on from:

- a message whose `type` names a type it does not recognise, skipping the message whole
- an ignorable member it does not recognise, reading the rest of the object
- a value it does not recognise where a feature spec says an unrecognised one is skipped, of which a `subscribe` for an unknown topic is the one this spec defines

Skipping MUST be silent: the receiver MUST send nothing in reply, MUST leave the stream open, and MUST leave the connection untouched. A receiver MAY record what it skipped.

A sender that needs a whole message not to be passed over in silence MUST name its `type` member in upper case, making the type itself critical.

> [!NOTE]
> A stream whose first message is skipped is a stream whose role the receiver never learns. It stays open and carries nothing until the end that opened it closes it, which is what a device older than the client looks like, and it fails nothing.

## Subscribing

A client MUST subscribe by opening a stream whose first message is `subscribe`, carrying exactly one critical member beyond `type` and no other:

| member | type | meaning |
| --- | --- | --- |
| `TOPIC` | string | what is being subscribed to |

The device MUST send that topic's data on that same stream for as long as the stream is open.

A client MUST unsubscribe by closing the stream it opened. On reading end of stream the device MUST stop sending the topic, MUST close its side in turn, and MUST keep nothing for that subscription.

A subscription ends whenever its stream ends, however it ends, and its ending MUST NOT be reported as a fault by either end.

A client MUST discard data that was already in flight when the stream ended, rather than treat it as a fault.

A client MUST open one stream per subscription.

A client SHOULD drop its subscriptions when the operator is no longer looking, and open them again when they are.

Topic names, and what a device sends on a subscription to each, are defined by the feature spec that owns them.

> [!NOTE]
> A subscription is a request to be sent something, and a device that acted on one whose selector it had not read would send something other than what was asked for. Marking the selector critical is what says so on the wire.
> The subscription lasting exactly as long as its stream is what leaves no subscription for a device to time out and no state the two ends can disagree about. A stream closed gracefully, a stream reset, a client that goes away without a word, a dropped link and a closed connection are each the unsubscribe.
> One stream per subscription is what makes subscribing to one topic and unsubscribing from another independent.
> A device that does not recognise a topic skips the `subscribe` as it skips anything else, and the client sees a subscription that yields no data.

## Message types

| type | sent by | on |
| --- | --- | --- |
| `client-hello` | client | its control stream, as the first message |
| `device-hello` | device | its reporting stream, as the first message |
| `subscribe` | client | a subscription stream, as the first message |

Every other message type belongs to the feature spec that defines it.
