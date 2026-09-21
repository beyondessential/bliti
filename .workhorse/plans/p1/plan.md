# Make the feed model structural, and move presentation out of the wire

Rebases on D1 and H1, both merged. The specs are [MSG](../../specs/messages.md) and [NFO](../../specs/device-info.md); NFO is the rename of what was SYS in `system-info.md`.

## Shape of the change

Three things move at once, and they are hard to land separately because each makes the others' shape obvious:

1. **The reading mechanism moves from NFO into MSG.** `system-identity` / `system-sample` / `system-history` were feature-shaped versions of something every feature wants. What replaces them — `fact`, `reading`, traits, identity, kinds — is base framework, and NFO keeps only the catalogue and the rendering rules.
2. **One message per reading, with no batching.** This is what removes the sample boundary, and with it the "a sample need not carry every reading" rule and the series shape.
3. **Presentation leaves the wire.** `group`, ordering, `label`, `note`, `graph` and `detail` all go. The client gains an order, a label table, a wording table and a set of aggregation rules.

## Why this shape

**Traits rather than nesting.** The card asked for nesting where data nests. Drafting it showed nesting needs a path to key a series by, and cannot address anything that stayed in `detail`. Measurement-plus-traits is the metrics model: identity is structural, every value is addressable at one level, and grouping is a query the client runs rather than a string it buckets. `group` and a compound identifier both failed for the same reason — a string the client has to parse.

**A `traits` container rather than reserved base member names.** Identity must include traits the receiver cannot read, or a sender that adds a trait splitting one series into several makes an older receiver merge them into one wrong graph. A container needs nothing reserved; reserved base names would work too, but then adding a base member later is a version change.

**Dimensional by default, descriptive by exception.** A trait counts toward identity unless the receiver knows it does not. This fails safe — an unrecognised trait splits a series rather than merging two — and it makes identity the receiver's to compute rather than a wire absolute, which is the same division as everything else on this card. The exception list exists because some traits genuinely vary: the default route moves between interfaces, and an application that let `route` distinguish would fork an interface's graph each time the route left it, for a reason that has nothing to do with throughput.

**Aggregation is the test for a trait.** A trait slices one catalogue entry into instances that can be summed, ranked or compared. Two candidates failed it while drafting and became separate entries instead: `part` (used against total) and `condition` (undervoltage against speed-capped). Both were two measurements wearing one name.

**`state-reason` rather than boolean conditions.** Throttling is why the frequency reading is in trouble, not a measurement beside it. This also carries a distinction a derived boolean could not: `cpu-frequency < cpu-frequency-max` is true whenever the processor is idle.

## Rejected

**Backfill.** Removed entirely rather than reshaped, and raised as U1. It was holding several open questions together — own topic or second stream, pushed or pulled, newest-first ordering against a client also receiving live data — none of which had to be answered to get the rest right, and answering them badly would have set the feed model's shape around a feature nobody had asked for yet. The feed model is deliberately **not** shaped to leave room for it.

**Two-way readings as a feature.** The merged `Message` set and the no-op rule make a client reporting readings possible and safe. Nothing sends them yet; wall-clock time and RSSI are later cards.

**A `boolean` kind.** Wanted only by a valueless undervoltage alarm, which W1 may replace with a real voltage. Not added on spec.

## Build order

Core first, because both ends depend on the types.

- [ ] Merge `ClientMessage` and `DeviceMessage` into one `Message`, with one `hello` and one criticality table
- [ ] Replace `Reading`/`Series`/`Value` in `channel/readings.rs` with one shape carrying `at`, catalogue name, `traits`, `kind`, `unit`, `value`, `state`, `state-reason`, `limits`, `error`; `fact` and `reading` differ by which catalogue names them and by a fact carrying no state
- [ ] Inline the value: `kind` as an open string, `unit` alongside, `value` as raw JSON rather than a tagged enum
- [ ] Identity as catalogue name plus every trait the client does not know to be descriptive, with equality and hashing over it, and `route` and `overlay` on the descriptive list
- [ ] Round numeric values to four decimal places on send
- [ ] Delete `system-identity`, `system-sample`, `system-history` and the series shape

Device next.

- [ ] Rework `facts/` to the catalogue: one entry per fact or reading, traits instead of `detail`, no `group`, no `label`, no `note`
- [ ] Split what was aggregated: `network-throughput` per interface and direction; `filesystem-usage` per filesystem with the `boot` role marked; `temperature` per sensor
- [ ] Replace `uptime` with `last-boot`, omitted where the device cannot answer for the instant
- [ ] Replace the `throttling` prose summary with `cpu-frequency` carrying `state-reason: throttled`, and drop the undervoltage alarm until W1
- [ ] Spell units out
- [ ] Open the `default` feed unprompted after `hello`, on its own stream; serve `subscribe` for `default` as the resume path; skip a `subscribe` for a topic already being served
- [ ] Keep sampling running across a decline

Client last, since it needs real messages to render.

- [ ] Hold the fixed order, the label table, the wording table for `state-reason`, and unit abbreviation and magnitude
- [ ] Render bespoke what it recognises; render the rest generically, with trait values as the qualifier
- [ ] Aggregation rules: fullest non-boot filesystem, summed throughput, `cpu` sensor, default-route and overlay addresses
- [ ] Per-interface mirrored graphs, with each reading's own `limits`, `state-reason` and error reason in the reveal
- [ ] Key histories by identity, and hold none for a `fact`
- [ ] Decline the feed when the page is hidden and resume with `subscribe`

## Watch for

**The two D1 defects should dissolve rather than need fixing.** The `network` group collision cannot recur because there is no `group`; the opposed-pair reveal dropping its members' detail cannot recur because a mirrored pair is two readings each rendered in full. If either needs code written specifically to address it, the model has not been applied properly.

**Identity equality is the subtle one.** It must compare unrecognised traits, which means comparing raw JSON rather than a parsed struct, minus an explicit descriptive list. A build that parsed traits into known fields and compared those would pass every test written against today's catalogue and fail in the field the first time a device gained a trait. A build that forgot the descriptive list would fork an interface's graph whenever the default route moved.

**`at` stays boot-relative.** `last-boot` is the one datetime, and it is a fact the device may not be able to answer for. Nothing else gains a wall clock.
