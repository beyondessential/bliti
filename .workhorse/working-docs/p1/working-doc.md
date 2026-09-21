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

**A trait stands alone, or it sits inside the one it qualifies.**
A trait is its own member only where it means something independently of the others.
`direction` is independent: any flow measurement has one, with or without an interface.
A modem's radio technology is not — it describes the modem, so it belongs inside the modem trait rather than beside it.

The same test applied to the readings a device already reports moves two things: the default route and the overlay describe the interface, and the block device describes the filesystem.

```json
"traits": { "interface": { "name": "eth0", "route": "default" } }
"traits": { "filesystem": { "mount": "/boot/firmware", "device": "mmcblk0p1" } }
```

This is the `detail`-versus-reading question one level down, and it takes the same answer: a thing that stands on its own is its own member, and a thing that only qualifies another belongs inside it.

**Where the device holds a piece of information, it sends it rather than leaving it implied.**
The overlay trait carries `"tailscale"`, not `true`.
The device knows which overlay it is; `true` throws that away and encodes only that there is one.

This says nothing about what shape a trait value takes.
A trait carries whatever the data is.

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

The order is fixed and does not move.
A reading in `warn` or `fault` stays where it sits and is found by colour, which is why the face colours the value rather than adding an element to carry it.

A layout that rearranged under an operator while they were looking at it would cost the screen its familiarity, and a device with several marginal readings would reshuffle as they crossed back and forth.

What the device *is*, then what it is *doing*:

| position | from |
| --- | --- |
| header | the `hostname`, `board` and `os` facts |
| tiles | `network-address`, then `cpu-usage`, `memory-usage`, `filesystem-usage`, `network-throughput`, `temperature`, `throttling`, `fan-speed`, `power-source`, `battery-charge`, `last-boot` |
| appended | everything unrecognised, generically, in the order received |

The measurement and fact names here are drafted, not settled; naming the catalogue is NFO's job.

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

### `kind` names the value's type, and the vocabulary is open

`kind` is not a four-way choice between `text`, `quantity`, `fraction` and `duration`.
It names what the value is, and an address is `ipv4`.

Nothing carries an address family, because the kind already says it.

**The fallback for an unrecognised kind has to change.**
Today an unrecognised kind makes the value unreadable and the client renders the label alone.
That was written for a closed set where the risk was drawing a number whose scale the client did not know.
With an open vocabulary it is too harsh: a client that has never heard of `ipv4` would show nothing, where showing the string verbatim would have been correct.

A client that does not recognise a kind renders a string value as text.
A string carries its own meaning; a number without its scale does not, so an unrecognised kind with a numeric value stays unreadable.

### Units are spelled out; abbreviating them is the client's

A unit is named in full on the wire — `bytes`, `bits`, `bytes/second`, `celsius`, `volts` — and the client writes it however it writes units.

An abbreviation is presentation, and an ambiguous one is worse than none: `B/s` and `bps` differ by a factor of eight and are routinely written for each other.
Spelling the unit out removes the ambiguity from the wire rather than relying on both ends reading the same abbreviation the same way.

Choosing a readable magnitude is the client's too: the wire carries `1200000 bytes/second` and the screen says `1.2 MB/s`.

A client that does not recognise a unit writes it out as it was sent, which is correct if ungainly.

### A fraction and a total, and nothing else

A quantity that fills something is reported as a `-usage` fraction and a `-total` fact.
Used bytes, free bytes and percentages are arithmetic on those two, so only those two cross the wire.

The split also falls where the data does: the fraction is what moves and is compact, the total is a fact that does not move.

### No supply reading until W1

The device reads an undervoltage alarm bit today and no voltage, so there is nothing for `state-reason: undervoltage` to attach to.
W1 establishes what the board's power management actually yields.

Until it lands the device reports no supply reading rather than a bare alarm, and no `boolean` kind is added: a kind introduced for one valueless reading is worth avoiding when a real number may be available instead.

### `state-reason` names why a state is not ok

`error` says why a reading has no value.
`state-reason` says why a reading that has a value is in trouble.

```json
{ "type": "reading", "at": 20308140, "measurement": "cpu-frequency",
  "state": "warn", "state-reason": "throttled",
  "kind": "quantity", "unit": "hertz", "value": 1500000000 }
```

It is a name, not prose: the client holds the wording as it holds every other label.

This is what throttling is. A capped processor is the frequency reading carrying a reason, not a separate measurement beside it, and the distinction ordinary idle scaling would otherwise blur — a low frequency on a device that is merely not busy — is carried without a second measurement to reconcile against the first.

**`note` leaves the wire.**
It was prose the device sent for a client to display, and too generic to act on: a client could show it and nothing more.
Everything it carried is either catalogue knowledge the client already holds — that a temperature is the processor core rather than the case — or wording keyed to a `state-reason`, which names what the trouble is rather than describing it.

### What a trait is

**A trait is a dimension you can aggregate across.**
It slices one measurement into instances, and re-combining those instances — summing, ranking, comparing — means something.

| trait | slices | re-combining gives |
| --- | --- | --- |
| `interface` | throughput per link | total traffic, by summing |
| `direction` | throughput per way it runs | total traffic, by summing |
| `filesystem` | usage per mount | the fullest, by ranking |
| `sensor` | temperature per probe | the hottest, by ranking |

Most traits also name a subject you could point at — a link, a mount, a probe.
`direction` does not, which is why naming a subject is a useful habit rather than the test.

Drafting the catalogue rejected two candidate traits by this test.
`part` (used against total) re-combines to nothing, a fraction and a byte count being neither summable nor rankable; it gave itself away by reading as `used` against memory and `free` against filesystems, one name for opposite quantities.
`condition` (undervoltage against speed-capped) re-combines to nothing either, being two unrelated questions rather than one question about two subjects.

Both were two measurements wearing one name, and both take two names instead.

### Uptime becomes the instant of boot

`last-boot` carries a datetime; the client subtracts to show an uptime.
The boot instant does not change, where an uptime changes every second and is a subtraction away from it.

This needs a clock, which `at` being boot-relative exists precisely because a device may not have.
A device that cannot answer for its boot instant omits `last-boot`, under the same rule as any other reading its hardware and operating system cannot answer for — which makes a missing clock visible where an uptime quietly hid it.

A clock that is set but wrong is a different problem, and not this card's: V1 carries detecting device drift and telling it apart from a drifted client.

### The generic fallback

Both types need one, and it is the same rule with the parts a fact does not have removed.

For a reading with no matching rule: title-case the `measurement` for the label, render the trait values as the qualifier, draw the value by its `kind`.
State still colours it and it still accumulates a history, because neither depends on recognising the measurement.

For a fact with no matching rule: the same, from `fact` rather than `measurement`, without state, scale or history.
An unknown fact is a tile rather than a header entry — the header is the facts the client knows name the device.

**The traits have to be rendered, not merely ignored.**
Two modems share a measurement and differ only in traits the client cannot read.
Dropping them puts two tiles on screen under one label with different numbers and nothing to tell them apart.
This is the display counterpart of the identity rule below, and it has the same cause.

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

- [ ] The measurement and fact catalogue, drafted at `.workhorse/design/mockups/p1/catalogue.html`

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
