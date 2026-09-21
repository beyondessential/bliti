---
id: MSG
---

# Application messages

The channel of [CHN](channel.md) carries application messages inside a common envelope, whatever the feature.
A feature spec adds message types and topics, and inherits everything here.

## Borrowed terms

| term | meaning |
| --- | --- |
| JSON | The interchange format of [RFC 8259](https://www.rfc-editor.org/rfc/rfc8259), which requires UTF-8 for text exchanged between systems. |
| stream | A yamux stream within the channel, as [CHN](channel.md) specifies: either end opens one, and closing one leaves the others alive. |
| end of stream | What a reader observes once the other end has closed its sending side and no more data will arrive on it. |
| critical | A member a receiver must understand before acting on the object carrying it, after the convention X.509 extensions and JWT header parameters use. |
| topic | What an end subscribes to, named by a string. |
| feed | A stream serving a topic. |

## How a message is delimited

A message MUST be a JSON object encoded as UTF-8, prefixed with its length as three bytes, big-endian, giving the number of bytes of JSON that follow.

A message MUST NOT carry a trailing newline or padding, and the next message's prefix MUST begin at the byte after the last byte of JSON.

> [!NOTE]
> This delimiting sits inside the framing of [CHN](channel.md) and is not the same thing: that framing delimits Noise messages on the link, while this delimits application messages within one stream. The two prefixes are of different widths, so code that reads one cannot be code that reads the other.

## How a message is shaped

Every message MUST carry a `type` member whose value is a string naming the message type.

A type name MUST be lower case, with words separated by hyphens.

Every other member MUST sit alongside `type` in the same object, and MUST NOT be nested under a payload member.

## Member names, and which are critical

A member name MUST be made of lower case letters, digits and hyphens, and MUST be written either wholly in lower case or wholly in upper case.

Names MUST be matched without regard to case, so `topic` and `TOPIC` name one member and not two.

A message carrying a mixed-case name, or carrying the same name twice whatever the case of each, MUST be treated as malformed.

An upper case name marks the member critical.
A receiver that does not recognise a critical member MUST NOT act on the object carrying it.

A lower case name marks the member ignorable.
A receiver that does not recognise an ignorable member MUST pass over it and read the rest.

The convention applies to member names alone and never to values, and it holds for every object in a message at any depth.

> [!NOTE]
> Acting on the rest of an object carrying an unrecognised critical member would mean acting on a partial reading of something the sender has said cannot be partially read.

## Both ends speak the same message set

There is one set of message types, and both ends send and receive from it.

A message's direction MUST be established by which end opened the stream it arrived on, and never by the message type.

An end that receives a message type it recognises but has nothing to do about MUST treat that as a no-op.
It MUST NOT treat it as a fault, MUST leave the stream open, and MUST go on handling everything else.

> [!NOTE]
> Without this rule an implementer could read one shared message set as making an unexpected-but-known type a protocol violation, which is the one place a type-enforced direction split would otherwise have helped.
> Readings travel both ways: an end may report what its peer cannot measure about itself.

## Which stream carries what

A stream's role MUST be established by which end opened it and by the first message on it, and never by its identifier.

| role | opened by | first message | carries |
| --- | --- | --- | --- |
| hello stream | each end, as soon as the handshake completes | `hello` | nothing further required |
| feed | an end, to serve a topic its peer has not asked for | the topic's data | that topic's data |
| subscription | an end, to ask for a topic | `subscribe` | that topic's data |

Each end MUST open its hello stream and send `hello` without waiting for the other.

> [!NOTE]
> yamux allocates identifiers and neither end attaches meaning to a particular number.
> Because neither end waits, the order in which the two hellos arrive carries no meaning. Streams are bidirectional, so an end may reply on a stream its peer opened.

## Both ends name themselves

`hello` MUST carry two members beyond `type`:

| member | type | meaning |
| --- | --- | --- |
| `name` | string | what the software calls itself |
| `version` | string | the version it is at |

Both values are opaque.

An end SHOULD display or log its peer's `name` and `version`, and MUST NOT act on either.

`hello` MUST NOT contain a critical member.

`hello` MUST be sent on a stream of its own, so that an end declining everything else it is being sent does not cost it its peer's identity and version.

> [!NOTE]
> These values are the software version each end runs, which [VER](version.md) requires nothing to act on.

## What the base protocol guarantees

Both ends are at the same version marker before a channel exists, so each may hold the other to this spec.

Every message is therefore valid UTF-8, is valid JSON, is a JSON object, and carries a `type` member whose value is a string; every member name in it is well formed and appears once; and a message whose type the receiver recognises carries the members that type required when it was defined, each as the JSON type given for it.

A message that breaks any of that MUST be treated as a fault in the peer and MUST be reported rather than passed over.

On such a fault the receiver MUST close the stream the message arrived on, and MUST leave the connection and every other stream alive.

> [!NOTE]
> A version difference cannot produce a malformed message, which is why one is a fault rather than skew.

## How a message type grows

A later version of either end MUST add members to a type rather than removing or repurposing them.

The members a type required when it was defined MUST keep their meaning.
Removing one, or changing what one means, is a change to the version marker of [VER](version.md).

> [!NOTE]
> The case a new member is named in says what a receiver that has never heard of it does: a lower case member is one the message still means something without, and an upper case member is one it does not.
> That is what removes the need for a type's members to be closed when it is defined, or for a version to be carried inside each type.

## A critical member the receiver does not know

Where the object carrying an unrecognised critical member is the message, the receiver MUST NOT process the message.

Where it is nested within the message, the receiver MUST treat that part as unusable and read the rest.

The receiver MUST report it: a device by logging it, a client by telling the operator that its peer has said something this version is too old to act on.

The receiver MUST NOT treat this as a fault in the peer, MUST leave the stream open, and MUST go on handling everything else it understands.

> [!NOTE]
> A client behind a device is the ordinary case, and an operator is better served by most of a view, plainly marked as partial, than by none of it.

## What is not recognised is skipped

A receiver MUST pass over and carry on from:

- a message whose `type` names a type it does not recognise, skipping the message whole
- an ignorable member it does not recognise, reading the rest of the object
- a value it does not recognise where a feature spec says an unrecognised one is skipped, of which a `subscribe` for an unknown topic is the one this spec defines

Skipping MUST be silent: the receiver MUST send nothing in reply, MUST leave the stream open, and MUST leave the connection untouched.
A receiver MAY record what it skipped.

A sender that needs a whole message not to be passed over in silence MUST name its `type` member in upper case, making the type itself critical.

> [!NOTE]
> A stream whose first message is skipped is a stream whose role the receiver never learns. It stays open and carries nothing until the end that opened it closes it, which is what an end older than its peer looks like, and it fails nothing.

## Topics and feeds

An end MUST serve a topic on at most one stream.

An end MAY open a feed for a topic its peer has not asked for, and MUST serve exactly the topic `default` on such a feed.

A feed MUST NOT announce the topic it serves.

> [!NOTE]
> There is one topic an end may push, so a pushed stream has only one thing it can be, and a peer resuming it already knows the name to ask for.

A peer declines a feed by closing the stream, and MUST NOT be required to send anything to decline it.

On reading end of stream the serving end MUST stop sending that topic, MUST close its side in turn, and MUST keep nothing for that stream.

An end MUST go on gathering what it would have sent after a decline, so that a peer which comes back is served what is current rather than what has accumulated since.

### Subscribing

An end MUST subscribe by opening a stream whose first message is `subscribe`, carrying exactly one critical member beyond `type` and no other:

| member | type | meaning |
| --- | --- | --- |
| `TOPIC` | string | the topic being subscribed to |

The receiving end MUST send that topic's data on that same stream for as long as the stream is open.

The receiving end MUST skip a `subscribe` for a topic it is already serving, so that a peer cannot receive one topic twice.

An end MUST unsubscribe by closing the stream it opened, which ends the subscription exactly as it ends a feed.

A subscription ends whenever its stream ends, however it ends, and its ending MUST NOT be reported as a fault by either end.

An end MUST discard data that was already in flight when the stream ended, rather than treat it as a fault.

An end MUST open one stream per subscription.

A client SHOULD drop its subscriptions when the operator is no longer looking, and open them again when they are.

Topic names, and what an end sends on a subscription to each, are defined by the feature spec that owns them.

> [!NOTE]
> A subscription is a request to be sent something, and an end that acted on one whose selector it had not read would send something other than what was asked for. Marking the selector critical is what says so on the wire.
> The subscription lasting exactly as long as its stream is what leaves no subscription to time out and no state the two ends can disagree about. A stream closed gracefully, a stream reset, a peer that goes away without a word, a dropped link and a closed connection are each the unsubscribe.
> An end that does not recognise a topic skips the `subscribe` as it skips anything else, and the subscriber sees a subscription that yields no data.

## Message types

| type | sent by | on |
| --- | --- | --- |
| `hello` | each end | its hello stream, as the first message |
| `subscribe` | an end | a subscription stream, as the first message |

Every other message type belongs to the feature spec that defines it.
