# Make the feed model structural, and move presentation out of the wire

Rebases on D1 and H1, both merged. The specs are [MSG](../../specs/messages.md), [NFO](../../specs/device-info.md) and [VIEW](../../specs/device-view.md); NFO is the rename of what was SYS in `system-info.md`, and VIEW is new.

## Shape of the change

Three things move at once, and they are hard to land separately because each makes the others' shape obvious:

1. **The reading mechanism moves from NFO into MSG.** `system-identity` / `system-sample` / `system-history` were feature-shaped versions of something every feature wants. What replaces them, namely `fact`, `reading`, traits and kinds, is base framework, and NFO keeps only the catalogue.
2. **One message per reading, with no batching.** This is what removes the sample boundary, and with it the "a sample need not carry every reading" rule and the series shape.
3. **Presentation leaves the wire.** `group`, ordering, `label`, `note`, `graph` and `detail` all go, and the rendering rules leave NFO for VIEW, a spec that binds our application alone. The client gains an order, a label table, a wording table and a set of aggregation rules.

## Why this shape

**Traits rather than nesting.** The card asked for nesting where data nests. Drafting it showed nesting needs a path to key a series by, and cannot address anything that stayed in `detail`. Measurement-plus-traits is the metrics model: identity is structural, every value is addressable at one level, and grouping is a query the client runs rather than a string it buckets. `group` and a compound identifier both failed for the same reason: a string the client has to parse.

**A `traits` container rather than reserved base member names.** Identity must include traits the receiver cannot read, or a sender that adds a trait splitting one series into several makes an older receiver merge them into one wrong graph. A container needs nothing reserved; reserved base names would work too, but then adding a base member later is a version change.

**The device owes unambiguity, not identity.** NFO first defined a series identity for receivers to compute. That was the wire telling clients how to think again, one level up from `group`. What the device actually owes is that two entries about different things differ somewhere in what is sent; how a client groups them is its own. Two addresses on one interface meet this through their `kind`, so no address-family trait is needed.

**Dimensional by default, descriptive by exception.** The distinction survives as a property of each trait, because it is the test for whether something deserves to be a trait at all, and because VIEW keys its series on it. A trait counts unless NFO names it descriptive, which fails safe, since an unrecognised trait splits a series rather than merging two. The exception list exists because some traits genuinely vary: the default route moves between interfaces, and an application that let `route` distinguish would fork an interface's graph each time the route left it, for a reason that has nothing to do with throughput.

**Aggregation is the test for a trait.** A trait slices one catalogue entry into instances that can be summed, ranked or compared. Two candidates failed it while drafting and became separate entries instead: `part` (used against total) and `condition` (undervoltage against speed-capped). Both were two measurements wearing one name.

**Status is a trait, and a free-text reason sits inside it.** Throttling is why the frequency reading is in trouble, not a measurement beside it, and this carries a distinction a derived boolean could not: `cpu-frequency < cpu-frequency-max` is true whenever the processor is idle.

**`status` is the datum's, not the device's.** The trait is named for what it describes: whether this piece of data is sound. That is what absorbed `error`, which is now `broken`, and it is why every entry carries a status while a busy link is still no kind of warning: `passed` against throughput says the figure is good, not that the link is quiet. Only `warning` and `failed` are held back for measurements with a notion of difficulty.

**The vocabulary is bestool's.** `passed`, `warning`, `failed`, `skipped`, `broken`, verified against `crates/alertd/src/check.rs`, where the wire strings are the inflected forms and every non-`passed` variant already carries free text. An operator meets the same five words here as in a doctor run. It also buys the `skipped` / `broken` distinction for free: a measurement this platform cannot make is not one that should have worked and did not.

**`reason` is free text.** The useful part of a failure is the part nobody anticipated: a path, a permission, an errno. The hyphenated codes (`throttled`, `derived`, `backup-bypassed`, `against-source`) are gone; NFO requires the warning and says what the reason must convey, and the device writes it.

## Rejected

