---
status: draft
---

# Make the feed model structural, and move presentation out of the wire

Rework the diagnostics wire so it carries data rather than presentation: strip `group` and ordering, fold identity into samples, make the `default` feed device-pushed with `subscribe` as the resume path, and merge the two message enums into one.

## Behaviour

### The framing

The wire carries data; the client carries intelligence.
Generic rendering is the floor that makes version skew survivable, not the target.
A client holds bespoke renderers for the readings it knows, in its own order and under its own labels, and a generic reading card for everything else.

### What leaves the wire

`group` goes.
Order goes: the sequence readings arrive in carries no meaning, and a client renders what it recognises where it wants and appends the rest.
`label` stays, but only as raw material for the generic renderer.
`direction` stays, because which way a flow runs is a fact.

### What arrives

Nesting, where the data genuinely nests.
An interface's in and out are two facts about one interface.
Exact shape is open — see the nesting question below.

### One message type for readings

`system-identity`, `system-sample` and `system-history` collapse into one type carrying a timestamp and readings.
Static facts are readings that rarely change, carrying `graph: false`; they are no longer a distinct message.

The mechanism moves into the base framework (MSG), leaving NFO as a catalogue of which readings a device reports.

### Feeds

A topic is what an end subscribes to; a feed is the stream serving it.
`subscribe` keeps its critical `TOPIC` selector.

There is exactly one pushed feed, serving the topic `default`.
That is the name a client resumes under, which is why it needs no first-message announcement: there is only one thing a pushed stream can be.
The device opens it unprompted on connection and starts sending.
A pushed feed does not name itself: being pushed is what makes it the default feed.

The client declines by closing the stream.
The client resumes with `subscribe`, which survives as the resume path and as the way to reach feeds too expensive to push unasked.

A feed is served on at most one stream: a `subscribe` for one already being served is skipped, so a client cannot get two copies.

No feed catalogue is needed.
A client subscribing to a feed an older device never heard of hits the existing rule: unknown topic, stay silent, leave the stream open.

`hello` stays on its own stream, so declining the readings does not cost the device's identity and version.

Sampling keeps running after a close, so a resume picks up live rather than waiting for the next tick.

### One `Message`, one `hello`

`ClientMessage` and `DeviceMessage` merge into one set, with direction established by which end opened the stream.
One `hello`; the `client-`/`device-` prefix discriminates nothing.
One criticality table.

**Receiving a known message with nothing to do about it is a no-op, not a fault.**
This is stated explicitly: without it, an implementer could read "both ends share one message set" as making an unexpected-but-known type a protocol violation.

Feed and `subscribe` rules are written as "an end" and "a peer".
`default` is a role either end may fill; the device is the one that currently does.

### No backfill

History is not sent.
Graphs fill forward from the moment a client connects.
`system-history` and the series shape go entirely.

The feed model is **not** shaped around backfill's eventual return.
U1 carries that work and is free to change whatever it has to.

### The order a client renders in

Proposed, to be corrected: what the device *is* first, then what it is *doing*, then what it is *running on*.

Identity as a header rather than tiles: hostname, board, OS.
Then tiles: address, processor, memory, storage, network, temperature, throttling, fan, power source, battery, uptime.
Then everything the client does not recognise, generically, in the order received.

### What the client owns

NFO carries both halves: the catalogue of readings a device reports, and which of them a client renders bespoke and in what order.

The spec becomes **NFO**, after the `.nfo` file: the thing you open to find out what a machine is.
Its file becomes `device-info.md`, which parallels `board-id.md` and `key-schedule.md` and uses the product's own word for the thing.
Every other spec keeps its filename.

Cross-references in the specs, the `(SYS)` citations in rustdoc, and the `BLI-SYS` mentions in the client's comments all follow the id.

The network tile's face carries a single combined traffic figure.
The per-direction split and the per-interface breakdown are a tap away, which is where a diagnostic belongs and where it is more useful anyway.

### The two D1 defects

Addresses and throughput become separate tiles rather than colliding on a `network` group.
Per-interface throughput becomes structural rather than inert `detail`, so it can carry state, an error reason and a mirrored graph of its own.
A reveal shows each reading's own `detail`, `note`, `limits` and error reason, including for the members of a mirrored pair.

## Implementation options

### Nesting shape (open)

Three options, mocked up at `.workhorse/design/mockups/p1/nesting-options.html` against the same device at the same moment.

1. **`detail` becomes readings.** One mechanism; every sub-value is a reading with a name, state, graph and children. Disk's filesystems, memory's totals and each address all gain a scale, a colour and a history.
2. **`readings` alongside `detail`.** A sub-fact that stands on its own is a child reading; a figure that only qualifies its parent stays in `detail`.
3. **Nesting only where it is a fact.** Same two mechanisms, bar set high: only an interface's in and out nest.

### Rounding

Sample values round to 4 decimal places, which nearly halves the compressed size.
`Sampler::numeric` already does this for series; live samples currently carry raw floats.

### Scheduling

yamux has no priority, and none is wanted.
Separate streams, with the device scheduling its own writes within CHN's 200 notifications/s budget.

## Trade-offs

**Two-way readings are structural only on this card.**
The merged `Message` set makes a client reporting readings possible, and the no-op rule makes it safe, but nothing actually sends readings client-to-device yet.
Wall-clock time and RSSI are the obvious first consumers and are deliberately left to a later card.

**Dropping backfill costs the populated graph**, and U1 is raised to bring it back.
D1 sent the buffered window on subscribing so a graph was populated the moment it appeared.
Without it a graph fills from connection forward, and an operator who has just opened the view sees an empty one for the first few samples.
Taken knowingly: it removes the series shape, the newest-first backfill stream, the second feed, and the whole question of how a backfill stream is addressed.
It is a staging decision rather than a permanent one, but the feed model is shaped without regard to it, so U1 starts from what backfill actually needs rather than from room left for it.

The device's sampling buffer is not removed with it, because battery direction is derived from the movement of cell voltage across that history where no hardware power-loss signal exists.

## Open questions

- [ ] Which nesting option, of the three mocked up
- [ ] What the client's preferred order actually is, and whether state reorders it
- [ ] Whether `hostname`, `board` and `os` render as a header rather than as tiles

## Testing notes

- A client that declines the pushed feed by closing the stream still holds the device's name and version from `hello`
- A client that resumes with `subscribe` after declining receives live samples without a second copy arriving
- A `subscribe` for the feed already being served is skipped, and the client does not receive two copies
- A known message type arriving at an end with nothing to do about it is a no-op: the stream stays open and nothing is reported as a fault
- A failed network direction shows its error reason in the reveal
- Per-interface throughput draws a mirrored graph per interface
- Addresses and throughput render as separate tiles
- A reading whose `name` the client does not recognise renders generically and is appended after the ones it does
- The network tile's face carries one combined figure, and the per-direction split appears on tap
