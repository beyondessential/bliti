---
id: NET
---

# Network configuration

A client configures a device's network over the channel of [CHN](../channel.md): the networks it attaches to, the hotspot it runs, the addresses it holds and the resolvers it queries.

This spec covers the configuration document and the rules every part of it obeys.
[LINK](attachment.md) specifies how a device attaches to a network, [WLAN](wireless.md) which wireless networks it may join, [HOT](hotspot.md) the hotspot, and [CFG](session.md) the exchange a client configures through.
[NSCR](screen.md) specifies what our client makes of all of it.

## The document

A device's network configuration MUST be one declarative document covering the whole device.

A client MUST configure a device by sending that document whole, and a device MUST make its running configuration match what the document describes.

Applying a document a device is already running MUST change nothing.

> [!NOTE]
> One document for the device rather than one per interface is what lets the ordering of [LINK](attachment.md) span links, which an ordering held inside any one of them could not express.
> Sending it whole leaves a client nothing to sequence, and makes a repeated write a no-op, which is what lets a client read, edit and write back without tracking what it touched.

## What an absent member means

An absent entity MUST be read as removed: a device MUST forget a wireless network the document does not carry.

An absent setting MUST be read as unset, and a device MUST supply its own behaviour for it.

> [!NOTE]
> The two readings are opposite, and both are needed. A hotspot carrying no band is one the device chooses a band for, not one with no band.

## The document's members

| member | type | required | meaning |
| --- | --- | --- | --- |
| `attachments` | array | yes | the ordered candidates of [LINK](attachment.md) |
| `hotspot` | object | no | the hotspot of [HOT](hotspot.md); absent means the device runs none |
| `regulatory-domain` | string | no | the domain the radio operates under, as an ISO 3166-1 alpha-2 code |

A device whose `regulatory-domain` is unset MUST restrict its radio to what every domain permits.

> [!NOTE]
> The operator is the party standing in the country, and a device imaged elsewhere has no other way to learn it.
> Restricting an unset device to the intersection leaves it legal wherever it is switched on, at the cost of channels it could have used.

## Capabilities

A device MUST state what it supports when a configuration session opens, as [CFG](session.md) specifies.

A device MUST treat a document asking for anything it did not state as invalid, and MUST NOT apply any part of such a document.

A device MUST NOT apply a document in part, MUST NOT report a setting as accepted but not in force, and MUST NOT run a configuration that differs from the document it accepted.

A device MUST omit from its capabilities anything the platform beneath it cannot carry out.

> [!NOTE]
> A client that has been told what a device supports has no reason to ask for more, which is what removes partial application and the unhonoured setting as outcomes, and what makes the document's declarative reading true rather than aspirational.
> The rule reaches into what a client draws: a setting absent from a device's capabilities is one the client does not offer, rather than one it offers and the device quietly overrides. A setting whose fate depended on something that changes while nobody is watching would be the worst kind to offer.

## Secrets

A device MUST report the secrets its configuration holds in full, including pre-shared keys, enterprise credentials and the hotspot's passphrase.

> [!NOTE]
> The channel is authenticated and encrypted under [CHN](../channel.md), and a client holding the presence token has already established that it is physically at the device.
> Withholding a secret from such a client protects nothing, and would make writing back what was read a special case rather than the ordinary path.

## The model is the contract

The document MUST be the contract a device is held to, and a device MUST map it onto whatever configures its network.

> [!NOTE]
> A document that deferred to the facilities of a particular network stack would constrain no re-implementation, and would change meaning whenever the image beneath it changed.
