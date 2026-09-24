---
id: VIEW
---

# Device view

The device view is the screen an operator reads a device from: the facts and readings of [NFO](device-info.md), rendered by our application.

[NFO](device-info.md) says what a device sends, and binds every implementation.
This spec says what ours makes of it, and binds nothing else that reads the same data.

> [!NOTE]
> The wire carries data and the client carries the intelligence. Holding the rendering here is what lets our application know the catalogue intimately without that knowledge becoming a rule every reader of a bliti device has to follow.

## Everything is rendered

The application MUST render every fact and reading it receives, whether or not it recognises the catalogue name.

The application MUST NOT report a fact or reading as missing.

> [!NOTE]
> The application has no list of expected entries to compare against, so one a device never sent is not an absence it can observe.

## What it recognises

The application MUST hold its own order, and MUST NOT take one from the order data arrives in.

The application MUST render in this order:

| position | from |
| --- | --- |
| a header naming the device | `hostname`, `board` with `board-revision`, `os`, `kernel` |
| a notice beneath the header, only while `provisional` | `network-configuration` |
| tiles | `network-address`, `wireless-network`, `hotspot`, `hotspot-clients`, `cpu-usage`, `memory-usage`, `filesystem-usage`, `network-throughput`, `temperature`, `cpu-frequency`, `fan-speed`, `power-source`, `battery-charge`, `last-boot` |
| within another entry's reveal | `memory-total`, `filesystem-total`, `cpu-frequency-max`, `battery-voltage`, `battery-direction` |
| appended | everything it does not recognise |

The application MUST NOT give an entry it renders within another's reveal a tile of its own.

While `network-configuration` is `provisional`, the application MUST say that the device is trying network settings that revert unless they are confirmed, and MUST mark the `network-address`, `wireless-network`, `hotspot` and `hotspot-clients` tiles as provisional.
The application MUST NOT give `network-configuration` a tile.

The application MUST NOT reorder by the `status` trait.

The application MUST supply its own wording for every catalogue name, trait, distinguishing trait value and unit it recognises, and MUST choose for itself how to write a unit and at what magnitude to show a value.

The application MUST render a `reason` as the sender wrote it.

The application MUST render `last-boot` as an elapsed time.

> [!NOTE]
> A layout that rearranged while an operator was looking at it would cost the screen its familiarity, and a device with several marginal readings would reshuffle as they crossed back and forth. Trouble is found by colour instead.
> A `reason` is the sender's own words about something the application did not anticipate, so there is no wording of its own to supply.

## What it does not

The application MUST render a catalogue name it does not recognise from the name itself, with the values of its traits as a qualifier, its value drawn by its `kind`, and `unit` where there is one.

The application MUST render the traits of an entry it does not recognise, and MUST NOT drop them.

The application MUST render an unrecognised fact as a tile rather than in the header.

The application MUST render an entry whose `kind` it does not recognise as the stringification of its `value`, followed by `unit` where there is one.

> [!NOTE]
> Two entries sharing a catalogue name and differing only in traits would otherwise appear as two tiles under one label with different values and nothing to tell them apart.
> Generic rendering is what makes version skew survivable: a device ahead of an installed application degrades to a plain reading card, and renders properly again when the application is next updated.

## The face and the reveal

The application MUST show each tile's face carrying a label and a headline value, and nothing else.

The application MUST reveal what is behind the headline, any scale drawn as a bar, and any history drawn as a graph, when the operator opens the tile.

The application MUST colour a face by its entry's `status`, MUST NOT colour a `passed` face, and MUST NOT add a further element to the face to carry that.

The application MUST show an entry whose `status` is `skipped` or `broken` as having no value, with its reason, and MUST distinguish the two.

The application MUST remove an entry's tile, and any history drawn for it, when the entry is sent as `ended`.

The application MUST show every reading in a reveal with its own limits, status reason and scale.

> [!NOTE]
> An open tile wants the full width, because figures and graphs are not read through half a column.
> A reveal that rendered a summary of its members in place of the members themselves would drop exactly the part an operator opened it for: which one is in trouble, and why.

## Aggregates and scales

The application MUST headline `filesystem-usage` with the fullest filesystem whose `filesystem` trait does not carry the `boot` role, and MUST show each filesystem in the reveal against its `filesystem-total`.

The application MUST headline `network-throughput` with the sum of every direction and interface, and MUST show each interface in the reveal.

The application MUST headline `temperature` with the `cpu` sensor, and MUST show every sensor in the reveal.

The application MUST headline `network-address` with at most two addresses, each on a line of its own and without its interface:

- on the interface carrying the `default` route, or on any interface other than an overlay where none is named as carrying it, the first held of an IPv4 address, a global IPv6 address and a unique local IPv6 address;
- on an interface naming an overlay, where one does, the first it holds of an IPv4 address and a global IPv6 address.

The application MUST show every address in the reveal, each with its interface.

The application MUST draw `cpu-frequency` against `cpu-frequency-max`.

The application MUST show `memory-total` in the reveal of `memory-usage`.

The application MUST headline `battery-charge` with a single battery, choosing the one named `built-in` where a device reports one and the first by `battery` name otherwise, and MUST show every battery in the reveal.

The application MUST pair each battery's `battery-voltage` and `battery-direction` with its `battery-charge` by the `battery` trait, and MUST show them in that battery's reveal.

The application MUST draw a `fraction` against its own scale, and MUST NOT draw a `quantity` against a scale unless its `limits` trait or its total above gives it one.

> [!NOTE]
> An interface commonly holds an IPv4 address and several IPv6 ones, so headlining them all crowds the tile. One address for the network the device is reached on and one for the overlay are what an operator reads it for, and an overlay's addresses are recognisable without naming it.

## Graphs

The application MUST hold a history for each `reading` it receives, and MUST NOT hold one for a `fact`.

The application MUST key that history by catalogue name together with every trait [NFO](device-info.md) does not name as descriptive, and MUST treat a trait it does not recognise as part of that key.

The application MUST show that history as a graph in the reveal.

The application MUST NOT draw a history for `filesystem-usage`.

The application MUST draw the two directions of one interface's `network-throughput` as a single graph mirrored about a shared time axis, one direction above it and the other below.

The application MUST scale each direction to its own peak, and MUST state each peak beside its line together with which line it belongs to.

The application MUST space readings by their `at` values rather than evenly.

> [!NOTE]
> A device newer than the application may add a trait that splits one series into several. Keying on a trait it cannot read leaves it two series it cannot fully tell apart, which is degraded and true; merging them would draw one series that is wrong, and it stops splitting them when it learns better.
> Two directions of throughput routinely differ by an order of magnitude, and a shared scale flattens the quieter one to a line.
> Filesystem use does not move fast enough over a session for a graph to say anything.

## Subscribing

The application MUST let the device's feed run while the operator is looking at the device.

The application SHOULD close the feed when they are not, and MUST subscribe to `default` to resume.