**Backfill.** Removed entirely rather than reshaped, and raised as U1. It was holding several open questions together (own topic or second stream, pushed or pulled, newest-first ordering against a client also receiving live data), none of which had to be answered to get the rest right, and answering them badly would have set the feed model's shape around a feature nobody had asked for yet. The feed model is deliberately **not** shaped to leave room for it.

**Two-way readings as a feature.** The merged `Message` set and the no-op rule make a client reporting readings possible and safe. Nothing sends them yet; wall-clock time and RSSI are later cards.

**A `boolean` kind.** Wanted only by a valueless undervoltage alarm, which W1 may replace with a real voltage. Not added on spec.

## Build order

Core first, because both ends depend on the types.

- [ ] Merge `ClientMessage` and `DeviceMessage` into one `Message`, with one `hello` and one criticality table
- [ ] Replace `Reading`/`Series`/`Value` in `channel/readings.rs` with one shape carrying `at`, catalogue name, `traits`, `kind`, `unit`, `value`; status and limits are traits, and `fact` and `reading` differ only by which catalogue names them
- [ ] Inline the value: `kind` as an open string, `unit` alongside, `value` as raw JSON rather than a tagged enum
- [ ] Drop `error`: a `status` trait of `is` plus free-text `reason`, over `passed` / `warning` / `failed` / `skipped` / `broken`, present on every entry, with `value` present for the first three and absent for the last two
- [ ] Round numeric values to four decimal places on send
- [ ] Delete `system-identity`, `system-sample`, `system-history` and the series shape

Device next.

- [ ] Rework `facts/` to the catalogue: one entry per fact or reading, traits instead of `detail`, no `group`, no `label`, no `note`
- [ ] Report `skipped` where a precondition was not met and `broken` where the measurement errored, each with a reason worth reading
- [ ] Split what was aggregated: `network-throughput` per interface and direction; `filesystem-usage` per filesystem with the `boot` role marked; `temperature` per sensor
- [ ] Replace `uptime` with `last-boot`, omitted where the device cannot answer for the instant
- [ ] Replace the `throttling` prose summary with `cpu-frequency` reported as `warning`, and drop the undervoltage alarm until W1
- [ ] Set the value vocabularies: `via-backup` / `battery` / `bypassing-backup` for `power-source`, and `charging` / `discharging` / `idle` for `battery-direction`
- [ ] Spell units out
- [ ] Open the `default` feed unprompted after `hello`, on its own stream; serve `subscribe` for `default` as the resume path; skip a `subscribe` for a topic already being served
- [ ] Keep sampling running across a decline, holding only what a derivation needs and nothing for replay

Client last, since it needs real messages to render.

- [ ] Hold the fixed order, the label table, and unit abbreviation and magnitude; render `reason` as the device wrote it
- [ ] Render bespoke what it recognises; render the rest generically, with trait values as the qualifier
- [ ] Aggregation rules: fullest non-boot filesystem, summed throughput, `cpu` sensor, default-route and overlay addresses
- [ ] Per-interface mirrored graphs, with each reading's own `limits` and status reason in the reveal
- [ ] Key histories by catalogue name plus every non-descriptive trait, comparing unrecognised traits raw, and hold none for a `fact`
- [ ] Decline the feed when the page is hidden and resume with `subscribe`

## Watch for

**The two D1 defects should dissolve rather than need fixing.** The `network` group collision cannot recur because there is no `group`; the opposed-pair reveal dropping its members' detail cannot recur because a mirrored pair is two readings each rendered in full. If either needs code written specifically to address it, the model has not been applied properly.

**Series keying is the subtle one**, and it now lives in the client alone. It must compare unrecognised traits, which means comparing raw JSON rather than a parsed struct, minus an explicit descriptive list. A build that parsed traits into known fields and compared those would pass every test written against today's catalogue and fail in the field the first time a device gained a trait. A build that forgot the descriptive list would fork a graph whenever the default route moved or a reading went to `warning`.

**`at` stays boot-relative.** `last-boot` is the one datetime, and it is a fact the device may not be able to answer for. Nothing else gains a wall clock.
