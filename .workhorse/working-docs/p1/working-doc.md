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

### What arrives: measurements and traits

A reading is not a named thing with a value.
It is a **measurement type** plus a set of **traits** describing what that measurement is about.

```json
{
  "type": "reading",
  "at": 20308140,
  "measurement": "network-throughput",
  "traits": { "interface": "eth0", "direction": "in" },
  "state": "ok",
  "kind": "quantity",
  "unit": "B/s",
  "value": 1200000
}
```

`measurement` names the reading against a catalogue of measurement types.
Traits sit in a `traits` container and say what this measurement is about: that it concerns an interface, which one, and which way the flow runs.

A trait may be a bare value where there is one thing to say, and an object where there is more.

`detail` dissolves.
What sat inside it becomes readings in its own right, distinguished by a trait: each filesystem carries a `filesystem` trait, memory's used and total carry theirs, each address its interface.
That is what makes them addressable, graphable, and able to carry a state of their own, which is the defect the card names for per-interface throughput.

There is no identifier.
A reading's identity — and so its series — is its measurement together with its `traits` object wholesale.
Nothing on the wire is a string to be parsed, which is what `group` was and what a compound id would have quietly become.

Traits are ignorable by case, so a client that has never heard of `interface` renders the measurement generically and is still correct.
The `traits` container is what lets identity be computed over traits the client cannot read: nothing has to be reserved, and the object compares wholesale.

### The client selects rather than looks up

A client's render rule is a query over measurements and traits, not a name lookup.
"One tile for network throughput" is: select every `network-throughput` reading, group by `interface`, pair by `direction`, aggregate the headline by summing.

This is what separates the trait model from `group`.
`group` was an opaque string the client bucketed and hoped about; traits are structured and the client knows their schema, so selecting on them is reading data rather than re-deriving presentation.

### One message per reading

Batching is gone.
Every reading is its own message carrying its own `at`, sent at whatever cadence suits what it measures.

There is no sample boundary, so nothing has to decide what a snapshot contains, and "a sample need not carry every reading" stops needing saying.

Two message types:

| type | names itself with | carries | has |
| --- | --- | --- | --- |
| `fact` | `fact` | something true about the device | no state, no history |
| `reading` | `measurement` | a measurement | state, and a history worth keeping |

The `graph` flag is replaced by the type itself.

The two catalogues overlap freely.
A `network-addresses` fact and a `network-addresses` reading are different things, and neither has to avoid the other's name.

The value is inlined rather than nested in a `value` object: `kind` says how to read `value`, and `unit` accompanies it where the kind wants one.

`system-identity`, `system-sample` and `system-history` all go.
The mechanism moves into the base framework (MSG), leaving NFO as the catalogue of measurement types and traits, and of which the client renders bespoke.

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

### Device judgements become client rules

Writing the messages out showed how far this reaches beyond the readings the card names.

The device currently picks which address to headline: it prefers the interface carrying the default route, and IPv4 over IPv6, because that is the one a person can read out.
Those are presentation decisions taken on the device.
Under traits the device states what makes them decidable — `route: default`, `family: ipv4`, `overlay: true` — and the client applies its own rule.

### Where a note about an aggregate goes

`detail` dissolving leaves nothing for some notes to attach to.
"Filesystem use does not move fast enough for a graph to say anything" was a note on the `disk` reading, and there is no longer a `disk` reading — only one reading per filesystem and a client-side aggregate.

A note about what a measurement type *means* is catalogue knowledge and belongs to the client, alongside the label it already supplies.
A note about *this particular* reading is data and stays on the wire, as a derived battery direction's caveat does.

Mocked up at `.workhorse/design/mockups/p1/measurements-and-traits.html`.

### Series identity must include traits the client does not know

If a newer device adds a trait that splits one series into several — per-queue throughput under an existing interface, say — a client that ignores the unknown trait merges them and draws a garbled graph.

So identity includes every trait, recognised or not.
An older client then shows two series it cannot fully tell apart, which is degraded but correct, rather than one series that is wrong.

Making such traits critical instead would cost the older client the reading entirely, which is worse.

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
