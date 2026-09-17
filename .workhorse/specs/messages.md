---
id: BLI-MSG
---

# Application messages

The channel of [BLI-CHN](channel.md) carries application messages inside a common envelope, whatever the feature.
This spec is that envelope: how a message is delimited and shaped, which stream carries what, how the two ends name themselves, how each skips what it does not recognise, and how live data is subscribed to.
A feature spec describes the message types it adds and inherits everything here.

Everything in this spec is wire contract.
An independently written client that follows it interoperates with a device that follows it, and the two keep interoperating as each gains messages the other has never heard of.

## How a message is delimited

A message is a JSON object encoded as UTF-8, prefixed with its length as four bytes, big-endian, giving the number of bytes of JSON that follow.
There is no trailing newline and no padding, and the next message's prefix begins at the byte after the last byte of JSON.

This delimiting sits inside the framing of [BLI-CHN](channel.md) and is not the same thing.
That framing delimits Noise messages on the link; this delimits application messages within one yamux stream.
The two use the same encoding at different layers, and a receiver that conflates them reads nonsense.

A message is at most one mebibyte of JSON.
A receiver sent a longer one treats it as the fault it is, as described below, rather than buffering it.

## How a message is shaped

Every message is a JSON object carrying a `type` member whose value is a string naming the message type.
Type names are lower case, with words separated by hyphens.

Every other member sits alongside `type` in the same object rather than nested under a payload member.

Where this spec or a feature spec gives a member's JSON type, a sender sends that type and no other.
A member is never sent as a different JSON type to mean the same thing, because a receiver that does not recognise the member cannot know what the substitution meant.

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
Neither end parses, compares, or orders the other's `name` or `version`, and nothing either end does depends on what it was told.
They exist to be read by a person and written to a log.

The client displays the device's `name` and `version`, because "what is this thing running" is a question an operator standing in front of a device has.
The device records the client's, which is how it can be known what is actually in the field talking to these devices.

Their being opaque is what keeps this from becoming the version gate below.
A value with no structure the other end reads is a value the other end cannot branch on.

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
A message whose type the receiver recognises carries the members that type requires, each as the JSON type given for it.

A message type's required members are fixed for the life of a base protocol version.
A later version of either end adds members rather than removing or repurposing them, and adds message types rather than reshaping existing ones, which is what makes the guarantee above safe to rely on.
A change that cannot be made that way is a change to the base protocol version.

A message that breaks any of this is not a version difference, because a version difference cannot produce one.
It is a fault in the peer, and it is reported rather than passed over: the device logs it, and the client surfaces it to the operator, because a device that is not speaking the protocol is something the person standing in front of it needs told rather than left to read a blank screen.

The receiver closes the stream the message arrived on, and leaves the connection and every other stream alive, so whatever else the peer can still say keeps arriving.
A message beyond the size ceiling above is a fault of the same kind and is handled the same way.

## What is not recognised is skipped

Within that floor, a receiver skips what it does not know and carries on:

- a message whose `type` names a type the receiver does not recognise: the message is skipped whole
- a member the receiver does not recognise within a message whose type it does: the member is skipped and the rest of the message is read
- a value the receiver does not recognise where a feature spec says an unrecognised one is skipped, of which a `subscribe` for an unknown topic is the one this spec defines

Skipping is silent on the wire.
Nothing is sent in reply, the stream stays open, and the connection is untouched.
A receiver may record what it skipped.

This holds in both directions, and it is what lets either end gain a message type or a member while the other has never heard of it, with no release coordinated between them.

A stream whose first message is skipped is a stream whose role the receiver never learns.
It stays open and carries nothing until the end that opened it closes it.
That is what a device older than the client looks like, and it fails nothing.

## Subscribing

What a device sends once, it pushes: its hello and its static data go on the reporting stream unasked.
What a device sends continuously flows only while a client is subscribed to it.

A client subscribes by opening a stream whose first message is `subscribe`, carrying one member beyond `type`:

| member | type | meaning |
| --- | --- | --- |
| `topic` | string | what is being subscribed to |

The device sends that topic's data on that same stream, for as long as the stream is open.

A client unsubscribes by closing the stream.
The subscription lasts exactly as long as the stream, so a client that goes away without a word, a page closed or a link dropped, has unsubscribed by doing so, and a device is left holding no subscription it must later time out.

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
